use std::path::Path;

use proc_macro2::Span;
use quote::ToTokens;
use syn::{
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, Expr, ExprBinary, ExprCall, ExprForLoop, ExprMethodCall, File as SynFile, FnArg,
    GenericArgument, ImplItem, Item, ItemEnum, ItemFn, ItemImpl, ItemType, Lit, Local, Macro, Pat,
    PathArguments, Token, Type, TypePath, UseTree, Visibility,
};

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    rules::LintRule,
};

fn is_pure_test_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    path_str.contains("/tests/")
        || path_str.contains("/test/")
        || path_str.contains("integration_tests")
        || path_str.contains("integration-tests")
        || path_str.contains("send-tx/")
        || path_str.ends_with("_test.rs")
        || path_str.ends_with("_tests.rs")
        || path_str.ends_with("tests.rs")
        || file_name == "mock.rs"
        || file_name == "testing_utils.rs"
}

fn should_skip_production_rule(ctx: &FileContext) -> bool {
    !ctx.is_rust || ctx.is_benchmark_file || is_pure_test_path(&ctx.path)
}

fn is_module_declaration(trimmed: &str) -> bool {
    trimmed.starts_with("mod ")
        || trimmed.starts_with("pub mod ")
        || trimmed.starts_with("pub(crate) mod ")
        || trimmed.starts_with("pub(super) mod ")
        || trimmed.starts_with("pub(in ")
}

fn is_block_start(trimmed: &str) -> bool {
    is_module_declaration(trimmed)
        || trimmed.starts_with("impl ")
        || trimmed.starts_with("pub impl ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("fn ")
        || trimmed.starts_with("pub(crate) fn ")
        || trimmed.starts_with("pub struct ")
        || trimmed.starts_with("pub(crate) struct ")
        || trimmed.starts_with("pub enum ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("pub trait ")
        || trimmed.starts_with("trait ")
}

fn is_masked_cfg_attribute(trimmed: &str) -> bool {
    trimmed == "#[cfg(test)]" || trimmed.contains("feature = \"runtime-benchmarks\"")
}

fn cfg_test_module_mask(content: &str) -> Vec<bool> {
    let lines: Vec<&str> = content.lines().collect();
    let mut mask = vec![false; lines.len()];
    let mut masked_depth: Option<i32> = None;
    let mut brace_depth: i32 = 0;
    let mut next_item_is_masked = false;
    // Tracks whether brace_depth has exceeded masked_depth (the block body started).
    // Without this, multi-line declarations (impl Foo\n  for Bar\n{) would
    // clear the mask before the opening `{` is reached.
    let mut mask_block_entered = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Mask #[cfg(test)] and runtime benchmark-only items.
        if is_masked_cfg_attribute(trimmed) {
            next_item_is_masked = true;
        }

        // Skip other attributes between the cfg and the target item.
        if next_item_is_masked && trimmed.starts_with("#[") && !is_masked_cfg_attribute(trimmed) {
            // keep waiting
        }

        let open = line.chars().filter(|&c| c == '{').count() as i32;
        let close = line.chars().filter(|&c| c == '}').count() as i32;
        let starts_masked_block = next_item_is_masked && is_block_start(trimmed);

        if starts_masked_block {
            masked_depth = Some(brace_depth);
            mask_block_entered = false;
            next_item_is_masked = false;
        } else if next_item_is_masked && !trimmed.is_empty() && !trimmed.starts_with("#[") {
            mask[i] = true;
            next_item_is_masked = false;
        }

        brace_depth += open;
        brace_depth -= close;

        if let Some(td) = masked_depth {
            if starts_masked_block || brace_depth > td {
                mask[i] = true;
                mask_block_entered = true;
            }
            // Only clear the mask after we have entered AND exited the block.
            if mask_block_entered && brace_depth <= td {
                masked_depth = None;
                mask_block_entered = false;
            }
        }
    }

    mask
}

pub(crate) fn strip_strings_and_line_comments(line: &str) -> String {
    let mut stripped = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => {
                    in_string = false;
                    stripped.push(' ');
                }
                _ => {}
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            stripped.push(' ');
            continue;
        }
        if ch == '/' && matches!(chars.peek(), Some('/')) {
            break;
        }
        stripped.push(ch);
    }
    stripped
}

fn ast_file<'a>(ctx: &'a FileContext<'a>) -> Option<&'a SynFile> {
    ctx.ast.as_ref()
}

fn span_line(span: Span) -> usize {
    span.start().line
}

fn span_column(span: Span) -> usize {
    span.start().column + 1
}

fn is_masked_span(mask: &[bool], span: Span) -> bool {
    mask.get(span_line(span).saturating_sub(1))
        .copied()
        .unwrap_or(false)
}

fn path_last_ident(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn macro_name(mac: &Macro) -> Option<String> {
    path_last_ident(&mac.path)
}

fn path_has_exact_ident(path: &syn::Path, ident: &str) -> bool {
    path_last_ident(path).as_deref() == Some(ident)
}

fn path_has_segment(path: &syn::Path, ident: &str) -> bool {
    path.segments.iter().any(|segment| segment.ident == ident)
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

fn attr_contains_dead_code(attr: &Attribute) -> bool {
    path_has_exact_ident(attr.path(), "allow")
        && attr.to_token_stream().to_string().contains("dead_code")
}

fn derive_paths(attr: &Attribute) -> Option<Punctuated<syn::Path, Token![,]>> {
    if !path_has_exact_ident(attr.path(), "derive") {
        return None;
    }
    attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
        .ok()
}

fn type_is_named(ty: &Type, names: &[&str]) -> bool {
    match ty {
        Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .map(|segment| names.iter().any(|name| segment.ident == *name))
            .unwrap_or(false),
        Type::Group(group) => type_is_named(&group.elem, names),
        Type::Paren(paren) => type_is_named(&paren.elem, names),
        Type::Reference(reference) => type_is_named(&reference.elem, names),
        _ => false,
    }
}

fn type_contains_named(ty: &Type, names: &[&str]) -> bool {
    struct TypeNameVisitor<'a> {
        names: &'a [&'a str],
        found: bool,
    }

    impl<'ast> Visit<'ast> for TypeNameVisitor<'_> {
        fn visit_type_path(&mut self, type_path: &'ast TypePath) {
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

fn expr_call_name(expr_call: &ExprCall) -> Option<String> {
    match &*expr_call.func {
        Expr::Path(expr_path) => expr_path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        _ => None,
    }
}

fn compact_tokens<T: ToTokens>(node: &T) -> String {
    node.to_token_stream()
        .to_string()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
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

fn path_owner_name(path: &syn::Path) -> Option<String> {
    let mut segments = path.segments.iter().rev();
    let _last = segments.next()?;
    segments.next().map(|segment| segment.ident.to_string())
}

fn attr_expr(attr: &Attribute) -> Option<Expr> {
    attr.parse_args::<Expr>().ok()
}

fn use_tree_has_disallowed_glob(tree: &UseTree) -> bool {
    fn walk(tree: &UseTree, allow_glob: bool) -> bool {
        match tree {
            UseTree::Glob(_) => !allow_glob,
            UseTree::Group(group) => group.items.iter().any(|item| walk(item, allow_glob)),
            UseTree::Name(_) | UseTree::Rename(_) => false,
            UseTree::Path(path) => {
                let allow_nested_glob = allow_glob
                    || path.ident == "super"
                    || path.ident.to_string().contains("prelude");
                walk(&path.tree, allow_nested_glob)
            }
        }
    }

    walk(tree, false)
}

fn has_public_visibility(visibility: &Visibility) -> bool {
    !matches!(visibility, Visibility::Inherited)
}

fn is_storage_iteration_call_path(path: &syn::Path) -> bool {
    matches!(path_last_ident(path).as_deref(), Some("iter" | "drain")) && path_owner_name(path).is_some()
}

struct MacroNameVisitor<'a> {
    names: &'a [&'a str],
    matches: Vec<(String, Span)>,
}

impl<'ast> Visit<'ast> for MacroNameVisitor<'_> {
    fn visit_macro(&mut self, mac: &'ast Macro) {
        if let Some(name) = path_last_ident(&mac.path) {
            if self
                .names
                .iter()
                .any(|candidate| *candidate == name.as_str())
            {
                self.matches.push((name.to_string(), mac.span()));
            }
        }
        visit::visit_macro(self, mac);
    }
}

struct ExprMethodVisitor<'a> {
    names: &'a [&'a str],
    matches: Vec<(String, Span)>,
}

impl<'ast> Visit<'ast> for ExprMethodVisitor<'_> {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let method = node.method.to_string();
        if self.names.iter().any(|candidate| *candidate == method) {
            self.matches.push((method, node.span()));
        }
        visit::visit_expr_method_call(self, node);
    }
}

// ---------------------------------------------------------------------------
// VAL001: Validation before heavy reads
// ---------------------------------------------------------------------------
// Reviewers repeatedly flag cases where expensive storage reads occur before
// cheap precondition checks. The fix is to reorder: cheap guards first, then
// heavy reads.

pub struct ValidationBeforeHeavyRead;

impl LintRule for ValidationBeforeHeavyRead {
    fn id(&self) -> &str {
        "VAL001"
    }
    fn name(&self) -> &str {
        "validation-before-heavy-read"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        // FIX #5: Skip benchmark and test files — validation order in setup code
        // is not the same concern as in dispatch/production code.
        if ctx.is_benchmark_file || ctx.is_test_file {
            return None;
        }

        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();

        // Track function boundaries
        let mut in_fn = false;
        let mut fn_start = 0;
        let mut brace_depth: i32 = 0;
        let mut first_heavy_op: Option<(usize, String)> = None;
        let mut heavy_op_lhs: Option<String> = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip comments and attributes
            if trimmed.starts_with("//") || trimmed.starts_with("#[") || trimmed.starts_with("///")
            {
                continue;
            }

            // Detect function start
            if !in_fn
                && (trimmed.contains("pub fn ") || trimmed.starts_with("fn "))
                && trimmed.contains('(')
            {
                in_fn = true;
                fn_start = i;
                first_heavy_op = None;
                heavy_op_lhs = None;
                brace_depth = 0;
            }

            if in_fn {
                brace_depth += line.chars().filter(|&c| c == '{').count() as i32;
                brace_depth -= line.chars().filter(|&c| c == '}').count() as i32;

                // Check for heavy operations
                if first_heavy_op.is_none() {
                    for pattern in &config.validation_order.heavy_operations {
                        if trimmed.contains(pattern.as_str()) {
                            // FIX #5: Extract the LHS variable name to avoid false positives
                            // when the validation depends on the read result.
                            let lhs = trimmed.split('=').next().unwrap_or("").trim().to_string();
                            heavy_op_lhs = Some(lhs);
                            first_heavy_op = Some((i, pattern.clone()));
                            break;
                        }
                    }
                }

                // If we already saw a heavy op, check for cheap validations after it
                if let Some((heavy_line, ref heavy_pattern)) = first_heavy_op {
                    for pattern in &config.validation_order.cheap_validations {
                        if trimmed.contains(pattern.as_str()) && i > heavy_line {
                            // FIX #5: Skip if the validation references the variable
                            // assigned from the heavy read (data-dependent validation).
                            if let Some(ref lhs) = heavy_op_lhs {
                                let var_name = lhs.split_whitespace().last().unwrap_or("");
                                if !var_name.is_empty() && trimmed.contains(var_name) {
                                    continue;
                                }
                            }

                            diagnostics.push(Diagnostic {
                                rule_id: self.id().to_string(),
                                rule_name: self.name().to_string(),
                                category: RuleCategory::Semantic,
                                severity: config.rule_severity(self.id(), Severity::Warning),
                                file: ctx.path.clone(),
                                line: heavy_line + 1,
                                column: None,
                                end_line: Some(i + 1),
                                message: format!(
                                    "Heavy operation `{}` at line {} occurs before cheap validation `{}` at line {}",
                                    heavy_pattern, heavy_line + 1, pattern, i + 1
                                ),
                                explanation: "Cheap precondition checks should run before expensive \
                                    storage reads or computation. This avoids wasting resources \
                                    when the operation would fail anyway."
                                    .to_string(),
                                suggestion: Some(format!(
                                    "Move the `{}` check before the `{}` call",
                                    pattern, heavy_pattern
                                )),
                            });
                            // Only report once per function per heavy op
                            first_heavy_op = None;
                            heavy_op_lhs = None;
                            break;
                        }
                    }
                }

                // Function end
                if brace_depth <= 0 && i > fn_start {
                    in_fn = false;
                    first_heavy_op = None;
                    heavy_op_lhs = None;
                }
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
// SEM002: Prefer collect::<Vec<_>>() turbofish
// ---------------------------------------------------------------------------
// Style guide: prefer `collect::<Vec<_>>()` over `let x: Vec<T> = iter.collect()`

pub struct PreferCollectTurbofish;

impl LintRule for PreferCollectTurbofish {
    fn id(&self) -> &str {
        "SEM002"
    }
    fn name(&self) -> &str {
        "prefer-collect-turbofish"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        let ast = ast_file(ctx)?;

        struct CollectVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for CollectVisitor<'_> {
            fn visit_local(&mut self, local: &'ast Local) {
                let Pat::Type(pat_type) = &local.pat else {
                    return;
                };
                if !type_contains_named(&pat_type.ty, &["Vec"]) {
                    return;
                }
                let Some(init) = &local.init else {
                    return;
                };
                let Expr::MethodCall(method_call) = &*init.expr else {
                    return;
                };
                if method_call.method != "collect" || method_call.turbofish.is_some() {
                    return;
                }

                self.diagnostics.push(Diagnostic {
                    rule_id: self.rule_id.to_string(),
                    rule_name: self.rule_name.to_string(),
                    category: RuleCategory::Semantic,
                    severity: self.severity,
                    file: self.file.to_path_buf(),
                    line: span_line(local.span()),
                    column: Some(span_column(local.span())),
                    end_line: None,
                    message: "Prefer `.collect::<Vec<_>>()` turbofish over typed let-binding"
                        .to_string(),
                    explanation: "Project convention: use turbofish syntax for collect() \
                        to keep the type near the call site."
                        .to_string(),
                    suggestion: Some("Rewrite as `.collect::<Vec<_>>()`".to_string()),
                });
            }
        }

        let mut visitor = CollectVisitor {
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
// SEM003: Prefer `for x in &collection` over `.iter()`
// ---------------------------------------------------------------------------

pub struct PreferRefIteration;

impl LintRule for PreferRefIteration {
    fn id(&self) -> &str {
        "SEM003"
    }
    fn name(&self) -> &str {
        "prefer-ref-iteration"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        // send-tx and other utility paths are excluded: they use library types
        // (e.g. subxt's ExtrinsicEvents) that have custom iter() but no IntoIterator.
        if is_pure_test_path(&ctx.path) {
            return None;
        }

        let ast = ast_file(ctx)?;

        struct RefIterationVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for RefIterationVisitor<'_> {
            fn visit_expr_for_loop(&mut self, expr_for: &'ast ExprForLoop) {
                let Expr::MethodCall(method_call) = &*expr_for.expr else {
                    return;
                };
                if method_call.method != "iter" || !matches!(&*method_call.receiver, Expr::Path(_))
                {
                    return;
                }

                self.diagnostics.push(Diagnostic {
                    rule_id: self.rule_id.to_string(),
                    rule_name: self.rule_name.to_string(),
                    category: RuleCategory::Semantic,
                    severity: self.severity,
                    file: self.file.to_path_buf(),
                    line: span_line(expr_for.span()),
                    column: Some(span_column(expr_for.span())),
                    end_line: None,
                    message: "Prefer `for x in &collection` over `for x in collection.iter()`"
                        .to_string(),
                    explanation: "Project convention: use reference iteration syntax for clarity."
                        .to_string(),
                    suggestion: Some(
                        "Replace `.iter()` with `&` prefix on the collection".to_string(),
                    ),
                });
            }
        }

        let mut visitor = RefIterationVisitor {
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
// SEM004: No wildcard imports in non-test code
// ---------------------------------------------------------------------------

pub struct NoWildcardImports;

impl LintRule for NoWildcardImports {
    fn id(&self) -> &str {
        "SEM004"
    }
    fn name(&self) -> &str {
        "no-wildcard-imports"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        // Wildcard imports are allowed in test and benchmark files
        if ctx.is_test_file || ctx.is_benchmark_file {
            return None;
        }

        // extension.rs files use `use crate::*;` as a standard Polkadot SDK pattern
        let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name == "extension.rs" {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct UseVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            block_depth: usize,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for UseVisitor<'_> {
            fn visit_block(&mut self, block: &'ast syn::Block) {
                self.block_depth += 1;
                visit::visit_block(self, block);
                self.block_depth -= 1;
            }

            fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
                if self.block_depth == 0
                    && !has_public_visibility(&node.vis)
                    && !is_masked_span(self.mask, node.span())
                    && use_tree_has_disallowed_glob(&node.tree)
                {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(node.span()),
                        column: Some(span_column(node.span())),
                        end_line: None,
                        message: format!(
                            "Wildcard import in non-test code: `{}`",
                            node.to_token_stream()
                        ),
                        explanation: "Project convention: import items explicitly in main code. \
                            Wildcard imports are only permitted in tests."
                            .to_string(),
                        suggestion: Some("Replace with explicit imports".to_string()),
                    });
                }

                visit::visit_item_use(self, node);
            }
        }

        let mut visitor = UseVisitor {
            mask: &test_mask,
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            block_depth: 0,
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
// SEM005: Parameterised weight functions
// ---------------------------------------------------------------------------
// Style: use `T::WeightInfo::foo(n)` not `T::WeightInfo::foo().saturating_mul(n)`

pub struct ParameteriseWeightFunctions;

impl LintRule for ParameteriseWeightFunctions {
    fn id(&self) -> &str {
        "SEM005"
    }
    fn name(&self) -> &str {
        "parameterise-weight-functions"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        let ast = ast_file(ctx)?;

        fn is_unparameterised_weight_mul(node: &ExprMethodCall) -> bool {
            if node.method != "saturating_mul" {
                return false;
            }
            let Expr::Call(receiver_call) = &*node.receiver else {
                return false;
            };
            let Expr::Path(expr_path) = &*receiver_call.func else {
                return false;
            };
            receiver_call.args.is_empty() && path_has_segment(&expr_path.path, "WeightInfo")
        }

        struct WeightExprVisitor {
            found: bool,
        }

        impl<'ast> Visit<'ast> for WeightExprVisitor {
            fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
                if !self.found && is_unparameterised_weight_mul(node) {
                    self.found = true;
                }
                visit::visit_expr_method_call(self, node);
            }
        }

        struct WeightInfoVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl WeightInfoVisitor<'_> {
            fn push_diag(&mut self, span: Span) {
                self.diagnostics.push(Diagnostic {
                    rule_id: self.rule_id.to_string(),
                    rule_name: self.rule_name.to_string(),
                    category: RuleCategory::Semantic,
                    severity: self.severity,
                    file: self.file.to_path_buf(),
                    line: span_line(span),
                    column: Some(span_column(span)),
                    end_line: None,
                    message: "Weight function called without parameter then multiplied"
                        .to_string(),
                    explanation:
                        "Use parameterised benchmarks: `T::WeightInfo::foo(n)` captures \
                        constant overhead that does not scale with the input, unlike \
                        `T::WeightInfo::foo().saturating_mul(n)` which misses it."
                            .to_string(),
                    suggestion: Some(
                        "Pass the scaling parameter directly: `T::WeightInfo::foo(n)`"
                            .to_string(),
                    ),
                });
            }

            fn inspect_attrs(&mut self, attrs: &[Attribute]) {
                for attr in attrs {
                    if !attr_path_matches(attr, &["pallet", "weight"]) {
                        continue;
                    }
                    let Ok(expr) = attr.parse_args::<Expr>() else {
                        continue;
                    };
                    let mut finder = WeightExprVisitor { found: false };
                    finder.visit_expr(&expr);
                    if finder.found {
                        self.push_diag(attr.span());
                    }
                }
            }
        }

        impl<'ast> Visit<'ast> for WeightInfoVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                self.inspect_attrs(&item_fn.attrs);
                visit::visit_item_fn(self, item_fn);
            }

            fn visit_impl_item_fn(&mut self, item_fn: &'ast syn::ImplItemFn) {
                self.inspect_attrs(&item_fn.attrs);
                visit::visit_impl_item_fn(self, item_fn);
            }

            fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
                if is_unparameterised_weight_mul(node) {
                    self.push_diag(node.span());
                }
                visit::visit_expr_method_call(self, node);
            }
        }

        let mut visitor = WeightInfoVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
// SEM006: DbWeight::get().reads() missing proof size
// ---------------------------------------------------------------------------
// size (PoV). Parachains need both dimensions. Use benchmarks instead.

pub struct DbWeightMissingPov;

impl LintRule for DbWeightMissingPov {
    fn id(&self) -> &str {
        "SEM006"
    }
    fn name(&self) -> &str {
        "dbweight-missing-pov"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || ctx.is_test_file || ctx.is_benchmark_file {
            return None;
        }

        // Skip auto-generated weights files (they use DbWeight as fallback)
        let path_str = ctx.rel_path.to_string_lossy();
        if path_str.ends_with("weights.rs") || path_str.contains("/weights/") {
            return None;
        }

        let ast = ast_file(ctx)?;

        struct DbWeightVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for DbWeightVisitor<'_> {
            fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
                if !["reads", "writes", "reads_writes"].contains(&node.method.to_string().as_str())
                {
                    visit::visit_expr_method_call(self, node);
                    return;
                }
                let Expr::Call(receiver_call) = &*node.receiver else {
                    visit::visit_expr_method_call(self, node);
                    return;
                };
                let Expr::Path(expr_path) = &*receiver_call.func else {
                    visit::visit_expr_method_call(self, node);
                    return;
                };
                if receiver_call.args.is_empty()
                    && path_has_exact_ident(&expr_path.path, "get")
                    && path_has_segment(&expr_path.path, "DbWeight")
                {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::Semantic,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line: span_line(node.span()),
						column: Some(span_column(node.span())),
						end_line: None,
						message:
							"`DbWeight::get().reads()` only accounts for ref-time, not proof size (PoV)"
								.to_string(),
						explanation: "Parachains have two weight dimensions: ref-time and proof size. \
                            `DbWeight::get().reads(N)` only estimates ref-time. Use a proper benchmark \
                            to capture both dimensions accurately."
							.to_string(),
						suggestion: Some(
							"Replace with a dedicated benchmark that captures proof size".to_string(),
						),
					});
                }
                visit::visit_expr_method_call(self, node);
            }
        }

        let mut visitor = DbWeightVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
// SEM007: RuntimeDebug is deprecated — use Debug
// ---------------------------------------------------------------------------

pub struct RuntimeDebugDeprecated;

impl LintRule for RuntimeDebugDeprecated {
    fn id(&self) -> &str {
        "SEM007"
    }
    fn name(&self) -> &str {
        "runtime-debug-deprecated"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        let ast = ast_file(ctx)?;

        struct RuntimeDebugVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl RuntimeDebugVisitor<'_> {
            fn push_attr_match(&mut self, span: Span, replacement: &str) {
                self.diagnostics.push(Diagnostic {
                    rule_id: self.rule_id.to_string(),
                    rule_name: self.rule_name.to_string(),
                    category: RuleCategory::Semantic,
                    severity: self.severity,
                    file: self.file.to_path_buf(),
                    line: span_line(span),
                    column: Some(span_column(span)),
                    end_line: None,
                    message: format!(
                        "`RuntimeDebug` is deprecated — use `{}` instead",
                        replacement
                    ),
                    explanation: "polkadot-sdk moved away from `RuntimeDebug` because the space \
                        savings in wasm are negligible and it strips debug info, making debugging \
                        much harder."
                        .to_string(),
                    suggestion: Some(format!("Replace with `{}`", replacement)),
                });
            }

            fn inspect_attrs(&mut self, attrs: &[Attribute]) {
                for attr in attrs {
                    if let Some(paths) = derive_paths(attr) {
                        for path in paths {
                            if path_has_exact_ident(&path, "RuntimeDebug") {
                                self.push_attr_match(path.span(), "Debug");
                            } else if path_has_exact_ident(&path, "RuntimeDebugNoBound") {
                                self.push_attr_match(path.span(), "DebugNoBound");
                            }
                        }
                    }
                }
            }
        }

        impl<'ast> Visit<'ast> for RuntimeDebugVisitor<'_> {
            fn visit_item(&mut self, node: &'ast Item) {
                match node {
                    Item::Struct(item) => self.inspect_attrs(&item.attrs),
                    Item::Enum(item) => self.inspect_attrs(&item.attrs),
                    Item::Union(item) => self.inspect_attrs(&item.attrs),
                    _ => {}
                }
                visit::visit_item(self, node);
            }
        }

        let mut visitor = RuntimeDebugVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
// SEM008: sp_std is deprecated — use alloc
// ---------------------------------------------------------------------------

pub struct SpStdDeprecated;

impl LintRule for SpStdDeprecated {
    fn id(&self) -> &str {
        "SEM008"
    }
    fn name(&self) -> &str {
        "sp-std-deprecated"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        let diagnostics = ctx
            .content
            .lines()
            .enumerate()
            .filter_map(|(idx, line)| {
                let sanitized = strip_strings_and_line_comments(line);
                sanitized.contains("sp_std::").then(|| Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    category: RuleCategory::Semantic,
                    severity: config.rule_severity(self.id(), Severity::Warning),
                    file: ctx.path.clone(),
                    line: idx + 1,
                    column: sanitized.find("sp_std::").map(|col| col + 1),
                    end_line: None,
                    message: "`sp_std` is deprecated — use `alloc` instead".to_string(),
                    explanation: "`sp_std` was deprecated in polkadot-sdk. Use \
                        `extern crate alloc; use alloc::vec::Vec;` for `no_std` compatibility."
                        .to_string(),
                    suggestion: Some(
                        "Replace `sp_std::vec::Vec` with `alloc::vec::Vec`".to_string(),
                    ),
                })
            })
            .collect::<Vec<_>>();

        if diagnostics.is_empty() {
            None
        } else {
            Some(diagnostics)
        }
    }
}

// ---------------------------------------------------------------------------
// SEM009: Redundant contains_key before remove/take
// ---------------------------------------------------------------------------

pub struct RedundantContainsKeyBeforeRemove;

impl LintRule for RedundantContainsKeyBeforeRemove {
    fn id(&self) -> &str {
        "SEM009"
    }
    fn name(&self) -> &str {
        "redundant-contains-key-before-remove"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || ctx.is_test_file {
            return None;
        }

        let ast = ast_file(ctx)?;

        fn storage_call_owner(expr: &Expr) -> Option<String> {
            let Expr::Call(expr_call) = expr else {
                return None;
            };
            let path = expr_call_path(expr_call)?;
            path_owner_name(path)
        }

        struct RemoveTakeFinder {
            target_owner: String,
            found_line: Option<usize>,
        }

        impl<'ast> Visit<'ast> for RemoveTakeFinder {
            fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                let Some(path) = expr_call_path(expr_call) else {
                    visit::visit_expr_call(self, expr_call);
                    return;
                };
                let is_remove =
                    path_has_exact_ident(path, "remove") || path_has_exact_ident(path, "take");
                if is_remove && path_owner_name(path).as_deref() == Some(self.target_owner.as_str())
                {
                    self.found_line = Some(span_line(expr_call.span()));
                }
                visit::visit_expr_call(self, expr_call);
            }
        }

        struct ContainsKeyVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for ContainsKeyVisitor<'_> {
            fn visit_expr_if(&mut self, expr_if: &'ast syn::ExprIf) {
                let Some(owner) = storage_call_owner(&expr_if.cond) else {
                    visit::visit_expr_if(self, expr_if);
                    return;
                };
                let Expr::Call(expr_call) = &*expr_if.cond else {
                    visit::visit_expr_if(self, expr_if);
                    return;
                };
                let Some(path) = expr_call_path(expr_call) else {
                    visit::visit_expr_if(self, expr_if);
                    return;
                };
                if !path_has_exact_ident(path, "contains_key") {
                    visit::visit_expr_if(self, expr_if);
                    return;
                }

                let mut finder = RemoveTakeFinder {
                    target_owner: owner.clone(),
                    found_line: None,
                };
                finder.visit_block(&expr_if.then_branch);
                if let Some(remove_line) = finder.found_line {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(expr_call.span()),
                        column: Some(span_column(expr_call.span())),
                        end_line: Some(remove_line),
                        message: format!(
							"`{owner}::contains_key()` before `{owner}::remove()`/`take()` is a wasted storage read"
						),
                        explanation:
                            "`remove()` is idempotent — it does nothing if the key doesn't exist. \
                            The `contains_key()` check is an unnecessary extra storage read."
                                .to_string(),
                        suggestion: Some(
                            "Remove the `contains_key()` check and call `remove()` directly"
                                .to_string(),
                        ),
                    });
                }
                visit::visit_expr_if(self, expr_if);
            }
        }

        let mut visitor = ContainsKeyVisitor {
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
// SEM010: ^ used as exponentiation (Rust XOR bug)
// ---------------------------------------------------------------------------

pub struct XorAsExponentiation;

impl LintRule for XorAsExponentiation {
    fn id(&self) -> &str {
        "SEM010"
    }
    fn name(&self) -> &str {
        "xor-as-exponentiation"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        let ast = ast_file(ctx)?;

        fn int_literal(expr: &Expr) -> Option<u64> {
            let Expr::Lit(expr_lit) = expr else {
                return None;
            };
            let Lit::Int(lit_int) = &expr_lit.lit else {
                return None;
            };
            lit_int.base10_parse().ok()
        }

        struct XorVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for XorVisitor<'_> {
            fn visit_expr_binary(&mut self, node: &'ast ExprBinary) {
                if !matches!(node.op, syn::BinOp::BitXor(_)) {
                    visit::visit_expr_binary(self, node);
                    return;
                }
                let (Some(base), Some(exp)) = (int_literal(&node.left), int_literal(&node.right))
                else {
                    visit::visit_expr_binary(self, node);
                    return;
                };
                if (base == 10 || base == 2 || base == 100) && exp > 3 {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(node.span()),
                        column: Some(span_column(node.span())),
                        end_line: None,
                        message: format!(
                            "`{} ^ {}` is bitwise XOR (= {}), not exponentiation",
                            base,
                            exp,
                            base ^ exp
                        ),
                        explanation: "In Rust, `^` is bitwise XOR, not exponentiation. \
                            `10 ^ 16` evaluates to `26`, not `10000000000000000`."
                            .to_string(),
                        suggestion: Some(format!("Use `{}_u128.pow({})` instead", base, exp)),
                    });
                }
                visit::visit_expr_binary(self, node);
            }
        }

        let mut visitor = XorVisitor {
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
// SEM011: Weight::zero() placeholder in weight attributes
// ---------------------------------------------------------------------------

pub struct WeightZeroPlaceholder;

impl LintRule for WeightZeroPlaceholder {
    fn id(&self) -> &str {
        "SEM011"
    }
    fn name(&self) -> &str {
        "weight-zero-placeholder"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || ctx.is_test_file {
            return None;
        }

        let ast = ast_file(ctx)?;

        fn is_weight_attr(attr: &Attribute) -> bool {
            attr_path_matches(attr, &["pallet", "weight"])
                || attr_path_matches(attr, &["pallet", "weight_of_authorize"])
        }

        struct WeightZeroVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl WeightZeroVisitor<'_> {
            fn inspect_attrs(&mut self, attrs: &[Attribute]) {
                for attr in attrs.iter().filter(|attr| is_weight_attr(attr)) {
                    let Some(expr) = attr_expr(attr) else {
                        continue;
                    };
                    let Expr::Call(expr_call) = expr else {
                        continue;
                    };
                    let Some(path) = expr_call_path(&expr_call) else {
                        continue;
                    };
                    if path_has_segment(path, "Weight") && path_has_exact_ident(path, "zero") {
                        self.diagnostics.push(Diagnostic {
							rule_id: self.rule_id.to_string(),
							rule_name: self.rule_name.to_string(),
							category: RuleCategory::Semantic,
							severity: self.severity,
							file: self.file.to_path_buf(),
							line: span_line(attr.span()),
							column: Some(span_column(attr.span())),
							end_line: None,
							message: "`Weight::zero()` placeholder in weight attribute".to_string(),
							explanation: "`Weight::zero()` means the extrinsic is free, which is \
                                almost certainly wrong. Replace with an actual benchmarked weight function."
								.to_string(),
							suggestion: Some(
								"Add a benchmark and use `T::WeightInfo::your_function()`".to_string(),
							),
						});
                    }
                }
            }
        }

        impl<'ast> Visit<'ast> for WeightZeroVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                self.inspect_attrs(&item_fn.attrs);
                visit::visit_item_fn(self, item_fn);
            }

            fn visit_impl_item(&mut self, item: &'ast ImplItem) {
                if let ImplItem::Fn(item_fn) = item {
                    self.inspect_attrs(&item_fn.attrs);
                }
                visit::visit_impl_item(self, item);
            }
        }

        let mut visitor = WeightZeroVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
// VAL002: Division by config/storage value without zero guard
// ---------------------------------------------------------------------------
// constants or storage values used as divisors without a preceding zero
// check. If the value is 0, the runtime panics.

pub struct DivisionWithoutZeroGuard;

impl LintRule for DivisionWithoutZeroGuard {
    fn id(&self) -> &str {
        "VAL002"
    }
    fn name(&self) -> &str {
        "division-without-zero-guard"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = cfg_test_module_mask(ctx.content);

        fn risky_divisor_key(expr: &Expr) -> Option<String> {
            match strip_expr_wrappers(expr) {
                Expr::Path(expr_path) => Some(expr_path.path.to_token_stream().to_string()),
                Expr::Call(expr_call) => {
                    let path = expr_call_path(expr_call)?;
                    if path_has_exact_ident(path, "get") {
                        Some(path.to_token_stream().to_string())
                    } else {
                        None
                    }
                }
                Expr::MethodCall(expr_method_call) => {
                    if matches!(
                        expr_method_call.method.to_string().as_str(),
                        "len" | "count"
                    ) {
                        Some(compact_tokens(expr))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }

        fn has_zero_guard_before(statements: &[(usize, String)], line: usize, key: &str) -> bool {
            let normalized_key: String = key.chars().filter(|c| !c.is_whitespace()).collect();
            statements.iter().any(|(stmt_line, text)| {
                *stmt_line < line
                    && *stmt_line + 10 >= line
                    && (text.contains("checked_div")
                        || text.contains("saturating_div")
                        || ((text.contains("ensure!") || text.contains(&normalized_key))
                            && (text.contains("!=0")
                                || text.contains(">0")
                                || text.contains(">=1")
                                || text.contains(".is_zero()"))))
            })
        }

        struct DivisionVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
        }

        impl DivisionVisitor<'_> {
            fn inspect_fn(&mut self, block: &syn::Block) {
                let mut risky_bindings = std::collections::HashMap::new();
                let mut statements = Vec::new();

                for stmt in &block.stmts {
                    let line = span_line(stmt.span());
                    let text = compact_tokens(stmt);
                    statements.push((line, text.clone()));

                    if let syn::Stmt::Local(local) = stmt {
                        if let Some(init) = &local.init {
                            if let Some(key) = risky_divisor_key(&init.expr) {
                                if let Pat::Ident(pat_ident) = &local.pat {
                                    risky_bindings.insert(pat_ident.ident.to_string(), key);
                                }
                            }
                        }
                    }
                }

                struct BinaryVisitor<'a> {
                    risky_bindings: &'a std::collections::HashMap<String, String>,
                    statements: &'a [(usize, String)],
                    diagnostics: &'a mut Vec<Diagnostic>,
                    file: &'a Path,
                    severity: Severity,
                    rule_id: &'a str,
                    rule_name: &'a str,
                    mask: &'a [bool],
                }

                impl<'ast> Visit<'ast> for BinaryVisitor<'_> {
                    fn visit_expr_binary(&mut self, expr_binary: &'ast ExprBinary) {
                        if is_masked_span(self.mask, expr_binary.span())
                            || !matches!(expr_binary.op, syn::BinOp::Div(_))
                        {
                            visit::visit_expr_binary(self, expr_binary);
                            return;
                        }

                        let rhs = strip_expr_wrappers(&expr_binary.right);
                        let risk_key = match rhs {
                            Expr::Path(expr_path) => self
                                .risky_bindings
                                .get(&expr_path.path.to_token_stream().to_string())
                                .cloned(),
                            _ => risky_divisor_key(rhs),
                        };

                        if let Some(key) = risk_key {
                            let line = span_line(expr_binary.span());
                            if !has_zero_guard_before(self.statements, line, &key) {
                                self.diagnostics.push(Diagnostic {
                                    rule_id: self.rule_id.to_string(),
                                    rule_name: self.rule_name.to_string(),
                                    category: RuleCategory::Semantic,
                                    severity: self.severity,
                                    file: self.file.to_path_buf(),
                                    line,
                                    column: Some(span_column(expr_binary.span())),
                                    end_line: None,
                                    message:
                                        "Division by config/storage value without a preceding zero guard"
                                            .to_string(),
                                    explanation: "If the divisor is zero, the runtime panics. Config constants \
                                        should have an `integrity_test` asserting non-zero, and dynamic values \
                                        (`.len()`, storage reads) should have an explicit zero check or use \
                                        `checked_div`."
                                        .to_string(),
                                    suggestion: Some(
                                        "Add `ensure!(divisor != 0, ...)` before the division, use `.checked_div()`, or add an `integrity_test` for config values"
                                            .to_string(),
                                    ),
                                });
                            }
                        }

                        visit::visit_expr_binary(self, expr_binary);
                    }
                }

                let mut visitor = BinaryVisitor {
                    risky_bindings: &risky_bindings,
                    statements: &statements,
                    diagnostics: &mut self.diagnostics,
                    file: self.file,
                    severity: self.severity,
                    rule_id: self.rule_id,
                    rule_name: self.rule_name,
                    mask: self.mask,
                };
                visitor.visit_block(block);
            }
        }

        impl<'ast> Visit<'ast> for DivisionVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                self.inspect_fn(&item_fn.block);
                visit::visit_item_fn(self, item_fn);
            }

            fn visit_impl_item(&mut self, item: &'ast ImplItem) {
                if let ImplItem::Fn(item_fn) = item {
                    self.inspect_fn(&item_fn.block);
                }
                visit::visit_impl_item(self, item);
            }
        }

        let mut visitor = DivisionVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
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
// SEM012: #[allow(dead_code)] in production pallet code
// ---------------------------------------------------------------------------
// in pallet code means the dead code should be removed, not silenced.

pub struct AllowDeadCodeInPallet;

impl LintRule for AllowDeadCodeInPallet {
    fn id(&self) -> &str {
        "SEM012"
    }
    fn name(&self) -> &str {
        "allow-dead-code-in-pallet"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        // Skip auto-generated weight files which legitimately need #![allow(dead_code)]
        let path_str = ctx.rel_path.to_string_lossy();
        if path_str.contains("/weights/") || path_str.ends_with("weights.rs") {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct DeadCodeVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl DeadCodeVisitor<'_> {
            fn check_attrs(&mut self, attrs: &[Attribute]) {
                for attr in attrs {
                    if !is_masked_span(self.mask, attr.span()) && attr_contains_dead_code(attr) {
                        self.diagnostics.push(Diagnostic {
							rule_id: self.rule_id.to_string(),
							rule_name: self.rule_name.to_string(),
							category: RuleCategory::Semantic,
							severity: self.severity,
							file: self.file.to_path_buf(),
							line: span_line(attr.span()),
							column: Some(span_column(attr.span())),
							end_line: None,
							message:
								"`#[allow(dead_code)]` in production code — remove the dead code instead"
									.to_string(),
							explanation: "Suppressing dead_code warnings hides unused functions, types, \
                            or fields that should be removed. Dead code adds maintenance burden and \
                            can mask real issues."
								.to_string(),
							suggestion: Some(
								"Remove the dead code instead of suppressing the warning".to_string(),
							),
						});
                    }
                }
            }
        }

        impl<'ast> Visit<'ast> for DeadCodeVisitor<'_> {
            fn visit_file(&mut self, file: &'ast SynFile) {
                self.check_attrs(&file.attrs);
                visit::visit_file(self, file);
            }

            fn visit_item(&mut self, node: &'ast Item) {
                match node {
                    Item::Const(item) => self.check_attrs(&item.attrs),
                    Item::Enum(item) => self.check_attrs(&item.attrs),
                    Item::ExternCrate(item) => self.check_attrs(&item.attrs),
                    Item::Fn(item) => self.check_attrs(&item.attrs),
                    Item::Impl(item) => self.check_attrs(&item.attrs),
                    Item::Macro(item) => self.check_attrs(&item.attrs),
                    Item::Mod(item) => self.check_attrs(&item.attrs),
                    Item::Static(item) => self.check_attrs(&item.attrs),
                    Item::Struct(item) => self.check_attrs(&item.attrs),
                    Item::Trait(item) => self.check_attrs(&item.attrs),
                    Item::TraitAlias(item) => self.check_attrs(&item.attrs),
                    Item::Type(item) => self.check_attrs(&item.attrs),
                    Item::Union(item) => self.check_attrs(&item.attrs),
                    Item::Use(item) => self.check_attrs(&item.attrs),
                    _ => {}
                }
                visit::visit_item(self, node);
            }
        }

        let mut visitor = DeadCodeVisitor {
            mask: &test_mask,
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
// SEC001: Unbounded Vec<T> in extrinsic parameters
// ---------------------------------------------------------------------------
// to pass arbitrarily large inputs, causing DoS via memory/weight exhaustion.
// Use BoundedVec<T, S> instead.

pub struct UnboundedVecInExtrinsic;

impl LintRule for UnboundedVecInExtrinsic {
    fn id(&self) -> &str {
        "SEC001"
    }
    fn name(&self) -> &str {
        "unbounded-vec-in-extrinsic"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct ExtrinsicVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for ExtrinsicVisitor<'_> {
            fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
                if !has_attr(&item.attrs, &["pallet", "call"])
                    || is_masked_span(self.mask, item.span())
                {
                    return;
                }

                for impl_item in &item.items {
                    let method = match impl_item {
                        ImplItem::Fn(method) => method,
                        _ => continue,
                    };

                    if !has_attr(&method.attrs, &["pallet", "call_index"])
                        || is_masked_span(self.mask, method.span())
                    {
                        continue;
                    }

                    let has_unbounded_vec = method.sig.inputs.iter().any(|arg| match arg {
                        FnArg::Typed(pat_type) => {
                            type_contains_named(&pat_type.ty, &["Vec"])
                                && !type_contains_named(
                                    &pat_type.ty,
                                    &["BoundedVec", "WeakBoundedVec"],
                                )
                        }
                        FnArg::Receiver(_) => false,
                    });

                    if has_unbounded_vec {
                        self.diagnostics.push(Diagnostic {
							rule_id: self.rule_id.to_string(),
							rule_name: self.rule_name.to_string(),
							category: RuleCategory::Semantic,
							severity: self.severity,
							file: self.file.to_path_buf(),
							line: span_line(method.sig.ident.span()),
							column: Some(span_column(method.sig.ident.span())),
							end_line: None,
							message: format!(
								"Extrinsic `{}` accepts unbounded `Vec<T>` parameter",
								method.sig.ident
							),
							explanation: "Unbounded `Vec<T>` in extrinsic parameters allows attackers \
                                to pass arbitrarily large inputs, causing memory exhaustion or \
                                overweight blocks. Use `BoundedVec<T, MaxLen>` to enforce an upper bound."
								.to_string(),
							suggestion: Some(
								"Replace `Vec<T>` with `BoundedVec<T, ConstU32<MAX>>`".to_string(),
							),
						});
                    }
                }

                visit::visit_item_impl(self, item);
            }
        }

        let mut visitor = ExtrinsicVisitor {
            mask: &test_mask,
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
// SEC002: debug_assert! in production code
// ---------------------------------------------------------------------------
// builds and is stripped in release. Neither behaviour is correct for
// handling user-reachable conditions.

pub struct DebugAssertInProduction;

impl LintRule for DebugAssertInProduction {
    fn id(&self) -> &str {
        "SEC002"
    }
    fn name(&self) -> &str {
        "debug-assert-in-production"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;
        let mut visitor = MacroNameVisitor {
            names: &["debug_assert", "debug_assert_eq", "debug_assert_ne"],
            matches: Vec::new(),
        };
        visitor.visit_file(ast);

        let diagnostics = visitor
            .matches
            .into_iter()
            .filter(|(_, span)| !is_masked_span(&test_mask, *span))
            .map(|(_, span)| Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                category: RuleCategory::Semantic,
                severity: config.rule_severity(self.id(), Severity::Warning),
                file: ctx.path.clone(),
                line: span_line(span),
                column: Some(span_column(span)),
                end_line: None,
                message:
                    "`debug_assert!` in production code — panics in debug, stripped in release"
                        .to_string(),
                explanation: "`debug_assert!` panics in debug builds and is completely removed \
                    in release builds. Neither is correct for runtime code: panics crash the \
                    node, and silent removal means the invariant is unchecked. Use \
                    `defensive!()` or a proper error return instead."
                    .to_string(),
                suggestion: Some("Replace with `defensive!()` or return an error".to_string()),
            })
            .collect::<Vec<_>>();

        if diagnostics.is_empty() {
            None
        } else {
            Some(diagnostics)
        }
    }
}

// ---------------------------------------------------------------------------
// SEC003: Missing decode depth limit
// ---------------------------------------------------------------------------
// call data without a depth limit allows stack exhaustion via deeply
// nested calls (e.g., batch(batch(batch(...)))).

pub struct MissingDecodeDepthLimit;

impl LintRule for MissingDecodeDepthLimit {
    fn id(&self) -> &str {
        "SEC003"
    }
    fn name(&self) -> &str {
        "missing-decode-depth-limit"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = cfg_test_module_mask(ctx.content);

        struct DecodeVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
        }

        impl<'ast> Visit<'ast> for DecodeVisitor<'_> {
            fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                if is_masked_span(self.mask, expr_call.span()) {
                    visit::visit_expr_call(self, expr_call);
                    return;
                }
                let Some(path) = expr_call_path(expr_call) else {
                    visit::visit_expr_call(self, expr_call);
                    return;
                };
                if path_has_exact_ident(path, "decode")
                    && !path_has_segment(path, "DecodeLimit")
                    && !path_has_exact_ident(path, "decode_with_depth_limit")
                    && !path_has_exact_ident(path, "decode_all_with_depth_limit")
                {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(expr_call.span()),
                        column: Some(span_column(expr_call.span())),
                        end_line: None,
                        message: "`Decode::decode()` without depth limit — risk of stack exhaustion"
                            .to_string(),
                        explanation: "Decoding user-supplied data without a recursion depth limit \
                            allows attackers to craft deeply nested structures (e.g., \
                            `batch(batch(batch(...)))`) that exhaust the stack. Use \
                            `decode_with_depth_limit(MAX_DEPTH, &mut input)` instead."
                            .to_string(),
                        suggestion: Some(
                            "Replace with `Decode::decode_with_depth_limit(sp_io::MAX_EXTRINSIC_DEPTH, &mut input)`"
                                .to_string(),
                        ),
                    });
                }
                visit::visit_expr_call(self, expr_call);
            }

            fn visit_expr_method_call(&mut self, expr_method_call: &'ast ExprMethodCall) {
                if is_masked_span(self.mask, expr_method_call.span()) {
                    visit::visit_expr_method_call(self, expr_method_call);
                    return;
                }
                if expr_method_call.method == "decode" {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(expr_method_call.span()),
                        column: Some(span_column(expr_method_call.span())),
                        end_line: None,
                        message: "`Decode::decode()` without depth limit — risk of stack exhaustion"
                            .to_string(),
                        explanation: "Decoding user-supplied data without a recursion depth limit \
                            allows attackers to craft deeply nested structures (e.g., \
                            `batch(batch(batch(...)))`) that exhaust the stack. Use \
                            `decode_with_depth_limit(MAX_DEPTH, &mut input)` instead."
                            .to_string(),
                        suggestion: Some(
                            "Replace with `Decode::decode_with_depth_limit(sp_io::MAX_EXTRINSIC_DEPTH, &mut input)`"
                                .to_string(),
                        ),
                    });
                }
                visit::visit_expr_method_call(self, expr_method_call);
            }
        }

        let mut visitor = DecodeVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
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
// SEC004: Unsafe arithmetic in weight attributes
// ---------------------------------------------------------------------------
// inside #[pallet::weight(...)] can overflow, producing a tiny weight
// that allows overweight blocks.

pub struct UnsafeWeightArithmetic;

impl LintRule for UnsafeWeightArithmetic {
    fn id(&self) -> &str {
        "SEC004"
    }
    fn name(&self) -> &str {
        "unsafe-weight-arithmetic"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = cfg_test_module_mask(ctx.content);

        fn is_weight_attr(attr: &Attribute) -> bool {
            attr_path_matches(attr, &["pallet", "weight"])
        }

        struct WeightArithmeticVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for WeightArithmeticVisitor<'_> {
            fn visit_expr_binary(&mut self, expr_binary: &'ast ExprBinary) {
                if matches!(expr_binary.op, syn::BinOp::Add(_) | syn::BinOp::Mul(_)) {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(expr_binary.span()),
                        column: Some(span_column(expr_binary.span())),
                        end_line: None,
                        message:
                            "Non-saturating arithmetic inside `#[pallet::weight(...)]` — overflow risk"
                                .to_string(),
                        explanation: "Arithmetic overflow in weight calculation produces a tiny \
                            weight value in release builds, allowing overweight blocks that can \
                            stall the chain. Use `saturating_add`/`saturating_mul` instead."
                            .to_string(),
                        suggestion: Some(
                            "Replace `.add()` with `.saturating_add()` and `.mul()` with `.saturating_mul()`"
                                .to_string(),
                        ),
                    });
                }
                visit::visit_expr_binary(self, expr_binary);
            }

            fn visit_expr_method_call(&mut self, expr_method_call: &'ast ExprMethodCall) {
                if matches!(expr_method_call.method.to_string().as_str(), "add" | "mul") {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(expr_method_call.span()),
                        column: Some(span_column(expr_method_call.span())),
                        end_line: None,
                        message:
                            "Non-saturating arithmetic inside `#[pallet::weight(...)]` — overflow risk"
                                .to_string(),
                        explanation: "Arithmetic overflow in weight calculation produces a tiny \
                            weight value in release builds, allowing overweight blocks that can \
                            stall the chain. Use `saturating_add`/`saturating_mul` instead."
                            .to_string(),
                        suggestion: Some(
                            "Replace `.add()` with `.saturating_add()` and `.mul()` with `.saturating_mul()`"
                                .to_string(),
                        ),
                    });
                }
                visit::visit_expr_method_call(self, expr_method_call);
            }
        }

        let mut diagnostics = Vec::new();
        for attr in ast
            .items
            .iter()
            .flat_map(|item| match item {
                Item::Impl(item_impl) => item_impl
                    .items
                    .iter()
                    .filter_map(|impl_item| match impl_item {
                        ImplItem::Fn(item_fn) => Some(&item_fn.attrs),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                Item::Fn(item_fn) => vec![&item_fn.attrs],
                _ => Vec::new(),
            })
            .flatten()
        {
            if !is_weight_attr(attr) || is_masked_span(&test_mask, attr.span()) {
                continue;
            }
            let Some(expr) = attr_expr(attr) else {
                continue;
            };
            let mut visitor = WeightArithmeticVisitor {
                diagnostics: Vec::new(),
                file: &ctx.path,
                severity: config.rule_severity(self.id(), Severity::Warning),
                rule_id: self.id(),
                rule_name: self.name(),
            };
            visitor.visit_expr(&expr);
            diagnostics.extend(visitor.diagnostics);
        }

        if diagnostics.is_empty() {
            None
        } else {
            Some(diagnostics)
        }
    }
}

// ---------------------------------------------------------------------------
// SEC005: Expensive operations in weight calculation
// ---------------------------------------------------------------------------
// since they run before dispatch. DB reads, encoding, or get_dispatch_info
// inside weight attrs create DoS vectors.

pub struct ExpensiveWeightCalculation;

impl LintRule for ExpensiveWeightCalculation {
    fn id(&self) -> &str {
        "SEC005"
    }
    fn name(&self) -> &str {
        "expensive-weight-calculation"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = cfg_test_module_mask(ctx.content);

        fn is_weight_attr(attr: &Attribute) -> bool {
            attr_path_matches(attr, &["pallet", "weight"])
        }

        struct ExpensiveWeightVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for ExpensiveWeightVisitor<'_> {
            fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                if let Some(path) = expr_call_path(expr_call) {
                    let last = path_last_ident(path);
                    let owner = path_owner_name(path);
                    let is_expensive = matches!(last.as_deref(), Some("get" | "get_dispatch_info"));
                    let compact_path = compact_tokens(path);
                    let allow_weight_info_get = owner.as_deref() == Some("WeightInfo");
                    let allow_config_get = compact_path.starts_with("T::");
                    if is_expensive && !allow_weight_info_get && !allow_config_get {
                        self.diagnostics.push(Diagnostic {
                            rule_id: self.rule_id.to_string(),
                            rule_name: self.rule_name.to_string(),
                            category: RuleCategory::Semantic,
                            severity: self.severity,
                            file: self.file.to_path_buf(),
                            line: span_line(expr_call.span()),
                            column: Some(span_column(expr_call.span())),
                            end_line: None,
                            message:
                                "Expensive operation inside `#[pallet::weight(...)]` — DoS risk"
                                    .to_string(),
                            explanation: "Weight is computed before dispatch to decide if the extrinsic \
                                fits in the block. Storage reads, encoding, and `get_dispatch_info()` \
                                inside weight attributes can be exploited for DoS. Weight functions \
                                must be purely arithmetic."
                                .to_string(),
                            suggestion: Some(
                                "Move expensive operations out of the weight attribute; use only pre-benchmarked weight functions"
                                    .to_string(),
                            ),
                        });
                    }
                }
                visit::visit_expr_call(self, expr_call);
            }

            fn visit_expr_method_call(&mut self, expr_method_call: &'ast ExprMethodCall) {
                if matches!(
                    expr_method_call.method.to_string().as_str(),
                    "encode" | "decode" | "using_encoded" | "get_dispatch_info"
                ) {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(expr_method_call.span()),
                        column: Some(span_column(expr_method_call.span())),
                        end_line: None,
                        message: "Expensive operation inside `#[pallet::weight(...)]` — DoS risk"
                            .to_string(),
                        explanation: "Weight is computed before dispatch to decide if the extrinsic \
                            fits in the block. Storage reads, encoding, and `get_dispatch_info()` \
                            inside weight attributes can be exploited for DoS. Weight functions \
                            must be purely arithmetic."
                            .to_string(),
                        suggestion: Some(
                            "Move expensive operations out of the weight attribute; use only pre-benchmarked weight functions"
                                .to_string(),
                        ),
                    });
                }
                visit::visit_expr_method_call(self, expr_method_call);
            }
        }

        let mut diagnostics = Vec::new();
        for attr in ast
            .items
            .iter()
            .flat_map(|item| match item {
                Item::Impl(item_impl) => item_impl
                    .items
                    .iter()
                    .filter_map(|impl_item| match impl_item {
                        ImplItem::Fn(item_fn) => Some(&item_fn.attrs),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                Item::Fn(item_fn) => vec![&item_fn.attrs],
                _ => Vec::new(),
            })
            .flatten()
        {
            if !is_weight_attr(attr) || is_masked_span(&test_mask, attr.span()) {
                continue;
            }
            let Some(expr) = attr_expr(attr) else {
                continue;
            };
            let mut visitor = ExpensiveWeightVisitor {
                diagnostics: Vec::new(),
                file: &ctx.path,
                severity: config.rule_severity(self.id(), Severity::Warning),
                rule_id: self.id(),
                rule_name: self.name(),
            };
            visitor.visit_expr(&expr);
            diagnostics.extend(visitor.diagnostics);
        }

        if diagnostics.is_empty() {
            None
        } else {
            Some(diagnostics)
        }
    }
}

// ---------------------------------------------------------------------------
// SEC006: Unchecked repatriate_reserved return value
// ---------------------------------------------------------------------------
// best-effort basis — it returns Ok(remaining) where remaining > 0 means
// not all funds were transferred. Ignoring this causes accounting errors.

pub struct UncheckedRepatriateReserved;

impl LintRule for UncheckedRepatriateReserved {
    fn id(&self) -> &str {
        "SEC006"
    }
    fn name(&self) -> &str {
        "unchecked-repatriate-reserved"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = cfg_test_module_mask(ctx.content);
        let lines: Vec<&str> = ctx.content.lines().collect();

        fn is_repatriate_reserved_expr(expr: &Expr) -> bool {
            struct Finder {
                found: bool,
            }

            impl<'ast> Visit<'ast> for Finder {
                fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                    if expr_call_path(expr_call)
                        .map(|path| path_has_exact_ident(path, "repatriate_reserved"))
                        .unwrap_or(false)
                    {
                        self.found = true;
                    }
                    visit::visit_expr_call(self, expr_call);
                }

                fn visit_expr_method_call(&mut self, expr_method_call: &'ast ExprMethodCall) {
                    if expr_method_call.method == "repatriate_reserved" {
                        self.found = true;
                    }
                    visit::visit_expr_method_call(self, expr_method_call);
                }
            }

            let mut finder = Finder { found: false };
            finder.visit_expr(expr);
            finder.found
        }

        fn has_remaining_check(lines: &[&str], mask: &[bool], line: usize, name: &str) -> bool {
            let window_end = (line + 12).min(lines.len());
            (line..window_end).any(|j| {
                !mask[j]
                    && lines[j].contains(name)
                    && (lines[j].contains(".is_zero()")
                        || lines[j].contains("== 0")
                        || lines[j].contains("!= 0")
                        || lines[j].contains("> 0")
                        || lines[j].contains(">= 1")
                        || lines[j].contains("ensure!")
                        || lines[j].trim_start().starts_with("if ")
                        || lines[j].trim_start().starts_with("match "))
            })
        }

        struct RepatriateVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
            lines: &'a [&'a str],
        }

        impl<'ast> Visit<'ast> for RepatriateVisitor<'_> {
            fn visit_local(&mut self, local: &'ast Local) {
                if is_masked_span(self.mask, local.span()) {
                    visit::visit_local(self, local);
                    return;
                }
                let Some(init) = &local.init else {
                    visit::visit_local(self, local);
                    return;
                };
                if !is_repatriate_reserved_expr(&init.expr) {
                    visit::visit_local(self, local);
                    return;
                }

                match &local.pat {
                    Pat::Wild(_) => self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(local.span()),
                        column: Some(span_column(local.span())),
                        end_line: None,
                        message:
                            "Return value of `repatriate_reserved` is discarded — accounting risk"
                                .to_string(),
                        explanation: "`repatriate_reserved` operates in best-effort mode: it may \
                            transfer less than requested if funds are locked/frozen, and returns \
                            `Ok(remaining)`. Ignoring the remaining amount creates accounting \
                            discrepancies where deposits appear transferred but aren't."
                            .to_string(),
                        suggestion: Some(
                            "Check the returned `remaining` amount: `let remaining = repatriate_reserved(...)?; ensure!(remaining.is_zero(), ...)`"
                                .to_string(),
                        ),
                    }),
                    Pat::Ident(pat_ident) => {
                        let name = pat_ident.ident.to_string();
                        if !has_remaining_check(
                            self.lines,
                            self.mask,
                            span_line(local.span()),
                            &name,
                        ) {
                            self.diagnostics.push(Diagnostic {
                                rule_id: self.rule_id.to_string(),
                                rule_name: self.rule_name.to_string(),
                                category: RuleCategory::Semantic,
                                severity: self.severity,
                                file: self.file.to_path_buf(),
                                line: span_line(local.span()),
                                column: Some(span_column(local.span())),
                                end_line: None,
                                message: format!(
                                    "`repatriate_reserved` return value is bound to `{}` but never checked",
                                    name
                                ),
                                explanation: "`repatriate_reserved` returns `Ok(remaining)`, where \
                                    `remaining > 0` means part of the transfer failed. Binding the \
                                    result without checking it still risks accounting mismatches."
                                    .to_string(),
                                suggestion: Some(format!(
                                    "Add an explicit check such as `ensure!({}.is_zero(), ...)`",
                                    name
                                )),
                            });
                        }
                    }
                    _ => {}
                }

                visit::visit_local(self, local);
            }
        }

        let mut visitor = RepatriateVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
            lines: &lines,
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
// SEC007: `let _ =` swallowing Result in production code
// ---------------------------------------------------------------------------
// reviewer: "not use `let _ =` when propagating errors. If the
// `?` is accidentally forgotten, the error gets silently swallowed."
// reviewer: "Capturing a problem with `let _` seems like we
// can really shoot ourselves in the foot."

pub struct LetUnderscoreResult;

impl LintRule for LetUnderscoreResult {
    fn id(&self) -> &str {
        "SEC007"
    }
    fn name(&self) -> &str {
        "let-underscore-result"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = cfg_test_module_mask(ctx.content);
        let result_hints = [
            "::reserve(",
            "::unreserve(",
            "::transfer(",
            "::withdraw(",
            "::deposit(",
            "::send(",
            "::execute(",
            "::dispatch(",
            "::mutate(",
            "::try_mutate(",
            "::try_push(",
            "::try_append(",
            ".map_err(",
        ];

        struct LetUnderscoreVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
            result_hints: &'a [&'a str],
        }

        impl<'ast> Visit<'ast> for LetUnderscoreVisitor<'_> {
            fn visit_local(&mut self, local: &'ast Local) {
                if is_masked_span(self.mask, local.span()) || !matches!(local.pat, Pat::Wild(_)) {
                    visit::visit_local(self, local);
                    return;
                }
                let Some(init) = &local.init else {
                    visit::visit_local(self, local);
                    return;
                };
                let rhs: String = init
                    .expr
                    .to_token_stream()
                    .to_string()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                if self.result_hints.iter().any(|hint| rhs.contains(hint)) {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::Semantic,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line: span_line(local.span()),
						column: Some(span_column(local.span())),
						end_line: None,
						message: "`let _ =` discards a likely Result — error silently swallowed"
							.to_string(),
						explanation: "Using `let _ =` on a Result-returning function silently discards \
                            the error. If `?` is accidentally omitted, the error is swallowed without \
                            any compiler warning since `let _ =` explicitly suppresses the `#[must_use]` lint."
							.to_string(),
						suggestion: Some(
							"Propagate the error with `?`, or handle it explicitly. If intentionally ignoring, add a comment explaining why."
								.to_string(),
						),
					});
                }
                visit::visit_local(self, local);
            }
        }

        let mut visitor = LetUnderscoreVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
            result_hints: &result_hints,
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
// SEC008: Panic-capable code in production pallets/runtimes
// ---------------------------------------------------------------------------
// reviewer (~10): "use defensive! instead of unwrap/panic"

pub struct PanicInProduction;

impl LintRule for PanicInProduction {
    fn id(&self) -> &str {
        "SEC008"
    }
    fn name(&self) -> &str {
        "panic-in-production"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        // Skip auto-generated weights files
        let path_str = ctx.rel_path.to_string_lossy();
        if path_str.ends_with("weights.rs") || path_str.contains("/weights/") {
            return None;
        }

        // Genesis config presets intentionally panic on bad configuration —
        // if the genesis state is invalid, the chain cannot function.
        let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name == "genesis_config_presets.rs" {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct PanicVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
        }

        impl PanicVisitor<'_> {
            fn push_diag(&mut self, span: Span, pattern: &str, suggestion: &str) {
                self.diagnostics.push(Diagnostic {
					rule_id: self.rule_id.to_string(),
					rule_name: self.rule_name.to_string(),
					category: RuleCategory::Semantic,
					severity: self.severity,
					file: self.file.to_path_buf(),
					line: span_line(span),
					column: Some(span_column(span)),
					end_line: None,
					message: format!("`{pattern}` in production code — can panic and halt the chain"),
					explanation: "Panics in runtime code crash the node or halt block production. FRAME provides \
                        `defensive!()` for \"should never happen\" paths, which logs an error instead of panicking. \
                        For fallible operations, propagate errors with `?` or use `unwrap_or_default()`."
						.to_string(),
					suggestion: Some(suggestion.to_string()),
				});
            }
        }

        impl<'ast> Visit<'ast> for PanicVisitor<'_> {
            fn visit_expr_method_call(&mut self, expr_method_call: &'ast ExprMethodCall) {
                if is_masked_span(self.mask, expr_method_call.span()) {
                    visit::visit_expr_method_call(self, expr_method_call);
                    return;
                }
                match expr_method_call.method.to_string().as_str() {
                    "unwrap" => self.push_diag(
                        expr_method_call.span(),
                        ".unwrap()",
                        "Use `.ok_or(Error::...)?`, `.unwrap_or_default()`, or `.defensive()`",
                    ),
                    "expect" => self.push_diag(
                        expr_method_call.span(),
                        ".expect()",
                        "Use `.ok_or(Error::...)?` or `.defensive()`",
                    ),
                    _ => {}
                }
                visit::visit_expr_method_call(self, expr_method_call);
            }

            fn visit_macro(&mut self, mac: &'ast Macro) {
                if is_masked_span(self.mask, mac.span()) {
                    visit::visit_macro(self, mac);
                    return;
                }
                match path_last_ident(&mac.path).as_deref() {
                    Some("panic") => self.push_diag(
                        mac.span(),
                        "panic!()",
                        "Return an error or use `defensive!()`",
                    ),
                    Some("unreachable") => self.push_diag(
                        mac.span(),
                        "unreachable!()",
                        "Use `defensive_unreachable!()` or return an error",
                    ),
                    Some("todo") => self.push_diag(
                        mac.span(),
                        "todo!()",
                        "Implement the function or return an error",
                    ),
                    Some("unimplemented") => self.push_diag(
                        mac.span(),
                        "unimplemented!()",
                        "Implement the function or return an error",
                    ),
                    _ => {}
                }
                visit::visit_macro(self, mac);
            }
        }

        let mut visitor = PanicVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
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
// SEC009: Raw arithmetic in fallible functions (wrapping overflow)
// ---------------------------------------------------------------------------
// reviewer (~12): "convention to always use saturating/checked arithmetic"
// reviewer: "if this can return an error, might as well do checked math"
// In release builds, Rust wraps on integer overflow silently.

pub struct RawArithmeticInFallible;

impl LintRule for RawArithmeticInFallible {
    fn id(&self) -> &str {
        "SEC009"
    }
    fn name(&self) -> &str {
        "raw-arithmetic-in-fallible"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let path_str = ctx.rel_path.to_string_lossy();
        if path_str.ends_with("weights.rs") || path_str.contains("/weights/") {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = cfg_test_module_mask(ctx.content);

        fn is_fallible_output(output: &syn::ReturnType) -> bool {
            match output {
                syn::ReturnType::Default => false,
                syn::ReturnType::Type(_, ty) => {
                    let compact = compact_tokens(ty);
                    compact.contains("Result")
                        || compact.contains("DispatchResult")
                        || compact.contains("TransactionValidity")
                        || compact.contains("ApplyExtrinsicResult")
                }
            }
        }

        fn is_arithmetic_operand_char(ch: char) -> bool {
            ch.is_ascii_alphanumeric() || matches!(ch, '_' | ')' | ']')
        }

        fn is_arithmetic_rhs_char(ch: char) -> bool {
            ch.is_ascii_alphanumeric() || matches!(ch, '_' | '(')
        }

        fn contains_raw_arithmetic_tokens(tokens: &str) -> bool {
            let chars: Vec<char> = tokens.chars().collect();
            for (idx, ch) in chars.iter().enumerate() {
                if !matches!(ch, '+' | '-' | '*') {
                    continue;
                }
                if idx == 0 || idx + 1 >= chars.len() {
                    continue;
                }
                let prev = chars[idx - 1];
                let next = chars[idx + 1];
                if !is_arithmetic_operand_char(prev) || !is_arithmetic_rhs_char(next) {
                    continue;
                }
                if matches!(prev, '<' | '>' | '=' | '!') || matches!(next, '=' | '>') {
                    continue;
                }
                return true;
            }
            false
        }

        struct ArithmeticVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
        }

        impl<'ast> Visit<'ast> for ArithmeticVisitor<'_> {
            fn visit_expr_binary(&mut self, expr_binary: &'ast ExprBinary) {
                if is_masked_span(self.mask, expr_binary.span()) {
                    visit::visit_expr_binary(self, expr_binary);
                    return;
                }
                if matches!(
                    expr_binary.op,
                    syn::BinOp::Add(_) | syn::BinOp::Sub(_) | syn::BinOp::Mul(_)
                ) {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(expr_binary.span()),
                        column: Some(span_column(expr_binary.span())),
                        end_line: None,
                        message: "Raw arithmetic in fallible function — wrapping overflow risk"
                            .to_string(),
                        explanation: "In release builds, integer overflow wraps silently \
                            (e.g., 255u8 + 1 = 0). Since this function returns Result, \
                            use `checked_add`/`saturating_add` to handle overflow explicitly."
                            .to_string(),
                        suggestion: Some(
                            "Replace `a + b` with `a.checked_add(b).ok_or(Error::Overflow)?` or `a.saturating_add(b)`"
                                .to_string(),
                        ),
                    });
                }
                visit::visit_expr_binary(self, expr_binary);
            }

            fn visit_macro(&mut self, mac: &'ast Macro) {
                if is_masked_span(self.mask, mac.span()) {
                    visit::visit_macro(self, mac);
                    return;
                }
                let Some(name) = path_last_ident(&mac.path) else {
                    visit::visit_macro(self, mac);
                    return;
                };
                if name == "ensure" {
                    let tokens = strip_strings_and_line_comments(&mac.tokens.to_string())
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>();
                    if contains_raw_arithmetic_tokens(&tokens)
                        && !tokens.contains("checked_")
                        && !tokens.contains("saturating_")
                        && !tokens.contains("overflowing_")
                        && !tokens.contains("wrapping_")
                    {
                        self.diagnostics.push(Diagnostic {
                            rule_id: self.rule_id.to_string(),
                            rule_name: self.rule_name.to_string(),
                            category: RuleCategory::Semantic,
                            severity: self.severity,
                            file: self.file.to_path_buf(),
                            line: span_line(mac.span()),
                            column: Some(span_column(mac.span())),
                            end_line: None,
                            message: "Raw arithmetic in fallible function — wrapping overflow risk"
                                .to_string(),
                            explanation: "In release builds, integer overflow wraps silently \
                                (e.g., 255u8 + 1 = 0). Since this function returns Result, \
                                use `checked_add`/`saturating_add` to handle overflow explicitly."
                                .to_string(),
                            suggestion: Some(
                                "Replace `a + b` with `a.checked_add(b).ok_or(Error::Overflow)?` or `a.saturating_add(b)`"
                                    .to_string(),
                            ),
                        });
                    }
                }
                visit::visit_macro(self, mac);
            }
        }

        struct FallibleFnVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
        }

        impl<'ast> Visit<'ast> for FallibleFnVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                if is_fallible_output(&item_fn.sig.output) {
                    let mut visitor = ArithmeticVisitor {
                        diagnostics: Vec::new(),
                        file: self.file,
                        severity: self.severity,
                        rule_id: self.rule_id,
                        rule_name: self.rule_name,
                        mask: self.mask,
                    };
                    visitor.visit_block(&item_fn.block);
                    self.diagnostics.extend(visitor.diagnostics);
                }
                visit::visit_item_fn(self, item_fn);
            }

            fn visit_impl_item(&mut self, item: &'ast ImplItem) {
                if let ImplItem::Fn(item_fn) = item {
                    if is_fallible_output(&item_fn.sig.output) {
                        let mut visitor = ArithmeticVisitor {
                            diagnostics: Vec::new(),
                            file: self.file,
                            severity: self.severity,
                            rule_id: self.rule_id,
                            rule_name: self.rule_name,
                            mask: self.mask,
                        };
                        visitor.visit_block(&item_fn.block);
                        self.diagnostics.extend(visitor.diagnostics);
                    }
                }
                visit::visit_impl_item(self, item);
            }
        }

        let mut visitor = FallibleFnVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Advisory),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
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
// VAL003: Storage write before all validations complete
// ---------------------------------------------------------------------------
// reviewer (~6): "we should put the validations upfront"
// Storage writes (::put, ::insert, ::mutate) before ensure! checks means
// if a later validation fails, the write persists (in hooks) or gets
// rolled back expensively (in extrinsics). Either way it's wrong.

pub struct StorageWriteBeforeValidation;

impl LintRule for StorageWriteBeforeValidation {
    fn id(&self) -> &str {
        "VAL003"
    }
    fn name(&self) -> &str {
        "storage-write-before-validation"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = cfg_test_module_mask(ctx.content);

        fn expr_is_validation(expr: &Expr) -> bool {
            match strip_expr_wrappers(expr) {
                Expr::Macro(expr_macro) => macro_name(&expr_macro.mac).as_deref() == Some("ensure"),
                Expr::Call(expr_call) => expr_call_path(expr_call)
                    .map(|path| {
                        matches!(
                            path_last_ident(path).as_deref(),
                            Some("ensure_signed" | "ensure_root" | "ensure_none")
                        )
                    })
                    .unwrap_or(false),
                _ => false,
            }
        }

        fn expr_storage_write(expr: &Expr) -> Option<String> {
            struct WriteFinder {
                found: Option<(Span, String)>,
            }

            impl<'ast> Visit<'ast> for WriteFinder {
                fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                    if let Some(path) = expr_call_path(expr_call) {
                        if matches!(
                            path_last_ident(path).as_deref(),
                            Some("put" | "insert" | "mutate" | "set" | "append")
                        ) {
                            self.found =
                                Some((expr_call.span(), path.to_token_stream().to_string()));
                        }
                    }
                    visit::visit_expr_call(self, expr_call);
                }
            }

            let mut finder = WriteFinder { found: None };
            finder.visit_expr(expr);
            finder.found.map(|(_, path)| path)
        }

        struct ValidationOrderVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
        }

        impl ValidationOrderVisitor<'_> {
            fn inspect_block(&mut self, block: &syn::Block) {
                let mut first_write: Option<(usize, String)> = None;

                for stmt in &block.stmts {
                    if is_masked_span(self.mask, stmt.span()) {
                        continue;
                    }

                    let line = span_line(stmt.span());
                    match stmt {
                        syn::Stmt::Local(local) => {
                            if let Some(init) = &local.init {
                                if first_write.is_none() {
                                    first_write =
                                        expr_storage_write(&init.expr).map(|path| (line, path));
                                }
                                if let Some((write_line, write_pattern)) = &first_write {
                                    if expr_is_validation(&init.expr) && line > *write_line {
                                        self.diagnostics.push(Diagnostic {
                                            rule_id: self.rule_id.to_string(),
                                            rule_name: self.rule_name.to_string(),
                                            category: RuleCategory::Semantic,
                                            severity: self.severity,
                                            file: self.file.to_path_buf(),
                                            line: *write_line,
                                            column: None,
                                            end_line: Some(line),
                                            message: format!(
                                                "Storage write `{}` at line {} occurs before validation `ensure!` at line {}",
                                                write_pattern, write_line, line
                                            ),
                                            explanation: "Storage writes should happen after all validations. \
                                                If a later ensure! fails, the write has already persisted \
                                                (in hooks) or causes unnecessary rollback (in extrinsics)."
                                                .to_string(),
                                            suggestion: Some(
                                                "Move all ensure! checks before any storage writes".to_string(),
                                            ),
                                        });
                                        first_write = None;
                                    }
                                }
                            }
                        }
                        syn::Stmt::Expr(expr, _) => {
                            if first_write.is_none() {
                                first_write = expr_storage_write(expr).map(|path| (line, path));
                            }
                            if let Some((write_line, write_pattern)) = &first_write {
                                if expr_is_validation(expr) && line > *write_line {
                                    self.diagnostics.push(Diagnostic {
                                        rule_id: self.rule_id.to_string(),
                                        rule_name: self.rule_name.to_string(),
                                        category: RuleCategory::Semantic,
                                        severity: self.severity,
                                        file: self.file.to_path_buf(),
                                        line: *write_line,
                                        column: None,
                                        end_line: Some(line),
                                        message: format!(
                                            "Storage write `{}` at line {} occurs before validation `ensure!` at line {}",
                                            write_pattern, write_line, line
                                        ),
                                        explanation: "Storage writes should happen after all validations. \
                                            If a later ensure! fails, the write has already persisted \
                                            (in hooks) or causes unnecessary rollback (in extrinsics)."
                                            .to_string(),
                                        suggestion: Some(
                                            "Move all ensure! checks before any storage writes".to_string(),
                                        ),
                                    });
                                    first_write = None;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        impl<'ast> Visit<'ast> for ValidationOrderVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                self.inspect_block(&item_fn.block);
                visit::visit_item_fn(self, item_fn);
            }

            fn visit_impl_item(&mut self, item: &'ast ImplItem) {
                if let ImplItem::Fn(item_fn) = item {
                    self.inspect_block(&item_fn.block);
                }
                visit::visit_impl_item(self, item);
            }
        }

        let mut visitor = ValidationOrderVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
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
// SEC010: Missing #[transactional] / with_storage_layer in hooks
// ---------------------------------------------------------------------------
// reviewer (~3): "Should this be marked as transactional? It's called
// from on_poll." Unlike #[pallet::call] extrinsics which get automatic
// storage rollback, hook functions (on_poll, on_idle, on_initialize) do NOT.

pub struct MissingTransactionalInHook;

impl LintRule for MissingTransactionalInHook {
    fn id(&self) -> &str {
        "SEC010"
    }
    fn name(&self) -> &str {
        "missing-transactional-in-hook"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }
        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct HookStorageVisitor {
            write_count: usize,
            has_transactional: bool,
        }

        impl<'ast> Visit<'ast> for HookStorageVisitor {
            fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                if let Some(path) = expr_call_path(expr_call) {
                    if matches!(
                        path_last_ident(path).as_deref(),
                        Some("put" | "insert" | "mutate" | "remove" | "kill" | "set")
                    ) {
                        self.write_count += 1;
                    }
                    if path_has_exact_ident(path, "with_storage_layer") {
                        self.has_transactional = true;
                    }
                }
                visit::visit_expr_call(self, expr_call);
            }
        }

        struct HookVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
        }

        impl<'ast> Visit<'ast> for HookVisitor<'_> {
            fn visit_impl_item(&mut self, item: &'ast ImplItem) {
                let ImplItem::Fn(item_fn) = item else {
                    visit::visit_impl_item(self, item);
                    return;
                };
                let hook_name = item_fn.sig.ident.to_string();
                if !matches!(
                    hook_name.as_str(),
                    "on_poll" | "on_idle" | "on_initialize" | "on_finalize"
                ) || is_masked_span(self.mask, item_fn.span())
                {
                    visit::visit_impl_item(self, item);
                    return;
                }

                let mut visitor = HookStorageVisitor {
                    write_count: 0,
                    has_transactional: has_attr(&item_fn.attrs, &["transactional"]),
                };
                visitor.visit_block(&item_fn.block);
                if visitor.write_count >= 2 && !visitor.has_transactional {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::Semantic,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line: span_line(item_fn.span()),
						column: Some(span_column(item_fn.span())),
						end_line: None,
						message: format!(
							"Hook `{}` has {} storage writes without `with_storage_layer` — partial state risk",
							hook_name, visitor.write_count
						),
						explanation: "Unlike `#[pallet::call]` extrinsics, hook functions (`on_poll`, \
                            `on_idle`, `on_initialize`, `on_finalize`) do NOT get automatic storage rollback on failure. \
                            If one write succeeds and a later one fails, storage is left in an inconsistent state."
							.to_string(),
						suggestion: Some(format!(
							"Wrap the body of `{}` in `frame_support::storage::with_storage_layer(|| {{ ... }})`",
							hook_name
						)),
					});
                }
                visit::visit_impl_item(self, item);
            }
        }

        let mut visitor = HookVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
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
// SEC011: Storage iteration/drain inside dispatchables and hooks
// ---------------------------------------------------------------------------

pub struct StorageIterationInDispatchables;

impl LintRule for StorageIterationInDispatchables {
    fn id(&self) -> &str {
        "SEC011"
    }
    fn name(&self) -> &str {
        "storage-iteration-in-dispatchables"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }
        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct IterationFinder {
            found_span: Option<Span>,
        }

        impl<'ast> Visit<'ast> for IterationFinder {
            fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                if let Some(path) = expr_call_path(expr_call) {
                    if is_storage_iteration_call_path(path) {
                        self.found_span = Some(expr_call.span());
                    }
                }
                visit::visit_expr_call(self, expr_call);
            }
        }

        struct IterationVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
        }

        impl IterationVisitor<'_> {
            fn inspect_fn(&mut self, label: &str, span: Span, block: &syn::Block) {
                let mut finder = IterationFinder { found_span: None };
                finder.visit_block(block);
                if let Some(found_span) = finder.found_span {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::Semantic,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line: span_line(found_span),
						column: Some(span_column(found_span)),
						end_line: None,
						message: format!("{label} iterates storage with `iter()`/`drain()`"),
						explanation: "Iterating or draining storage inside dispatchables or hooks can make execution \
                            cost scale with chain state, which is difficult to benchmark safely and can create DoS risk."
							.to_string(),
						suggestion: Some(
							"Move iteration into a bounded workflow or redesign the storage access pattern".to_string(),
						),
					});
                }
                let _ = span;
            }
        }

        impl<'ast> Visit<'ast> for IterationVisitor<'_> {
            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                let is_call_impl = has_attr(&item_impl.attrs, &["pallet", "call"]);
                for item in &item_impl.items {
                    let ImplItem::Fn(item_fn) = item else {
                        continue;
                    };
                    if is_masked_span(self.mask, item_fn.span()) {
                        continue;
                    }
                    if is_call_impl && has_attr(&item_fn.attrs, &["pallet", "call_index"]) {
                        self.inspect_fn(
                            &format!("Dispatchable `{}`", item_fn.sig.ident),
                            item_fn.span(),
                            &item_fn.block,
                        );
                    }
                    let hook_name = item_fn.sig.ident.to_string();
                    if matches!(
                        hook_name.as_str(),
                        "on_poll" | "on_idle" | "on_initialize" | "on_finalize"
                    ) {
                        self.inspect_fn(
                            &format!("Hook `{}`", hook_name),
                            item_fn.span(),
                            &item_fn.block,
                        );
                    }
                }
                visit::visit_item_impl(self, item_impl);
            }
        }

        let mut visitor = IterationVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
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
// SEC012: Unbounded clear_prefix
// ---------------------------------------------------------------------------

pub struct UnboundedClearPrefix;

impl LintRule for UnboundedClearPrefix {
    fn id(&self) -> &str {
        "SEC012"
    }
    fn name(&self) -> &str {
        "unbounded-clear-prefix"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }
        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        fn is_unbounded_clear_prefix_limit(expr: &Expr) -> bool {
            match expr {
                Expr::Path(expr_path) => expr_path
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.ident == "None" || segment.ident == "MAX")
                    .unwrap_or(false),
                Expr::Call(expr_call) => {
                    expr_call_name(expr_call).as_deref() == Some("Some")
                        && expr_call
                            .args
                            .first()
                            .map(is_unbounded_clear_prefix_limit)
                            .unwrap_or(false)
                }
                Expr::Group(group) => is_unbounded_clear_prefix_limit(&group.expr),
                Expr::Paren(paren) => is_unbounded_clear_prefix_limit(&paren.expr),
                _ => false,
            }
        }

        struct ClearPrefixVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
        }

        impl<'ast> Visit<'ast> for ClearPrefixVisitor<'_> {
            fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                if is_masked_span(self.mask, expr_call.span()) {
                    visit::visit_expr_call(self, expr_call);
                    return;
                }
                let Some(path) = expr_call_path(expr_call) else {
                    visit::visit_expr_call(self, expr_call);
                    return;
                };
                if path_has_exact_ident(path, "clear_prefix")
                    && expr_call
                        .args
                        .iter()
                        .nth(1)
                        .map(is_unbounded_clear_prefix_limit)
                        .unwrap_or(false)
                {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::Semantic,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line: span_line(expr_call.span()),
						column: Some(span_column(expr_call.span())),
						end_line: None,
						message: "`clear_prefix` is called with an unbounded deletion limit".to_string(),
						explanation: "Calling `clear_prefix` with `None` or `u32::MAX` can delete an unbounded number \
                            of keys in one execution path, creating unpredictable weight and DoS risk."
							.to_string(),
						suggestion: Some(
							"Pass a strict bounded limit and handle continuation over multiple calls".to_string(),
						),
					});
                }
                visit::visit_expr_call(self, expr_call);
            }
        }

        let mut visitor = ClearPrefixVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
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
// SEC013: Unbounded storage collections without #[pallet::unbounded]
// ---------------------------------------------------------------------------

pub struct UnboundedStorageCollections;

impl LintRule for UnboundedStorageCollections {
    fn id(&self) -> &str {
        "SEC013"
    }
    fn name(&self) -> &str {
        "unbounded-storage-collections"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct StorageCollectionVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for StorageCollectionVisitor<'_> {
            fn visit_item_type(&mut self, item: &'ast ItemType) {
                if !has_attr(&item.attrs, &["pallet", "storage"])
                    || has_attr(&item.attrs, &["pallet", "unbounded"])
                    || is_masked_span(self.mask, item.span())
                {
                    return;
                }

                if type_contains_named(&item.ty, &["Vec", "BTreeMap", "BTreeSet"]) {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(item.span()),
                        column: Some(span_column(item.span())),
                        end_line: None,
                        message:
                            "Storage item uses `Vec`/`BTreeMap` without `#[pallet::unbounded]`"
                                .to_string(),
                        explanation: "FRAME requires `#[pallet::unbounded]` on storage items \
                            whose encoded size can grow without a static bound. Without it, the \
                            metadata and weight story are misleading."
                            .to_string(),
                        suggestion: Some(
                            "Add `#[pallet::unbounded]` or switch to a bounded storage type"
                                .to_string(),
                        ),
                    });
                }

                visit::visit_item_type(self, item);
            }
        }

        let mut visitor = StorageCollectionVisitor {
            mask: &test_mask,
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
// SEC014: Identity hasher on common key types
// ---------------------------------------------------------------------------

pub struct IdentityHasherOnCommonKeys;

impl LintRule for IdentityHasherOnCommonKeys {
    fn id(&self) -> &str {
        "SEC014"
    }
    fn name(&self) -> &str {
        "identity-hasher-on-common-keys"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        fn storage_map_type_args(item: &ItemType) -> Option<Vec<Type>> {
            let type_path = match &*item.ty {
                Type::Path(type_path) => type_path,
                _ => return None,
            };
            let segment = type_path.path.segments.last()?;
            match &segment.arguments {
                PathArguments::AngleBracketed(args) => Some(
                    args.args
                        .iter()
                        .filter_map(|arg| match arg {
                            GenericArgument::Type(ty) => Some(ty.clone()),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            }
        }

        struct IdentityHasherVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for IdentityHasherVisitor<'_> {
            fn visit_item_type(&mut self, item: &'ast ItemType) {
                if !has_attr(&item.attrs, &["pallet", "storage"])
                    || is_masked_span(self.mask, item.span())
                {
                    return;
                }

                let Some(type_args) = storage_map_type_args(item) else {
                    return;
                };

                let uses_identity = type_args.iter().any(|ty| type_is_named(ty, &["Identity"]));
                let uses_common_key = type_args.iter().any(|ty| {
                    type_contains_named(ty, &["AccountId", "Balance", "BlockNumber"])
                        || type_is_named(ty, &["u32", "u64"])
                });

                if uses_identity && uses_common_key {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::Semantic,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line: span_line(item.span()),
						column: Some(span_column(item.span())),
						end_line: None,
						message: "Storage map uses `Identity` hasher on a common key type".to_string(),
						explanation: "`Identity` hashing on predictable keys like `AccountId`, \
                            `u32`, `u64`, or balances can expose the trie to poor key dispersion \
                            and unsafe assumptions. Prefer a standard hasher such as `Blake2_128Concat`."
							.to_string(),
						suggestion: Some("Replace `Identity` with `Blake2_128Concat` unless the identity layout is strictly required".to_string()),
					});
                }

                visit::visit_item_type(self, item);
            }
        }

        let mut visitor = IdentityHasherVisitor {
            mask: &test_mask,
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
// SEC015: dispatch_bypass_filter in production
// ---------------------------------------------------------------------------

pub struct DispatchBypassFilterInProduction;

impl LintRule for DispatchBypassFilterInProduction {
    fn id(&self) -> &str {
        "SEC015"
    }
    fn name(&self) -> &str {
        "dispatch-bypass-filter-in-production"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;
        let mut visitor = ExprMethodVisitor {
            names: &["dispatch_bypass_filter"],
            matches: Vec::new(),
        };
        visitor.visit_file(ast);

        let diagnostics = visitor
			.matches
			.into_iter()
			.filter(|(_, span)| !is_masked_span(&test_mask, *span))
			.map(|(_, span)| Diagnostic {
				rule_id: self.id().to_string(),
				rule_name: self.name().to_string(),
				category: RuleCategory::Semantic,
				severity: config.rule_severity(self.id(), Severity::Warning),
				file: ctx.path.clone(),
				line: span_line(span),
				column: Some(span_column(span)),
				end_line: None,
				message: "`dispatch_bypass_filter` is used in production code".to_string(),
				explanation: "Bypassing dispatch filters sidesteps normal call filtering and \
                    can accidentally enable privileged or unsafe execution paths."
					.to_string(),
				suggestion: Some("Use normal `dispatch`/`dispatch_as` flow unless bypassing filters is explicitly justified and reviewed".to_string()),
			})
			.collect::<Vec<_>>();

        if diagnostics.is_empty() {
            None
        } else {
            Some(diagnostics)
        }
    }
}

// ---------------------------------------------------------------------------
// SEC016: on_runtime_upgrade writes without StorageVersion check
// ---------------------------------------------------------------------------

pub struct MissingStorageVersionCheckInRuntimeUpgrade;

impl LintRule for MissingStorageVersionCheckInRuntimeUpgrade {
    fn id(&self) -> &str {
        "SEC016"
    }
    fn name(&self) -> &str {
        "missing-storage-version-check-in-runtime-upgrade"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct UpgradeBodyVisitor {
            has_storage_version_check: bool,
            has_write: bool,
        }

        impl<'ast> Visit<'ast> for UpgradeBodyVisitor {
            fn visit_expr_call(&mut self, node: &'ast ExprCall) {
                if let Some(name) = expr_call_name(node) {
                    if [
                        "put",
                        "insert",
                        "mutate",
                        "remove",
                        "kill",
                        "set",
                        "append",
                        "take",
                        "clear_prefix",
                    ]
                    .contains(&name.as_str())
                    {
                        self.has_write = true;
                    }
                }
                visit::visit_expr_call(self, node);
            }

            fn visit_path(&mut self, path: &'ast syn::Path) {
                if [
                    "StorageVersion",
                    "current_storage_version",
                    "on_chain_storage_version",
                ]
                .iter()
                .any(|name| path_has_segment(path, name))
                {
                    self.has_storage_version_check = true;
                }
                visit::visit_path(self, path);
            }
        }

        struct UpgradeVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl UpgradeVisitor<'_> {
            fn inspect_fn(&mut self, item_fn: &ItemFn) {
                if item_fn.sig.ident != "on_runtime_upgrade"
                    || is_masked_span(self.mask, item_fn.span())
                {
                    return;
                }

                let mut body_visitor = UpgradeBodyVisitor {
                    has_storage_version_check: false,
                    has_write: false,
                };
                body_visitor.visit_block(&item_fn.block);

                if body_visitor.has_write && !body_visitor.has_storage_version_check {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::Semantic,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line: span_line(item_fn.sig.ident.span()),
						column: Some(span_column(item_fn.sig.ident.span())),
						end_line: None,
						message:
							"`on_runtime_upgrade` writes storage without checking `StorageVersion`"
								.to_string(),
						explanation: "Runtime upgrades should guard storage migrations with a \
                            `StorageVersion` check so repeated execution does not corrupt or \
                            re-apply state transitions."
							.to_string(),
						suggestion: Some(
							"Gate the writes with an `on_chain_storage_version()` / `StorageVersion::get()` check"
								.to_string(),
						),
					});
                }
            }
        }

        impl<'ast> Visit<'ast> for UpgradeVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                self.inspect_fn(item_fn);
                visit::visit_item_fn(self, item_fn);
            }

            fn visit_impl_item_fn(&mut self, item_fn: &'ast syn::ImplItemFn) {
                let wrapper = ItemFn {
                    attrs: item_fn.attrs.clone(),
                    vis: item_fn.vis.clone(),
                    sig: item_fn.sig.clone(),
                    block: Box::new(item_fn.block.clone()),
                };
                self.inspect_fn(&wrapper);
                visit::visit_impl_item_fn(self, item_fn);
            }
        }

        let mut visitor = UpgradeVisitor {
            mask: &test_mask,
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
// SEC017: Vec<T> inside pallet events
// ---------------------------------------------------------------------------

pub struct VecInEvents;

impl LintRule for VecInEvents {
    fn id(&self) -> &str {
        "SEC017"
    }
    fn name(&self) -> &str {
        "vec-in-events"
    }
    fn family(&self) -> &str {
        "security"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct EventVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for EventVisitor<'_> {
            fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
                if !has_attr(&item.attrs, &["pallet", "event"])
                    || is_masked_span(self.mask, item.span())
                {
                    return;
                }

                let has_vec_field = item.variants.iter().any(|variant| match &variant.fields {
                    syn::Fields::Named(fields) => fields
                        .named
                        .iter()
                        .any(|field| type_contains_named(&field.ty, &["Vec"])),
                    syn::Fields::Unnamed(fields) => fields
                        .unnamed
                        .iter()
                        .any(|field| type_contains_named(&field.ty, &["Vec"])),
                    syn::Fields::Unit => false,
                });

                if has_vec_field {
                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(item.span()),
                        column: Some(span_column(item.span())),
                        end_line: None,
                        message: "`#[pallet::event]` contains a `Vec<T>` field".to_string(),
                        explanation:
                            "Event payloads should stay bounded and predictable. `Vec<T>` \
                            in events allows arbitrarily large event encoding and weaker weight \
                            assumptions."
                                .to_string(),
                        suggestion: Some(
                            "Use a bounded vector or emit repeated smaller events".to_string(),
                        ),
                    });
                }

                visit::visit_item_enum(self, item);
            }
        }

        let mut visitor = EventVisitor {
            mask: &test_mask,
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
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
