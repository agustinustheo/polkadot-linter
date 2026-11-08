//! Small `syn` helpers shared by the FRAME model and the source rules.

use proc_macro2::Span;
use quote::ToTokens;
use syn::{
    visit::{self, Visit},
    Attribute, Expr, ExprCall, File as SynFile, Macro, Type,
};

use crate::{engine::FileContext, frame_model::active_cfg_attr_path_matches};

pub(crate) fn ast_file<'a>(ctx: &'a FileContext<'a>) -> Option<&'a SynFile> {
    ctx.ast.as_ref()
}

pub(crate) fn span_line(span: Span) -> usize {
    span.start().line
}

pub(crate) fn span_column(span: Span) -> usize {
    span.start().column + 1
}

pub(crate) fn is_masked_span(mask: &[bool], span: Span) -> bool {
    mask.get(span_line(span).saturating_sub(1))
        .copied()
        .unwrap_or(false)
}

pub(crate) fn path_last_ident(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

/// `construct_runtime!` or a fork that keeps the prefix
/// (InvArch's `construct_runtime_modified!`).
pub(crate) fn is_construct_runtime(name: &str) -> bool {
    name.starts_with("construct_runtime")
}

pub(crate) fn macro_name(mac: &Macro) -> Option<String> {
    path_last_ident(&mac.path)
}

/// Matches `segments` exactly, as a path suffix, or through an always-active
/// `cfg_attr`.
pub(crate) fn attr_path_matches(attr: &Attribute, segments: &[&str]) -> bool {
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
        || active_cfg_attr_path_matches(attr, segments)
}

pub(crate) fn has_attr(attrs: &[Attribute], segments: &[&str]) -> bool {
    attrs.iter().any(|attr| attr_path_matches(attr, segments))
}

/// Like [`has_attr`], but only an attribute path equal to `segments` matches.
pub(crate) fn has_exact_attr(attrs: &[Attribute], segments: &[&str]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .eq(segments.iter().copied())
    })
}

/// `#[path = "..."]` on a module declaration.
pub(crate) fn path_attr(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        syn::Meta::NameValue(name_value) if name_value.path.is_ident("path") => {
            match &name_value.value {
                Expr::Lit(expr) => match &expr.lit {
                    syn::Lit::Str(path) => Some(path.value()),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    })
}

pub(crate) fn attr_expr(attr: &Attribute) -> Option<Expr> {
    attr.parse_args::<Expr>().ok()
}

pub(crate) fn compact_tokens<T: ToTokens>(node: &T) -> String {
    node.to_token_stream()
        .to_string()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

pub(crate) fn strip_expr_wrappers(mut expr: &Expr) -> &Expr {
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

fn expr_path(expr: &Expr) -> Option<&syn::Path> {
    match expr {
        Expr::Path(expr_path) => Some(&expr_path.path),
        Expr::Group(group) => expr_path(&group.expr),
        Expr::Paren(paren) => expr_path(&paren.expr),
        Expr::Reference(reference) => expr_path(&reference.expr),
        _ => None,
    }
}

pub(crate) fn expr_call_name(expr_call: &ExprCall) -> Option<String> {
    match &*expr_call.func {
        Expr::Path(expr_path) => path_last_ident(&expr_path.path),
        _ => None,
    }
}

pub(crate) fn expr_call_path(expr_call: &ExprCall) -> Option<&syn::Path> {
    expr_path(&expr_call.func)
}

pub(crate) fn type_contains_named(ty: &Type, names: &[&str]) -> bool {
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

/// `#[test]`, or a `cfg` that requires `test` (`all(test, ..)`, `any(test, doctest)`).
pub(crate) fn test_only(attrs: &[Attribute]) -> bool {
    fn requires_test(meta: &syn::Meta) -> bool {
        match meta {
            syn::Meta::Path(path) => path.is_ident("test") || path.is_ident("doctest"),
            syn::Meta::List(list) => {
                let Ok(predicates) = list.parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                ) else {
                    return false;
                };
                (list.path.is_ident("all") && predicates.iter().any(requires_test))
                    || (list.path.is_ident("any")
                        && !predicates.is_empty()
                        && predicates.iter().all(requires_test))
            }
            _ => false,
        }
    }
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .parse_args::<syn::Meta>()
                    .ok()
                    .is_some_and(|meta| requires_test(&meta)))
    })
}

/// The first origin check in `block` (the first call named `ensure_*`) is
/// privileged and stands alone as `check(origin)?;` with its result
/// discarded: `ensure_root`, `X::ensure_origin` (one argument),
/// `ensure_origin_or_root` or `ensure_admin*`. A bound result (`let who =
/// T::X::ensure_origin(o)?`) is an account, and `ensure_root(o).is_err()`
/// falls back to another origin, so neither is privileged.
pub(crate) fn privileged_origin(block: &syn::Block) -> bool {
    struct First<'ast>(Option<&'ast ExprCall>);
    impl<'ast> Visit<'ast> for First<'ast> {
        fn visit_expr_call(&mut self, call: &'ast ExprCall) {
            if self.0.is_none() {
                let name = expr_call_path(call).and_then(path_last_ident);
                if name.is_some_and(|name| name.starts_with("ensure_")) {
                    self.0 = Some(call);
                }
                visit::visit_expr_call(self, call);
            }
        }
    }
    let mut first = First(None);
    first.visit_block(block);
    let Some(call) = first.0 else {
        return false;
    };
    let Some(path) = expr_call_path(call) else {
        return false;
    };
    let name = path_last_ident(path).unwrap_or_default();
    let privileged = matches!(name.as_str(), "ensure_root" | "ensure_origin_or_root")
        || name.starts_with("ensure_admin")
        || (name == "ensure_origin" && path.segments.len() > 1 && call.args.len() == 1);
    privileged
        && block.stmts.iter().any(|stmt| {
            matches!(stmt, syn::Stmt::Expr(Expr::Try(t), Some(_))
                if matches!(&*t.expr, Expr::Call(c) if std::ptr::eq(c, call)))
        })
}
