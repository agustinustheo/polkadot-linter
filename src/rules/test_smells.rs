use syn::{
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
    Expr, Macro, StmtMacro, Token,
};

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    rules::LintRule,
    syn_util::{ast_file, expr_call_name, macro_name, span_column, span_line},
};

// ---------------------------------------------------------------------------
// TST002: Don't use assert_ok! on apply_extrinsic
// ---------------------------------------------------------------------------

pub struct ApplyExtrinsicAssertOk;

impl LintRule for ApplyExtrinsicAssertOk {
    fn id(&self) -> &str {
        "TST002"
    }
    fn name(&self) -> &str {
        "apply-extrinsic-assert-ok"
    }
    fn family(&self) -> &str {
        "test-smell"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || !ctx.is_test_file {
            return None;
        }

        let ast = ast_file(ctx)?;

        struct ApplyExtrinsicVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a std::path::Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for ApplyExtrinsicVisitor<'_> {
            fn visit_stmt_macro(&mut self, stmt_macro: &'ast StmtMacro) {
                if macro_name(&stmt_macro.mac).as_deref() == Some("assert_ok")
                    && asserts_on_apply_extrinsic(&stmt_macro.mac)
                {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::TestSmell,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(stmt_macro.span()),
                        column: Some(span_column(stmt_macro.span())),
                        end_line: None,
                        message:
                            "`assert_ok!` on `apply_extrinsic` ignores the inner DispatchError"
                                .to_string(),
                        explanation: "`apply_extrinsic` returns \
                                `Result<Result<(), DispatchError>, TransactionInvalidityError>`. \
                                `assert_ok!` only checks the outer Result, so a dispatch failure \
                                is silently swallowed."
                            .to_string(),
                        suggestion: Some(
                            "Check both layers: `assert_eq!(apply_extrinsic(call), Ok(Ok(())))`"
                                .to_string(),
                        ),
                    });
                }
                visit::visit_stmt_macro(self, stmt_macro);
            }
        }

        let mut visitor = ApplyExtrinsicVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Error),
            rule_id: self.id(),
            rule_name: self.name(),
        };
        visitor.visit_file(ast);

        if visitor.diagnostics.is_empty() {
            None
        } else {
            Some(visitor.diagnostics)
        }
    }
}

/// Whether the asserted expression is the two-layer `apply_extrinsic` result.
/// The name is matched as a whole path or method segment, so helpers such as
/// `apply_extrinsics`, which return a plain `Result`, are not mistaken for it.
/// `apply_extrinsic(xt).unwrap()` / `.expect(..)` / `?` peel the outer
/// validity layer, leaving `assert_ok!` to check the inner `DispatchResult`.
/// The two-argument form `assert_ok!(x, y)` expands to
/// `assert_eq!(x, Ok(y))`, which compares the whole value, so it is exact.
fn asserts_on_apply_extrinsic(mac: &Macro) -> bool {
    fn is_apply_extrinsic_call(expr: &Expr) -> bool {
        match expr {
            Expr::Call(call) => expr_call_name(call).as_deref() == Some("apply_extrinsic"),
            Expr::MethodCall(call) => call.method == "apply_extrinsic",
            Expr::Paren(paren) => is_apply_extrinsic_call(&paren.expr),
            Expr::Group(group) => is_apply_extrinsic_call(&group.expr),
            _ => false,
        }
    }

    struct ApplyExtrinsicCall(bool);

    impl<'ast> Visit<'ast> for ApplyExtrinsicCall {
        fn visit_expr(&mut self, expr: &'ast Expr) {
            let unwrapped_inner = match expr {
                Expr::MethodCall(call)
                    if matches!(call.method.to_string().as_str(), "unwrap" | "expect") =>
                {
                    Some(&*call.receiver)
                }
                Expr::Try(try_expr) => Some(&*try_expr.expr),
                _ => None,
            };
            if unwrapped_inner.is_some_and(is_apply_extrinsic_call) {
                return;
            }
            self.0 |= is_apply_extrinsic_call(expr);
            visit::visit_expr(self, expr);
        }

        // A closure argument's body is not the asserted value.
        fn visit_expr_closure(&mut self, _closure: &'ast syn::ExprClosure) {}
    }

    let Ok(arguments) = mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) else {
        return false;
    };
    if arguments.len() != 1 {
        return false;
    }
    arguments.first().is_some_and(|asserted| {
        let mut visitor = ApplyExtrinsicCall(false);
        visitor.visit_expr(asserted);
        visitor.0
    })
}
