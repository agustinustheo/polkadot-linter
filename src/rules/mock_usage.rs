use std::{collections::HashSet, path::Path};

use quote::ToTokens;
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, ExprCall, ExprMethodCall, File as SynFile, ItemFn, Macro,
};

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    rules::LintRule,
};

fn ast_file<'a>(ctx: &'a FileContext<'a>) -> Option<&'a SynFile> {
    ctx.ast.as_ref()
}

fn span_line(span: proc_macro2::Span) -> usize {
    span.start().line
}

fn path_last_ident(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn has_attr(attrs: &[Attribute], segments: &[&str]) -> bool {
    attrs.iter().any(|attr| {
        let attr_segments = attr
            .path()
            .segments
            .iter()
            .map(|segment| segment.ident.to_string());
        attr_segments.eq(segments.iter().copied())
    })
}

fn macro_name(mac: &Macro) -> Option<String> {
    path_last_ident(&mac.path)
}

fn expr_call_path(expr_call: &ExprCall) -> Option<&syn::Path> {
    match &*expr_call.func {
        syn::Expr::Path(expr_path) => Some(&expr_path.path),
        _ => None,
    }
}

fn contains_mock_pattern(text: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| text.contains(pattern))
}

struct MockStats<'a> {
    mock_lines: HashSet<usize>,
    assert_lines: HashSet<usize>,
    expectation_lines: HashSet<usize>,
    patterns: &'a [String],
}

impl MockStats<'_> {
    fn mark_mock_line(&mut self, span: proc_macro2::Span, text: &str) {
        if contains_mock_pattern(text, self.patterns) && !text.contains("new_test_ext") {
            self.mock_lines.insert(span_line(span));
        }
    }
}

impl<'ast> Visit<'ast> for MockStats<'_> {
    fn visit_macro(&mut self, mac: &'ast Macro) {
        match macro_name(mac).as_deref() {
            Some(
                "assert" | "assert_eq" | "assert_ne" | "assert_ok" | "assert_err" | "assert_noop"
                | "assert_matches" | "assert_last_event" | "assert_has_event",
            ) => {
                self.assert_lines.insert(span_line(mac.span()));
            }
            _ => {}
        }
        visit::visit_macro(self, mac);
    }

    fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
        let tokens = expr_call.to_token_stream().to_string();
        self.mark_mock_line(expr_call.span(), &tokens);

        if let Some(path) = expr_call_path(expr_call) {
            if matches!(
                path_last_ident(path).as_deref(),
                Some("assert_last_event" | "assert_has_event")
            ) {
                self.assert_lines.insert(span_line(expr_call.span()));
            }
        }

        visit::visit_expr_call(self, expr_call);
    }

    fn visit_expr_method_call(&mut self, expr_method_call: &'ast ExprMethodCall) {
        let line = span_line(expr_method_call.span());
        let method = expr_method_call.method.to_string();
        let tokens = expr_method_call.to_token_stream().to_string();

        self.mark_mock_line(expr_method_call.span(), &tokens);

        if method.starts_with("expect_") || matches!(method.as_str(), "times" | "returning") {
            self.expectation_lines.insert(line);
            self.mock_lines.insert(line);
        }

        if method.starts_with("assert") {
            self.assert_lines.insert(line);
        }

        visit::visit_expr_method_call(self, expr_method_call);
    }
}

// ---------------------------------------------------------------------------
// MOK001: Excessive mock setup in tests
// ---------------------------------------------------------------------------

pub struct ExcessiveMockSetup;

impl LintRule for ExcessiveMockSetup {
    fn id(&self) -> &str {
        "MOK001"
    }
    fn name(&self) -> &str {
        "excessive-mock-setup"
    }
    fn family(&self) -> &str {
        "mock-usage"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || !ctx.is_test_file {
            return None;
        }

        let ast = ast_file(ctx)?;

        struct TestFnVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            config: &'a crate::config::MockUsageConfig,
        }

        impl<'ast> Visit<'ast> for TestFnVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                if !has_attr(&item_fn.attrs, &["test"]) {
                    visit::visit_item_fn(self, item_fn);
                    return;
                }

                let mut stats = MockStats {
                    mock_lines: HashSet::new(),
                    assert_lines: HashSet::new(),
                    expectation_lines: HashSet::new(),
                    patterns: &self.config.mock_patterns,
                };
                stats.visit_block(&item_fn.block);

                let mock_lines = stats.mock_lines.len();
                let assert_lines = stats.assert_lines.len();
                let mock_expectation_count = stats.expectation_lines.len();
                let fn_name = item_fn.sig.ident.to_string();

                if assert_lines > 0
                    && mock_lines as f64 / assert_lines as f64 > self.config.max_mock_ratio
                {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::TestSmell,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line: span_line(item_fn.span()),
						column: None,
						end_line: None,
						message: format!(
							"Test `{}` has {mock_lines} mock/setup lines but only {assert_lines} assertions (ratio: {:.1}x)",
							fn_name,
							mock_lines as f64 / assert_lines as f64
						),
						explanation: "Tests dominated by mock setup tend to test interactions rather than outcomes, \
                            and break easily when implementation changes."
							.to_string(),
						suggestion: Some(
							"Consider using integration-style testing with real dependencies, or extract shared setup into helper functions."
								.to_string(),
						),
					});
                }

                if mock_expectation_count > self.config.max_mock_expectations {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::TestSmell,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line: span_line(item_fn.span()),
						column: None,
						end_line: None,
						message: format!(
							"Test `{}` has {} mock expectations (threshold: {})",
							fn_name,
							mock_expectation_count,
							self.config.max_mock_expectations
						),
						explanation: "Many mock expectations suggest the test is verifying interaction details rather than behaviour outcomes."
							.to_string(),
						suggestion: Some(
							"Reduce mock expectations; focus on asserting outcomes.".to_string(),
						),
					});
                }

                visit::visit_item_fn(self, item_fn);
            }
        }

        let mut visitor = TestFnVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            config: &config.mock_usage,
        };
        visitor.visit_file(ast);

        if visitor.diagnostics.is_empty() {
            None
        } else {
            Some(visitor.diagnostics)
        }
    }
}
