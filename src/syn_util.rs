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
