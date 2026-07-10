use proc_macro2::Span;
use quote::ToTokens;
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, Expr, ExprCall, ExprMethodCall, File as SynFile, FnArg, ImplItem, Pat, Type,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchableOrigin {
    Signed,
    Privileged,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchableParam {
    pub name: String,
    pub is_unbounded_vec: bool,
}

#[derive(Clone)]
pub struct Dispatchable {
    pub name: String,
    pub span: Span,
    pub origin: DispatchableOrigin,
    pub params: Vec<DispatchableParam>,
    pub is_deprecated: bool,
    weight_expr: Option<Expr>,
}

impl Dispatchable {
    pub fn unbounded_vec_params(&self) -> impl Iterator<Item = &DispatchableParam> {
        self.params.iter().filter(|param| param.is_unbounded_vec)
    }

    pub fn has_max_weight(&self) -> bool {
        self.weight_expr
            .as_ref()
            .map(|expr| compact_tokens(expr).contains("Weight::MAX"))
            .unwrap_or(false)
    }

    pub fn weight_accounts_for_param(&self, param_name: &str) -> bool {
        let Some(expr) = &self.weight_expr else {
            return false;
        };

        struct ParamLengthVisitor<'a> {
            param_name: &'a str,
            found: bool,
        }

        impl<'ast> Visit<'ast> for ParamLengthVisitor<'_> {
            fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
                if matches!(
                    node.method.to_string().as_str(),
                    "len" | "encoded_size" | "using_encoded"
                ) && expr_root_ident(&node.receiver).as_deref() == Some(self.param_name)
                {
                    self.found = true;
                }
                visit::visit_expr_method_call(self, node);
            }
        }

        let mut visitor = ParamLengthVisitor {
            param_name,
            found: false,
        };
        visitor.visit_expr(expr);
        visitor.found
    }
}

pub fn collect_dispatchables(ast: &SynFile, test_mask: &[bool]) -> Vec<Dispatchable> {
    struct DispatchableVisitor<'a> {
        test_mask: &'a [bool],
        dispatchables: Vec<Dispatchable>,
    }

    impl<'ast> Visit<'ast> for DispatchableVisitor<'_> {
        fn visit_item_impl(&mut self, item_impl: &'ast syn::ItemImpl) {
            if !has_attr(&item_impl.attrs, &["pallet", "call"])
                || is_masked_span(self.test_mask, item_impl.span())
            {
                return;
            }

            for impl_item in &item_impl.items {
                let ImplItem::Fn(method) = impl_item else {
                    continue;
                };
                if !has_attr(&method.attrs, &["pallet", "call_index"])
                    || is_masked_span(self.test_mask, method.span())
                {
                    continue;
                }

                let params = method
                    .sig
                    .inputs
                    .iter()
                    .filter_map(dispatchable_param)
                    .collect::<Vec<_>>();

                self.dispatchables.push(Dispatchable {
                    name: method.sig.ident.to_string(),
                    span: method.sig.ident.span(),
                    origin: detect_origin(&method.block),
                    params,
                    is_deprecated: has_attr_ending_with(&method.attrs, "deprecated"),
                    weight_expr: method
                        .attrs
                        .iter()
                        .find(|attr| attr_path_matches(attr, &["pallet", "weight"]))
                        .and_then(attr_expr),
                });
            }

            visit::visit_item_impl(self, item_impl);
        }
    }

    let mut visitor = DispatchableVisitor {
        test_mask,
        dispatchables: Vec::new(),
    };
    visitor.visit_file(ast);
    visitor.dispatchables
}

fn dispatchable_param(arg: &FnArg) -> Option<DispatchableParam> {
    let FnArg::Typed(pat_type) = arg else {
        return None;
    };

    Some(DispatchableParam {
        name: pat_ident_name(&pat_type.pat)?,
        is_unbounded_vec: type_contains_named(&pat_type.ty, &["Vec"])
            && !type_contains_named(&pat_type.ty, &["BoundedVec", "WeakBoundedVec"]),
    })
}

fn detect_origin(block: &syn::Block) -> DispatchableOrigin {
    struct OriginGuardVisitor {
        has_signed_guard: bool,
        has_privileged_guard: bool,
    }

    impl<'ast> Visit<'ast> for OriginGuardVisitor {
        fn visit_expr_call(&mut self, node: &'ast ExprCall) {
            if let Some(path) = expr_call_path(node) {
                match path_last_ident(path).as_deref() {
                    Some("ensure_signed") => self.has_signed_guard = true,
                    Some("ensure_root" | "ensure_none" | "ensure_origin") => {
                        self.has_privileged_guard = true;
                    }
                    _ => {}
                }
            }
            visit::visit_expr_call(self, node);
        }
    }

    let mut visitor = OriginGuardVisitor {
        has_signed_guard: false,
        has_privileged_guard: false,
    };
    visitor.visit_block(block);

    match (visitor.has_signed_guard, visitor.has_privileged_guard) {
        (true, false) => DispatchableOrigin::Signed,
        (false, true) => DispatchableOrigin::Privileged,
        (true, true) => DispatchableOrigin::Mixed,
        (false, false) => DispatchableOrigin::Unknown,
    }
}

fn attr_expr(attr: &Attribute) -> Option<Expr> {
    attr.parse_args::<Expr>().ok()
}

fn attr_path_matches(attr: &Attribute, segments: &[&str]) -> bool {
    let attr_segments = attr
        .path()
        .segments
        .iter()
        .map(|segment| segment.ident.to_string());
    attr_segments.eq(segments.iter().copied())
}

fn has_attr(attrs: &[Attribute], segments: &[&str]) -> bool {
    attrs.iter().any(|attr| attr_path_matches(attr, segments))
}

fn has_attr_ending_with(attrs: &[Attribute], segment: &str) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .map(|last| last.ident == segment)
            .unwrap_or(false)
    })
}

fn is_masked_span(mask: &[bool], span: Span) -> bool {
    mask.get(span.start().line.saturating_sub(1))
        .copied()
        .unwrap_or(false)
}

fn path_last_ident(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn expr_path(expr: &Expr) -> Option<&syn::Path> {
    match expr {
        Expr::Path(expr_path) => Some(&expr_path.path),
        Expr::Group(group) => expr_path(&group.expr),
        Expr::Paren(paren) => expr_path(&paren.expr),
        Expr::Reference(reference) => expr_path(&reference.expr),
        _ => None,
    }
}

fn expr_call_path(expr_call: &ExprCall) -> Option<&syn::Path> {
    expr_path(&expr_call.func)
}

fn pat_ident_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(pat_ident) => Some(pat_ident.ident.to_string()),
        Pat::Reference(reference) => pat_ident_name(&reference.pat),
        Pat::Type(pat_type) => pat_ident_name(&pat_type.pat),
        Pat::Paren(paren) => pat_ident_name(&paren.pat),
        _ => None,
    }
}

fn expr_root_ident(expr: &Expr) -> Option<String> {
    match strip_expr_wrappers(expr) {
        Expr::Path(expr_path) => path_last_ident(&expr_path.path),
        Expr::MethodCall(method_call) => expr_root_ident(&method_call.receiver),
        Expr::Field(field) => expr_root_ident(&field.base),
        _ => None,
    }
}

fn strip_expr_wrappers(mut expr: &Expr) -> &Expr {
    loop {
        expr = match expr {
            Expr::Group(group) => &group.expr,
            Expr::Paren(paren) => &paren.expr,
            Expr::Reference(reference) => &reference.expr,
            Expr::Cast(cast) => &cast.expr,
            Expr::Try(expr_try) => &expr_try.expr,
            _ => return expr,
        };
    }
}

fn compact_tokens<T: ToTokens>(node: &T) -> String {
    node.to_token_stream()
        .to_string()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn type_contains_named(ty: &Type, names: &[&str]) -> bool {
    struct TypeNameVisitor<'a> {
        names: &'a [&'a str],
        found: bool,
    }

    impl<'ast> Visit<'ast> for TypeNameVisitor<'_> {
        fn visit_type_path(&mut self, type_path: &'ast syn::TypePath) {
            if type_path
                .path
                .segments
                .last()
                .map(|segment| self.names.iter().any(|name| segment.ident == *name))
                .unwrap_or(false)
            {
                self.found = true;
            }
            visit::visit_type_path(self, type_path);
        }
    }

    let mut visitor = TypeNameVisitor {
        names,
        found: false,
    };
    visitor.visit_type(ty);
    visitor.found
}
