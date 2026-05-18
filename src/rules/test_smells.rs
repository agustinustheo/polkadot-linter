use regex::Regex;
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, Expr, ExprCall, ExprMethodCall, File as SynFile, ImplItem, ItemFn, ItemImpl, Macro,
    StmtMacro,
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

fn span_column(span: proc_macro2::Span) -> usize {
    span.start().column + 1
}

fn path_last_ident(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn attr_path_matches(attr: &Attribute, segments: &[&str]) -> bool {
    let attr_segments = attr
        .path()
        .segments
        .iter()
        .map(|segment| segment.ident.to_string());
    let expected = segments.iter().copied();
    attr_segments.eq(expected)
}

fn has_attr(attrs: &[Attribute], segments: &[&str]) -> bool {
    attrs.iter().any(|attr| attr_path_matches(attr, segments))
}

fn macro_name(mac: &Macro) -> Option<String> {
    path_last_ident(&mac.path)
}

fn expr_call_name(expr_call: &ExprCall) -> Option<String> {
    match &*expr_call.func {
        Expr::Path(expr_path) => path_last_ident(&expr_path.path),
        _ => None,
    }
}

fn compact_tokens(tokens: &proc_macro2::TokenStream) -> String {
    tokens
        .to_string()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn compact_expr(expr: &Expr) -> String {
    quote::ToTokens::to_token_stream(expr)
        .to_string()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

// ---------------------------------------------------------------------------
// TST001: Prefer assert_noop! over manual is_err/unwrap_err
// ---------------------------------------------------------------------------

pub struct AssertNoop;

impl LintRule for AssertNoop {
    fn id(&self) -> &str {
        "TST001"
    }
    fn name(&self) -> &str {
        "prefer-assert-noop"
    }
    fn family(&self) -> &str {
        "test-smell"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || !ctx.is_test_file {
            return None;
        }

        let ast = ast_file(ctx)?;
        let mut diagnostics = Vec::new();

        struct AssertNoopFnVisitor {
            assert_lines: Vec<usize>,
            unwrap_err_lines: Vec<usize>,
        }

        impl<'ast> Visit<'ast> for AssertNoopFnVisitor {
            fn visit_stmt_macro(&mut self, stmt_macro: &'ast StmtMacro) {
                if macro_name(&stmt_macro.mac).as_deref() == Some("assert") {
                    let tokens = compact_tokens(&stmt_macro.mac.tokens);
                    if tokens.contains(".is_err()") {
                        self.assert_lines.push(span_line(stmt_macro.span()));
                    }
                }
                visit::visit_stmt_macro(self, stmt_macro);
            }

            fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
                if node.method == "unwrap_err" {
                    self.unwrap_err_lines.push(span_line(node.span()));
                }
                visit::visit_expr_method_call(self, node);
            }
        }

        struct AssertNoopVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a std::path::Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl AssertNoopVisitor<'_> {
            fn inspect_fn(&mut self, item_fn: &ItemFn) {
                let mut finder = AssertNoopFnVisitor {
                    assert_lines: Vec::new(),
                    unwrap_err_lines: Vec::new(),
                };
                finder.visit_block(&item_fn.block);
                for assert_line in finder.assert_lines {
                    if finder
                        .unwrap_err_lines
                        .iter()
                        .any(|line| *line >= assert_line && *line <= assert_line + 4)
                    {
                        self.diagnostics.push(Diagnostic {
							rule_id: self.rule_id.to_string(),
							rule_name: self.rule_name.to_string(),
							category: RuleCategory::TestSmell,
							severity: self.severity,
							file: self.file.to_path_buf(),
							line: assert_line,
							column: None,
							end_line: None,
							message:
								"Manual `is_err()` + `unwrap_err()` pattern; prefer `assert_noop!`"
									.to_string(),
							explanation:
								"Project convention: use `assert_noop!` for dispatch error \
                                assertions. It checks both the error and that storage was not modified."
									.to_string(),
							suggestion: Some(
								"Replace with `assert_noop!(call, Error::<T>::YourError)`".to_string(),
							),
						});
                    }
                }
            }
        }

        impl<'ast> Visit<'ast> for AssertNoopVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                self.inspect_fn(item_fn);
                visit::visit_item_fn(self, item_fn);
            }
        }

        let mut visitor = AssertNoopVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
        };
        visitor.visit_file(ast);
        diagnostics.extend(visitor.diagnostics);

        let lines: Vec<&str> = ctx.content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if !line.contains(".is_err()") {
                continue;
            }
            if lines
                .iter()
                .skip(idx + 1)
                .take(4)
                .any(|candidate| candidate.contains(".unwrap_err()"))
            {
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    category: RuleCategory::TestSmell,
                    severity: config.rule_severity(self.id(), Severity::Warning),
                    file: ctx.path.clone(),
                    line: idx + 1,
                    column: line.find(".is_err()").map(|col| col + 1),
                    end_line: None,
                    message: "Manual `is_err()` + `unwrap_err()` pattern; prefer `assert_noop!`"
                        .to_string(),
                    explanation:
                        "Project convention: use `assert_noop!` for dispatch error assertions. \
                        It checks both the error and that storage was not modified."
                            .to_string(),
                    suggestion: Some(
                        "Replace with `assert_noop!(call, Error::<T>::YourError)`".to_string(),
                    ),
                });
            }
        }
        diagnostics.sort_by_key(|diag| (diag.line, diag.column.unwrap_or(0)));
        diagnostics.dedup_by(|a, b| {
            a.rule_id == b.rule_id
                && a.line == b.line
                && a.column == b.column
                && a.message == b.message
        });

        if diagnostics.is_empty() {
            None
        } else {
            Some(diagnostics)
        }
    }
}

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
                if macro_name(&stmt_macro.mac).as_deref() == Some("assert_ok") {
                    let tokens = compact_tokens(&stmt_macro.mac.tokens);
                    if tokens.contains("apply_extrinsic") {
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

// ---------------------------------------------------------------------------
// TST003: Imports inside closures/test function bodies
// ---------------------------------------------------------------------------

pub struct ImportsInsideClosures;

impl LintRule for ImportsInsideClosures {
    fn id(&self) -> &str {
        "TST003"
    }
    fn name(&self) -> &str {
        "imports-inside-closures"
    }
    fn family(&self) -> &str {
        "test-smell"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || !ctx.is_test_file {
            return None;
        }

        let ast = ast_file(ctx)?;

        struct UseVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a std::path::Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            block_depth: usize,
        }

        impl<'ast> Visit<'ast> for UseVisitor<'_> {
            fn visit_block(&mut self, block: &'ast syn::Block) {
                self.block_depth += 1;
                visit::visit_block(self, block);
                self.block_depth -= 1;
            }

            fn visit_item_use(&mut self, item_use: &'ast syn::ItemUse) {
                if self.block_depth >= 1 {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::TestSmell,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(item_use.span()),
                        column: Some(span_column(item_use.span())),
                        end_line: None,
                        message: format!(
                            "`use` import inside function/closure body: `{}`",
                            quote::ToTokens::to_token_stream(item_use)
                        ),
                        explanation:
                            "Project convention: place `use` imports at the module level, \
                            not inside closures or test function bodies."
                                .to_string(),
                        suggestion: Some("Move this import to the module level".to_string()),
                    });
                }
                visit::visit_item_use(self, item_use);
            }
        }

        let mut visitor = UseVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Advisory),
            rule_id: self.id(),
            rule_name: self.name(),
            block_depth: 0,
        };
        visitor.visit_file(ast);

        if visitor.diagnostics.is_empty() {
            None
        } else {
            Some(visitor.diagnostics)
        }
    }
}

// ---------------------------------------------------------------------------
// TST004: Pays::No success path must have Pays::Yes error path test
// ---------------------------------------------------------------------------
// FIX #3: This rule now scans test files too, checking whether the test file
// contains a Pays::Yes assertion. If a lib.rs uses Pays::No, and the sibling
// tests.rs does NOT contain `pays_fee, Pays::Yes`, we flag it.

pub struct PaysYesErrorPath;

impl LintRule for PaysYesErrorPath {
    fn id(&self) -> &str {
        "TST004"
    }
    fn name(&self) -> &str {
        "pays-yes-error-path"
    }
    fn family(&self) -> &str {
        "test-smell"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        // FIX: Don't skip files with inline #[cfg(test)] modules.
        // Instead, only skip pure test files (tests/ dir, _test.rs suffix).
        // Files like lib.rs that CONTAIN #[cfg(test)] should still be checked
        // for Pays::No in their non-test production code.
        let path_str = ctx.path.to_string_lossy();
        let is_pure_test_file = path_str.contains("/tests/")
            || path_str.contains("/test/")
            || path_str.ends_with("_test.rs")
            || path_str.ends_with("_tests.rs")
            || path_str.ends_with("tests.rs");
        if is_pure_test_file {
            return None;
        }

        let mut diagnostics = Vec::new();
        let ast = ast_file(ctx)?;

        struct PaysNoVisitor {
            lines: Vec<usize>,
        }

        impl<'ast> Visit<'ast> for PaysNoVisitor {
            fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                if expr_call_name(expr_call).as_deref() == Some("Ok") {
                    let has_pays_no = expr_call
                        .args
                        .iter()
                        .any(|arg| compact_expr(arg).contains("Pays::No"));
                    if has_pays_no {
                        self.lines.push(span_line(expr_call.span()));
                    }
                }
                visit::visit_expr_call(self, expr_call);
            }
        }

        let mut visitor = PaysNoVisitor { lines: Vec::new() };
        visitor.visit_file(ast);
        let pays_no_lines = visitor.lines;

        if pays_no_lines.is_empty() {
            return None;
        }

        // FIX #3: Look for companion test file and check for Pays::Yes assertions
        let has_companion_test = if let Some(parent) = ctx.path.parent() {
            let test_path = parent.join("tests.rs");
            let test_dir = parent.join("tests");

            let test_content = if test_path.exists() {
                std::fs::read_to_string(&test_path).ok()
            } else if test_dir.exists() {
                // Read all .rs files in tests/ directory
                let mut combined = String::new();
                if let Ok(entries) = std::fs::read_dir(&test_dir) {
                    for entry in entries.flatten() {
                        if entry.path().extension().and_then(|e| e.to_str()) == Some("rs") {
                            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                                combined.push_str(&content);
                                combined.push('\n');
                            }
                        }
                    }
                }
                if combined.is_empty() {
                    None
                } else {
                    Some(combined)
                }
            } else {
                None
            };

            if let Some(ref content) = test_content {
                // Check for Pays::Yes assertion pattern
                content.contains("Pays::Yes") && content.contains("pays_fee")
            } else {
                false
            }
        } else {
            false
        };

        if !has_companion_test {
            for line in &pays_no_lines {
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    category: RuleCategory::TestSmell,
                    severity: config.rule_severity(self.id(), Severity::Advisory),
                    file: ctx.path.clone(),
                    line: *line,
                    column: None,
                    end_line: None,
                    message: "Extrinsic returns `Pays::No` on success — no companion test found asserting `Pays::Yes` on the error path".to_string(),
                    explanation: "When an extrinsic refunds fees on success (Pays::No), there \
                        must be a test proving that the error path charges fees (Pays::Yes). \
                        This prevents someone from accidentally setting Pays::No on errors."
                        .to_string(),
                    suggestion: Some(
                        "Add a test: `assert_eq!(result.unwrap_err().post_info.pays_fee, Pays::Yes)`"
                            .to_string(),
                    ),
                });
            }
        }

        if diagnostics.is_empty() {
            None
        } else {
            Some(diagnostics)
        }
    }
}

// ---------------------------------------------------------------------------
// TST005: Tests asserting on implementation details
// ---------------------------------------------------------------------------

pub struct ImplementationDetailAssertions;

impl LintRule for ImplementationDetailAssertions {
    fn id(&self) -> &str {
        "TST005"
    }
    fn name(&self) -> &str {
        "implementation-detail-assertions"
    }
    fn family(&self) -> &str {
        "test-smell"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || !ctx.is_test_file {
            return None;
        }

        let patterns: Vec<Regex> = config
            .test_smells
            .internal_field_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        let ast = ast_file(ctx)?;

        struct AssertMacroVisitor<'a> {
            patterns: &'a [Regex],
            diagnostics: Vec<Diagnostic>,
            file: &'a std::path::Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for AssertMacroVisitor<'_> {
            fn visit_stmt_macro(&mut self, stmt_macro: &'ast StmtMacro) {
                let Some(name) = macro_name(&stmt_macro.mac) else {
                    visit::visit_stmt_macro(self, stmt_macro);
                    return;
                };
                if !matches!(
                    name.as_str(),
                    "assert" | "assert_eq" | "assert_ne" | "assert_matches"
                ) {
                    visit::visit_stmt_macro(self, stmt_macro);
                    return;
                }
                let tokens = compact_tokens(&stmt_macro.mac.tokens);
                if self
                    .patterns
                    .iter()
                    .any(|pattern| pattern.is_match(&tokens))
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
							"Assertion may be testing an implementation detail rather than observable behaviour"
								.to_string(),
						explanation: "Tests that assert on internal fields (counters, caches, \
                            flags, internal state) tend to break when refactoring and do not \
                            prove the system works correctly from a user's perspective."
							.to_string(),
						suggestion: Some(
							"Consider asserting on observable output or public API results instead"
								.to_string(),
						),
					});
                }
                visit::visit_stmt_macro(self, stmt_macro);
            }
        }

        let mut visitor = AssertMacroVisitor {
            patterns: &patterns,
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Advisory),
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

// ---------------------------------------------------------------------------
// TST006: #[pallet::call] extrinsic that mutates storage but emits no event
// ---------------------------------------------------------------------------
// reviewer, reviewer: State-changing extrinsics should emit events so
// external consumers (UIs, indexers) can observe what happened.

pub struct ExtrinsicWithoutEvent;

impl LintRule for ExtrinsicWithoutEvent {
    fn id(&self) -> &str {
        "TST006"
    }
    fn name(&self) -> &str {
        "extrinsic-without-event"
    }
    fn family(&self) -> &str {
        "test-smell"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || ctx.is_test_file || ctx.is_benchmark_file {
            return None;
        }

        let ast = ast_file(ctx)?;

        struct BodyVisitor {
            has_event: bool,
            has_mutation: bool,
        }

        impl<'ast> Visit<'ast> for BodyVisitor {
            fn visit_expr_call(&mut self, node: &'ast ExprCall) {
                if let Expr::Path(expr_path) = &*node.func {
                    if path_last_ident(&expr_path.path).as_deref() == Some("deposit_event") {
                        self.has_event = true;
                    }
                    if let Some(name) = path_last_ident(&expr_path.path) {
                        if [
                            "put",
                            "insert",
                            "mutate",
                            "remove",
                            "kill",
                            "set",
                            "append",
                            "try_append",
                        ]
                        .contains(&name.as_str())
                        {
                            self.has_mutation = true;
                        }
                    }
                }
                visit::visit_expr_call(self, node);
            }

            fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
                let method = node.method.to_string();
                if method == "deposit_event" {
                    self.has_event = true;
                }
                if [
                    "put",
                    "insert",
                    "mutate",
                    "remove",
                    "kill",
                    "set",
                    "append",
                    "try_append",
                ]
                .contains(&method.as_str())
                {
                    self.has_mutation = true;
                }
                visit::visit_expr_method_call(self, node);
            }
        }

        struct ExtrinsicEventVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a std::path::Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for ExtrinsicEventVisitor<'_> {
            fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
                if !has_attr(&item.attrs, &["pallet", "call"]) {
                    return;
                }
                for impl_item in &item.items {
                    let method = match impl_item {
                        ImplItem::Fn(method) => method,
                        _ => continue,
                    };
                    if !has_attr(&method.attrs, &["pallet", "call_index"]) {
                        continue;
                    }
                    let mut body_visitor = BodyVisitor {
                        has_event: false,
                        has_mutation: false,
                    };
                    body_visitor.visit_block(&method.block);
                    if body_visitor.has_mutation && !body_visitor.has_event {
                        self.diagnostics.push(Diagnostic {
							rule_id: self.rule_id.to_string(),
							rule_name: self.rule_name.to_string(),
							category: RuleCategory::TestSmell,
							severity: self.severity,
							file: self.file.to_path_buf(),
							line: span_line(method.sig.ident.span()),
							column: Some(span_column(method.sig.ident.span())),
							end_line: None,
							message: format!(
								"Extrinsic `{}` mutates storage but does not emit an event",
								method.sig.ident
							),
							explanation: "State-changing extrinsics should emit events so external \
                                consumers (UIs, indexers, other pallets) can observe what happened. \
                                Missing events make it impossible to track state changes off-chain."
								.to_string(),
							suggestion: Some(
								"Add `Self::deposit_event(Event::YourEvent { ... })` after the state change"
									.to_string(),
							),
						});
                    }
                }
                visit::visit_item_impl(self, item);
            }
        }

        let mut visitor = ExtrinsicEventVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Advisory),
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
