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
    pub is_bounded_in_body: bool,
}

#[derive(Clone)]
pub struct Dispatchable {
    pub name: String,
    pub span: Span,
    pub origin: DispatchableOrigin,
    pub params: Vec<DispatchableParam>,
    pub is_deprecated: bool,
    pub consumes_max_block: bool,
    pub body_tokens: String,
    weight_expr: Option<Expr>,
    has_impl_weight_provider: bool,
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
        // `#[pallet::call(weight = T::WeightInfo)]` delegates generated call
        // weights to the matching WeightInfo method with the dispatchable's
        // declared inputs. The source model cannot inspect that generated call,
        // but it must not treat the absence of a per-method attribute as proof
        // that every input is unaccounted for.
        if self.has_impl_weight_provider {
            return true;
        }
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

            let has_impl_weight_provider = call_attr_has_weight_provider(&item_impl.attrs);

            for impl_item in &item_impl.items {
                let ImplItem::Fn(method) = impl_item else {
                    continue;
                };
                // `call_index` is optional in FRAME. Every method in a
                // `#[pallet::call]` impl is a dispatchable, whether it uses an
                // explicit index or FRAME assigns its positional index.
                if is_masked_span(self.test_mask, method.span()) {
                    continue;
                }

                let params = method
                    .sig
                    .inputs
                    .iter()
                    .filter_map(|arg| dispatchable_param(arg, &method.block))
                    .collect::<Vec<_>>();

                self.dispatchables.push(Dispatchable {
                    name: method.sig.ident.to_string(),
                    span: method.sig.ident.span(),
                    origin: detect_origin(&method.block),
                    params,
                    is_deprecated: has_attr_ending_with(&method.attrs, "deprecated"),
                    consumes_max_block: body_consumes_max_block(&method.block),
                    body_tokens: compact_tokens(&method.block),
                    weight_expr: method
                        .attrs
                        .iter()
                        .find(|attr| attr_path_matches(attr, &["pallet", "weight"]))
                        .and_then(attr_expr),
                    has_impl_weight_provider,
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

fn dispatchable_param(arg: &FnArg, block: &syn::Block) -> Option<DispatchableParam> {
    let FnArg::Typed(pat_type) = arg else {
        return None;
    };

    let name = pat_ident_name(&pat_type.pat)?;
    if name == "origin" {
        return None;
    }
    Some(DispatchableParam {
        is_bounded_in_body: body_bounds_param(block, &name),
        name,
        is_unbounded_vec: type_contains_named(&pat_type.ty, &["Vec"])
            && !type_contains_named(&pat_type.ty, &["BoundedVec", "WeakBoundedVec"]),
    })
}

fn body_bounds_param(block: &syn::Block, param_name: &str) -> bool {
    let tokens = compact_tokens(block);
    let len_call = format!("{param_name}.len()");
    let reference = format!("&{param_name}");

    tokens.contains(&format!("{len_call}<"))
        || tokens.contains(&format!("{len_call}<="))
        || tokens.contains(&format!("({len_call}asu32)<"))
        || tokens.contains(&format!("({len_call}asu32)<="))
        || tokens.contains(&format!("({len_call}asusize)<"))
        || tokens.contains(&format!("({len_call}asusize)<="))
        || tokens.contains(&format!("try_from({param_name})"))
        || tokens.contains(&format!("try_from(&{param_name})"))
        || tokens.contains(&format!("{param_name}.try_into()"))
        || tokens.contains(&format!("decode(&mut&{param_name}[..]"))
        || (param_name.contains("signature")
            && tokens.contains("verify_")
            && tokens.contains(&reference))
        || (tokens.contains("to_text()")
            && (tokens.contains(&format!("==&{param_name}[..]"))
                || tokens.contains(&format!("&{param_name}[..]=="))))
        || (tokens.contains("ownership_proof_is_valid") && tokens.contains(&reference))
        || (tokens.contains("bound_") && tokens.contains(param_name))
        || (tokens.contains("validate_") && tokens.contains(&reference))
}

fn body_consumes_max_block(block: &syn::Block) -> bool {
    let tokens = compact_tokens(block);
    tokens.contains("Some(T::BlockWeights::get().max_block)")
        || tokens.contains("Some(<T as frame_system::Config>::BlockWeights::get().max_block)")
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
                    Some(
                        "ensure_root" | "ensure_none" | "ensure_origin" | "ensure_origin_or_root",
                    ) => {
                        self.has_privileged_guard = true;
                    }
                    _ => {}
                }
            }
            visit::visit_expr_call(self, node);
        }

        fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
            if matches!(
                node.method.to_string().as_str(),
                "ensure_origin" | "ensure_origin_or_root"
            ) {
                self.has_privileged_guard = true;
            }
            visit::visit_expr_method_call(self, node);
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
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    attr_segments
        .iter()
        .map(String::as_str)
        .eq(segments.iter().copied())
        || attr_segments
            .iter()
            .map(String::as_str)
            .rev()
            .zip(segments.iter().rev().copied())
            .all(|(actual, expected)| actual == expected)
            && attr_segments.len() >= segments.len()
}

fn call_attr_has_weight_provider(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr_path_matches(attr, &["pallet", "call"])
            // FRAME accepts both `weight = T::WeightInfo` and
            // `weight(T::WeightInfo)`. We only need to know that the impl
            // delegates its methods to a provider; the generated per-method
            // call is not present in the authored syn tree.
            && attr.to_token_stream().to_string().contains("weight")
    })
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
