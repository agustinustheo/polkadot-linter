use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use proc_macro2::Span;
use quote::ToTokens;
use syn::{
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, Expr, ExprBinary, ExprCall, ExprForLoop, ExprIf, ExprMethodCall, File as SynFile,
    GenericArgument, ImplItem, Item, ItemEnum, ItemFn, ItemImpl, ItemType, Lit, Local, Macro, Pat,
    PathArguments, Stmt, Token, Type, TypePath, UseTree, Visibility,
};

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    frame_model::{collect_dispatchables, DispatchableOrigin},
    project_model::SourceTargetKind,
    rules::LintRule,
};

fn is_pure_test_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    path_str.contains("/tests/")
        || path_str.contains("/test/")
        || path_str.contains("/test-runtime/")
        || path_str.contains("/test-staking-e2e/")
        || path_str.contains("/test-utils/")
        || path_str.contains("/ui-tests/")
        || path_str.contains("/mocks/")
        || path_str.contains("/mock/")
        || path_str.contains("/mock-")
        || path_str.contains("/remote-tests/")
        || path_str.contains("/fuzzer/")
        || path_str.contains("/fixtures/")
        || path_str.contains("/conformance_tests/")
        || path_str.contains("/subsystem-bench/")
        || path_str.contains("-benchmarking/")
        || path_str.contains("/test-common/")
        || path_str.contains("/test_utils/")
        || path_str.contains("/testing/")
        || path_str.contains("/bench/")
        || path_str.contains("/frame/root-offences/")
        || path_str.contains("/parachains/pallets/ping/")
        || path_str.contains("integration_tests")
        || path_str.contains("integration-tests")
        || path_str.contains("send-tx/")
        || path_str.ends_with("_test.rs")
        || path_str.ends_with("_tests.rs")
        || path_str.ends_with("tests.rs")
        || file_name == "build.rs"
        || file_name.starts_with("mock_")
        || file_name == "mock.rs"
        || file_name == "remote_mining.rs"
        || file_name == "test_utils.rs"
        || file_name == "testing.rs"
        || file_name == "testing_utils.rs"
}

fn should_skip_production_rule(ctx: &FileContext) -> bool {
    !ctx.is_rust
        || ctx.is_benchmark_file
        || ctx.is_test_file
        || ctx
            .source_target_kinds
            .iter()
            .any(|kind| matches!(kind, SourceTargetKind::ProcMacro))
        || is_pure_test_path(&ctx.path)
        || has_crate_level_non_production_cfg(ctx.content)
        || is_cfg_gated_module_file(&ctx.path)
}

fn has_crate_level_non_production_cfg(content: &str) -> bool {
    content.lines().take(80).any(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with("#![cfg(") {
            return false;
        }
        let item_attr = trimmed.replacen("#![", "#[", 1);
        is_masked_cfg_attribute(&item_attr)
    })
}

fn is_cfg_gated_module_file(path: &Path) -> bool {
    let Some(module_name) = module_name_for_file(path) else {
        return false;
    };

    parent_module_candidates(path).iter().any(|candidate| {
        fs::read_to_string(candidate)
            .ok()
            .is_some_and(|content| module_decl_has_masked_cfg(&content, &module_name))
    })
}

fn module_name_for_file(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    if file_name == "mod.rs" {
        return path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned);
    }

    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToOwned::to_owned)
}

fn parent_module_candidates(path: &Path) -> Vec<std::path::PathBuf> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if file_name == "mod.rs" {
        let Some(grandparent) = parent.parent() else {
            return Vec::new();
        };
        return vec![
            grandparent.join("lib.rs"),
            grandparent.join("main.rs"),
            grandparent.join("mod.rs"),
            grandparent.join(format!(
                "{}.rs",
                parent
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
            )),
        ];
    }

    let mut candidates = vec![
        parent.join("mod.rs"),
        parent.with_extension("rs"),
        parent.join("lib.rs"),
        parent.join("main.rs"),
    ];
    candidates.dedup();
    candidates
}

fn module_decl_has_masked_cfg(content: &str, module_name: &str) -> bool {
    let mut pending_masked_cfg = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if is_masked_cfg_attribute(trimmed) {
            pending_masked_cfg = true;
            continue;
        }
        if trimmed.starts_with("#[") {
            continue;
        }
        if module_declaration_name(trimmed).as_deref() == Some(module_name) {
            return pending_masked_cfg;
        }
        pending_masked_cfg = false;
    }

    false
}

fn module_declaration_name(trimmed: &str) -> Option<String> {
    let after_mod = if let Some(rest) = trimmed.strip_prefix("mod ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("pub mod ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("pub(crate) mod ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("pub(super) mod ") {
        rest
    } else if trimmed.starts_with("pub(in ") {
        trimmed.split_once(" mod ")?.1
    } else {
        return None;
    };

    let name = after_mod
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

fn is_documented_benchmark_test_builder(path: &Path, content: &str) -> bool {
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if file_name != "builder.rs" {
        return false;
    }

    let path_str = path.to_string_lossy();
    let normalized = format!("/{path_str}");
    if !(normalized.contains("/runtime/")
        || normalized.contains("/runtimes/")
        || normalized.contains("/pallets/"))
    {
        return false;
    }

    let lower = content.to_ascii_lowercase();
    lower.contains("benchmark scenario builder") || lower.contains("benchmarks and tests")
}

fn is_runtime_or_pallet_security_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let normalized = format!("/{path_str}");
    if normalized.contains("/substrate/frame/support/")
        || normalized.contains("/substrate/frame/examples/")
        || normalized.contains("/proc-macro/")
        || normalized.contains("/rpc/")
        || normalized.contains("/uapi/")
        || normalized.contains("/rc-client/")
        || normalized.contains("/ah-client/")
        || normalized.contains("/polkadot/node/")
        || normalized.contains("/dev-node/node/")
        || normalized.contains("/reward-curve/")
        || normalized.contains("/substrate/frame/asset-conversion/ops/")
    {
        return false;
    }

    normalized.contains("/pallets/")
        || normalized.contains("/runtime/")
        || normalized.contains("/runtimes/")
        || normalized.contains("/substrate/frame/")
        || normalized.contains("/bridges/modules/")
}

fn is_frame_support_migration_infrastructure(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let normalized = format!("/{path_str}");
    normalized.contains("/substrate/frame/support/src/migrations.rs")
        || normalized.contains("/substrate/frame/support/src/storage/migration.rs")
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
        || trimmed.starts_with('{')
        || trimmed.starts_with("impl ")
        || trimmed.starts_with("impl<")
        || trimmed.starts_with("pub impl ")
        || trimmed.starts_with("pub impl<")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("fn ")
        || trimmed.starts_with("pub async fn ")
        || trimmed.starts_with("async fn ")
        || trimmed.starts_with("pub(crate) fn ")
        || trimmed.starts_with("pub(crate) async fn ")
        || trimmed.starts_with("pub struct ")
        || trimmed.starts_with("pub(crate) struct ")
        || trimmed.starts_with("pub enum ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("pub trait ")
        || trimmed.starts_with("trait ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("if let ")
        || trimmed.starts_with("match ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("while let ")
        || trimmed.starts_with("loop ")
        || trimmed == "loop"
}

fn is_masked_cfg_attribute(trimmed: &str) -> bool {
    if trimmed == "#[test]" || trimmed.ends_with("::test]") {
        return true;
    }

    if !(trimmed.starts_with("#[cfg(") || trimmed.starts_with("#![cfg(")) {
        return false;
    }

    let compact = trimmed.split_whitespace().collect::<String>();
    compact.contains("cfg(test)")
        || compact.contains("(test,")
        || compact.contains(",test)")
        || compact.contains(",test,")
        || trimmed.contains("feature = \"runtime-benchmarks\"")
        || trimmed.contains("feature = \"try-runtime\"")
        || trimmed.contains("feature = \"test-helpers\"")
        || trimmed.contains("feature = \"remote-test\"")
        || trimmed.contains("feature = \"integrity-test\"")
}

fn item_mask_by_attr<F>(content: &str, is_masking_attr: F) -> Vec<bool>
where
    F: Fn(&str) -> bool,
{
    let lines: Vec<&str> = content.lines().collect();
    let mut mask = vec![false; lines.len()];
    let mut masked_depth: Option<i32> = None;
    let mut brace_depth: i32 = 0;
    let mut next_item_is_masked = false;
    // Tracks whether the masked block body started. Multi-line item
    // declarations can sit at the parent brace depth until a later `{`.
    let mut mask_block_entered = false;
    let mut masked_statement_depth: Option<i32> = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let statement_was_masked = masked_statement_depth.is_some();
        if statement_was_masked {
            mask[i] = true;
        }

        if is_masking_attr(trimmed) && masked_depth.is_none() {
            mask[i] = true;
            next_item_is_masked = true;
        }

        // Skip other attributes and comments between the cfg and the target item.
        if next_item_is_masked
            && (trimmed.starts_with("#[") || trimmed.starts_with("//"))
            && !is_masking_attr(trimmed)
        {
            // keep waiting
        }

        let open = line.chars().filter(|&c| c == '{').count() as i32;
        let close = line.chars().filter(|&c| c == '}').count() as i32;
        let starts_masked_block =
            next_item_is_masked && masked_depth.is_none() && is_block_start(trimmed);

        if starts_masked_block {
            masked_depth = Some(brace_depth);
            mask_block_entered = false;
            next_item_is_masked = false;
        } else if next_item_is_masked
            && !trimmed.is_empty()
            && !trimmed.starts_with("#[")
            && !trimmed.starts_with("//")
        {
            mask[i] = true;
            if !trimmed.ends_with(';') {
                masked_statement_depth = Some(brace_depth);
            }
            next_item_is_masked = false;
        }

        brace_depth += open;
        brace_depth -= close;

        if let Some(statement_depth) = masked_statement_depth {
            mask[i] = true;
            if trimmed.ends_with(';') && brace_depth <= statement_depth {
                masked_statement_depth = None;
            }
        }

        if let Some(td) = masked_depth {
            mask[i] = true;
            if brace_depth > td || (starts_masked_block && open > 0) {
                mask_block_entered = true;
            }
            if mask_block_entered && brace_depth <= td {
                masked_depth = None;
                mask_block_entered = false;
            }
        }
    }

    mask
}

fn cfg_test_module_mask(content: &str) -> Vec<bool> {
    merge_masks(
        &merge_masks(
            &item_mask_by_attr(content, is_masked_cfg_attribute),
            &non_production_cfg_expr_block_mask(content),
        ),
        &documented_test_only_item_mask(content),
    )
}

fn non_production_cfg_expr_block_mask(content: &str) -> Vec<bool> {
    let lines: Vec<&str> = content.lines().collect();
    let mut mask = vec![false; lines.len()];
    let mut masked_depth: Option<i32> = None;
    let mut brace_depth: i32 = 0;
    let mut mask_block_entered = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let starts_masked_block = masked_depth.is_none()
            && (trimmed.starts_with("if cfg!(debug_assertions)")
                || trimmed.starts_with("if cfg!(feature = \"runtime-benchmarks\")")
                || trimmed.starts_with("if cfg!(feature = \"try-runtime\")")
                || trimmed.starts_with("if cfg!(feature = \"test-helpers\")")
                || trimmed.starts_with("if cfg!(feature = \"remote-test\")")
                || trimmed.starts_with("if cfg!(feature = \"integrity-test\")"));

        let open = line.chars().filter(|&c| c == '{').count() as i32;
        let close = line.chars().filter(|&c| c == '}').count() as i32;

        if starts_masked_block {
            masked_depth = Some(brace_depth);
            mask_block_entered = false;
        }

        brace_depth += open;
        brace_depth -= close;

        if let Some(td) = masked_depth {
            mask[i] = true;
            if brace_depth > td || (starts_masked_block && open > 0) {
                mask_block_entered = true;
            }
            if mask_block_entered && brace_depth <= td {
                masked_depth = None;
                mask_block_entered = false;
            }
        }
    }

    mask
}

fn documented_test_only_item_mask(content: &str) -> Vec<bool> {
    let lines: Vec<&str> = content.lines().collect();
    let mut mask = vec![false; lines.len()];
    let mut masked_depth: Option<i32> = None;
    let mut brace_depth: i32 = 0;
    let mut pending_test_only_doc = false;
    let mut doc_block_active = false;
    let mut mask_block_entered = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if masked_depth.is_none() {
            if let Some(comment) = trimmed.strip_prefix("///") {
                doc_block_active = true;
                pending_test_only_doc |= text_marks_test_only_item(comment);
            } else if doc_block_active && (trimmed.is_empty() || trimmed.starts_with("#[")) {
                // Keep the doc marker across attributes and the blank line that sometimes
                // separates docs from cfg attributes.
            } else if doc_block_active && !trimmed.is_empty() {
                doc_block_active = false;
            }
        }

        let open = line.chars().filter(|&c| c == '{').count() as i32;
        let close = line.chars().filter(|&c| c == '}').count() as i32;
        let starts_masked_block =
            pending_test_only_doc && masked_depth.is_none() && is_block_start(trimmed);

        if starts_masked_block {
            masked_depth = Some(brace_depth);
            mask_block_entered = false;
            pending_test_only_doc = false;
        } else if pending_test_only_doc
            && !trimmed.is_empty()
            && !trimmed.starts_with("#[")
            && !trimmed.starts_with("///")
        {
            pending_test_only_doc = false;
        }

        brace_depth += open;
        brace_depth -= close;

        if let Some(td) = masked_depth {
            mask[i] = true;
            if brace_depth > td || (starts_masked_block && open > 0) {
                mask_block_entered = true;
            }
            if mask_block_entered && brace_depth <= td {
                masked_depth = None;
                mask_block_entered = false;
            }
        }
    }

    mask
}

fn text_marks_test_only_item(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let mentions_test = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|word| matches!(word, "test" | "tests" | "testing"));
    let excludes_production = lower.contains("do not use in production")
        || lower.contains("not be used in production")
        || lower.contains("never be used in production")
        || lower.contains("must never be used in production");
    mentions_test
        && (excludes_production
            || lower.contains("only be used")
            || lower.contains("only used")
            || lower.contains("used only")
            || lower.contains("test only")
            || lower.contains("testing only"))
}

fn genesis_build_mask(content: &str) -> Vec<bool> {
    item_mask_by_attr(content, |trimmed| trimmed == "#[pallet::genesis_build]")
}

fn genesis_config_impl_mask(content: &str) -> Vec<bool> {
    let lines: Vec<&str> = content.lines().collect();
    let mut mask = vec![false; lines.len()];
    let mut masked_depth: Option<i32> = None;
    let mut brace_depth: i32 = 0;
    let mut mask_block_entered = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let starts_masked_block = masked_depth.is_none()
            && (trimmed.starts_with("impl ") || trimmed.starts_with("impl<"))
            && trimmed.contains("GenesisConfig")
            && !trimmed.contains(" for ");

        let open = line.chars().filter(|&c| c == '{').count() as i32;
        let close = line.chars().filter(|&c| c == '}').count() as i32;

        if starts_masked_block {
            masked_depth = Some(brace_depth);
            mask_block_entered = false;
        }

        brace_depth += open;
        brace_depth -= close;

        if let Some(td) = masked_depth {
            mask[i] = true;
            if brace_depth > td || (starts_masked_block && open > 0) {
                mask_block_entered = true;
            }
            if mask_block_entered && brace_depth <= td {
                masked_depth = None;
                mask_block_entered = false;
            }
        }
    }

    mask
}

fn const_unit_initializer_mask(content: &str) -> Vec<bool> {
    let lines: Vec<&str> = content.lines().collect();
    let mut mask = vec![false; lines.len()];
    let mut masked_depth: Option<i32> = None;
    let mut brace_depth: i32 = 0;
    let mut mask_block_entered = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let starts_masked_block =
            masked_depth.is_none() && trimmed.starts_with("const ") && trimmed.contains(": () = {");

        let open = line.chars().filter(|&c| c == '{').count() as i32;
        let close = line.chars().filter(|&c| c == '}').count() as i32;

        if starts_masked_block {
            masked_depth = Some(brace_depth);
            mask_block_entered = false;
        }

        brace_depth += open;
        brace_depth -= close;

        if let Some(td) = masked_depth {
            mask[i] = true;
            if brace_depth > td || (starts_masked_block && open > 0) {
                mask_block_entered = true;
            }
            if mask_block_entered && brace_depth <= td && trimmed.ends_with(';') {
                masked_depth = None;
                mask_block_entered = false;
            }
        }
    }

    mask
}

fn integrity_test_mask(content: &str) -> Vec<bool> {
    item_mask_by_attr(content, |trimmed| {
        trimmed.starts_with("fn integrity_test")
            || trimmed.starts_with("pub fn integrity_test")
            || trimmed.starts_with("pub(crate) fn integrity_test")
            || trimmed.starts_with("pub(super) fn integrity_test")
            || trimmed.starts_with("unsafe fn integrity_test")
            || trimmed.starts_with("pub unsafe fn integrity_test")
    })
}

fn merge_masks(left: &[bool], right: &[bool]) -> Vec<bool> {
    let len = left.len().max(right.len());
    (0..len)
        .map(|idx| {
            left.get(idx).copied().unwrap_or(false) || right.get(idx).copied().unwrap_or(false)
        })
        .collect()
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

fn type_last_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(TypePath { path, .. }) => path_last_ident(path),
        Type::Reference(reference) => type_last_ident(&reference.elem),
        Type::Paren(paren) => type_last_ident(&paren.elem),
        Type::Group(group) => type_last_ident(&group.elem),
        _ => None,
    }
}

fn macro_name(mac: &Macro) -> Option<String> {
    path_last_ident(&mac.path)
}

fn expr_string_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(expr_lit) => match &expr_lit.lit {
            Lit::Str(lit) => Some(lit.value()),
            _ => None,
        },
        _ => None,
    }
}

fn expect_message_marks_proven_invariant(expr_method_call: &ExprMethodCall) -> bool {
    expr_method_call
        .args
        .first()
        .is_some_and(|arg| match expr_string_literal(arg) {
            Some(message) => message.to_ascii_lowercase().contains("qed"),
            None => arg
                .to_token_stream()
                .to_string()
                .eq_ignore_ascii_case("qed"),
        })
}

fn macro_message_marks_proven_invariant(mac: &Macro) -> bool {
    mac.tokens.to_string().to_ascii_lowercase().contains("qed")
}

fn unwrap_receiver_is_nonzero_literal_new(expr_method_call: &ExprMethodCall) -> bool {
    if expr_method_call.method != "unwrap" {
        return false;
    }

    let Expr::Call(expr_call) = strip_expr_wrappers(&expr_method_call.receiver) else {
        return false;
    };
    let Some(path) = expr_call_path(expr_call) else {
        return false;
    };
    if !path_has_exact_ident(path, "new") || !path_has_segment(path, "NonZero") {
        return false;
    }

    let Some(Expr::Lit(expr_lit)) = expr_call.args.first().map(strip_expr_wrappers) else {
        return false;
    };
    let Lit::Int(lit_int) = &expr_lit.lit else {
        return false;
    };

    integer_literal_is_nonzero(lit_int)
}

fn expr_usize_literal_value(expr: &Expr) -> Option<usize> {
    let Expr::Lit(expr_lit) = strip_expr_wrappers(expr) else {
        return None;
    };
    let Lit::Int(lit_int) = &expr_lit.lit else {
        return None;
    };
    lit_int.base10_parse::<usize>().ok()
}

fn additive_range_delta_from_start(expr: &Expr, start_key: &str) -> Option<usize> {
    let Expr::Binary(expr_binary) = strip_expr_wrappers(expr) else {
        return None;
    };
    if !matches!(expr_binary.op, syn::BinOp::Add(_)) {
        return None;
    }

    let left_key = compact_tokens(&expr_binary.left);
    let right_key = compact_tokens(&expr_binary.right);
    if left_key == start_key {
        expr_usize_literal_value(&expr_binary.right)
    } else if right_key == start_key {
        expr_usize_literal_value(&expr_binary.left)
    } else {
        None
    }
}

fn half_open_range_has_static_len(expr_range: &syn::ExprRange) -> bool {
    if !matches!(expr_range.limits, syn::RangeLimits::HalfOpen(_)) {
        return false;
    }

    match (&expr_range.start, &expr_range.end) {
        (None, Some(end)) => expr_usize_literal_value(end).is_some(),
        (Some(start), Some(end)) => {
            if let (Some(start_value), Some(end_value)) = (
                expr_usize_literal_value(start),
                expr_usize_literal_value(end),
            ) {
                return end_value >= start_value;
            }

            let start_key = compact_tokens(start);
            additive_range_delta_from_start(end, &start_key).is_some()
        }
        _ => false,
    }
}

fn panic_receiver_is_fixed_range_try_into(expr_method_call: &ExprMethodCall) -> bool {
    if !matches!(
        expr_method_call.method.to_string().as_str(),
        "unwrap" | "expect"
    ) {
        return false;
    }

    let Expr::MethodCall(try_into_call) = strip_expr_wrappers(&expr_method_call.receiver) else {
        return false;
    };
    if try_into_call.method != "try_into" || !try_into_call.args.is_empty() {
        return false;
    }

    let Expr::Index(expr_index) = strip_expr_wrappers(&try_into_call.receiver) else {
        return false;
    };
    let Expr::Range(expr_range) = strip_expr_wrappers(&expr_index.index) else {
        return false;
    };
    half_open_range_has_static_len(expr_range)
}

fn h160_param_names(signature: &syn::Signature) -> HashSet<String> {
    signature
        .inputs
        .iter()
        .filter_map(|arg| {
            let syn::FnArg::Typed(pat_type) = arg else {
                return None;
            };
            if type_last_ident(&pat_type.ty).as_deref() != Some("H160") {
                return None;
            }
            let Pat::Ident(pat_ident) = strip_pat_wrappers(&pat_type.pat) else {
                return None;
            };
            Some(pat_ident.ident.to_string())
        })
        .collect()
}

fn expr_path_last_ident(expr: &Expr) -> Option<String> {
    let Expr::Path(expr_path) = strip_expr_wrappers(expr) else {
        return None;
    };
    path_last_ident(&expr_path.path)
}

fn panic_receiver_is_h160_suffix_try_into(
    expr_method_call: &ExprMethodCall,
    h160_names: &HashSet<String>,
) -> bool {
    if !matches!(
        expr_method_call.method.to_string().as_str(),
        "unwrap" | "expect"
    ) {
        return false;
    }

    let Expr::MethodCall(try_into_call) = strip_expr_wrappers(&expr_method_call.receiver) else {
        return false;
    };
    if try_into_call.method != "try_into" || !try_into_call.args.is_empty() {
        return false;
    }

    let Expr::Index(expr_index) = strip_expr_wrappers(&try_into_call.receiver) else {
        return false;
    };
    let Expr::Range(expr_range) = strip_expr_wrappers(&expr_index.index) else {
        return false;
    };
    if !matches!(expr_range.limits, syn::RangeLimits::HalfOpen(_))
        || expr_range.end.is_some()
        || expr_range
            .start
            .as_ref()
            .and_then(|start| expr_usize_literal_value(start))
            != Some(12)
    {
        return false;
    }

    let Expr::MethodCall(as_ref_call) = strip_expr_wrappers(&expr_index.expr) else {
        return false;
    };
    as_ref_call.method == "as_ref"
        && as_ref_call.args.is_empty()
        && expr_path_last_ident(&as_ref_call.receiver)
            .as_ref()
            .is_some_and(|ident| h160_names.contains(ident))
}

fn expr_integer_literal_value(expr: &Expr) -> Option<u128> {
    let Expr::Lit(expr_lit) = strip_expr_wrappers(expr) else {
        return None;
    };
    let Lit::Int(lit_int) = &expr_lit.lit else {
        return None;
    };
    lit_int.base10_parse::<u128>().ok()
}

fn integer_literal_is_nonzero(lit_int: &syn::LitInt) -> bool {
    let mut digits = lit_int.to_string();
    if let Some(suffix) = lit_int.suffix().strip_prefix('u') {
        digits.truncate(digits.len().saturating_sub(suffix.len() + 1));
    } else if let Some(suffix) = lit_int.suffix().strip_prefix('i') {
        digits.truncate(digits.len().saturating_sub(suffix.len() + 1));
    }

    let normalized = digits.replace('_', "");
    let without_prefix = normalized
        .strip_prefix("0x")
        .or_else(|| normalized.strip_prefix("0o"))
        .or_else(|| normalized.strip_prefix("0b"))
        .unwrap_or(&normalized);
    without_prefix.chars().any(|ch| ch != '0')
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

fn expr_contains_named(expr: &Expr, names: &[&str]) -> bool {
    struct ExprNameVisitor<'a> {
        names: &'a [&'a str],
        found: bool,
    }

    impl<'ast> Visit<'ast> for ExprNameVisitor<'_> {
        fn visit_expr_path(&mut self, expr_path: &'ast syn::ExprPath) {
            if expr_path
                .path
                .segments
                .last()
                .map(|segment| self.names.iter().any(|name| segment.ident == *name))
                .unwrap_or(false)
            {
                self.found = true;
            }
            visit::visit_expr_path(self, expr_path);
        }
    }

    let mut visitor = ExprNameVisitor {
        names,
        found: false,
    };
    visitor.visit_expr(expr);
    visitor.found
}

fn type_contains_unbounded_storage_collection(ty: &Type) -> bool {
    const UNBOUNDED_COLLECTIONS: &[&str] = &["Vec", "BTreeMap", "BTreeSet"];
    const BOUNDED_COLLECTIONS: &[&str] = &[
        "BoundedBTreeMap",
        "BoundedBTreeSet",
        "BoundedVec",
        "WeakBoundedVec",
    ];

    struct TypeNameVisitor {
        found: bool,
    }

    impl<'ast> Visit<'ast> for TypeNameVisitor {
        fn visit_type_path(&mut self, type_path: &'ast TypePath) {
            let Some(last_segment) = type_path.path.segments.last() else {
                return;
            };
            let last_ident = last_segment.ident.to_string();

            if BOUNDED_COLLECTIONS.contains(&last_ident.as_str()) {
                return;
            }
            if UNBOUNDED_COLLECTIONS.contains(&last_ident.as_str()) {
                self.found = true;
                return;
            }

            visit::visit_type_path(self, type_path);
        }
    }

    let mut visitor = TypeNameVisitor { found: false };
    visitor.visit_type(ty);
    visitor.found
}

fn has_pallet_dev_mode(ast: &SynFile) -> bool {
    ast.items.iter().any(|item| {
        let attrs = match item {
            Item::Mod(item_mod) => &item_mod.attrs,
            _ => return false,
        };

        attrs.iter().any(|attr| {
            path_has_exact_ident(attr.path(), "pallet") && compact_tokens(attr).contains("dev_mode")
        })
    })
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

fn strip_pat_wrappers(mut pat: &Pat) -> &Pat {
    loop {
        pat = match pat {
            Pat::Paren(paren) => &paren.pat,
            Pat::Reference(reference) => &reference.pat,
            _ => return pat,
        };
    }
}

fn expr_is_top_level_try(expr: &Expr) -> bool {
    match expr {
        Expr::Try(_) => true,
        Expr::Group(group) => expr_is_top_level_try(&group.expr),
        Expr::Paren(paren) => expr_is_top_level_try(&paren.expr),
        _ => false,
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
    matches!(path_last_ident(path).as_deref(), Some("iter" | "drain"))
        && path_owner_name(path).is_some()
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
                    message: "Weight function called without parameter then multiplied".to_string(),
                    explanation: "Use parameterised benchmarks: `T::WeightInfo::foo(n)` captures \
                        constant overhead that does not scale with the input, unlike \
                        `T::WeightInfo::foo().saturating_mul(n)` which misses it."
                        .to_string(),
                    suggestion: Some(
                        "Pass the scaling parameter directly: `T::WeightInfo::foo(n)`".to_string(),
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
// SEM013: Custom invalidity enums should use #[repr(u8)]
// ---------------------------------------------------------------------------

pub struct CustomInvalidityReprU8;

impl LintRule for CustomInvalidityReprU8 {
    fn id(&self) -> &str {
        "SEM013"
    }
    fn name(&self) -> &str {
        "custom-invalidity-repr-u8"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        fn is_custom_invalidity_enum(name: &str) -> bool {
            name == "CustomInvalidity" || name == "CustomValidity" || name.contains("Invalidity")
        }

        fn has_repr_u8(attrs: &[Attribute]) -> bool {
            attrs.iter().any(|attr| {
                attr_path_matches(attr, &["repr"])
                    && attr
                        .to_token_stream()
                        .to_string()
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>()
                        .contains("repr(u8)")
            })
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct CustomInvalidityVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for CustomInvalidityVisitor<'_> {
            fn visit_item_enum(&mut self, item_enum: &'ast ItemEnum) {
                if is_masked_span(self.mask, item_enum.span()) {
                    return;
                }

                let enum_name = item_enum.ident.to_string();
                if !is_custom_invalidity_enum(&enum_name) || has_repr_u8(&item_enum.attrs) {
                    return;
                }

                self.diagnostics.push(Diagnostic {
                    rule_id: self.rule_id.to_string(),
                    rule_name: self.rule_name.to_string(),
                    category: RuleCategory::Semantic,
                    severity: self.severity,
                    file: self.file.to_path_buf(),
                    line: span_line(item_enum.span()),
                    column: Some(span_column(item_enum.span())),
                    end_line: None,
                    message: format!(
                        "`{}` should declare `#[repr(u8)]` before converting to `InvalidTransaction::Custom`",
                        enum_name
                    ),
                    explanation: "Custom invalidity enums are often converted into \
                        `InvalidTransaction::Custom(e as u8)`. Without `#[repr(u8)]`, the \
                        discriminant layout is implicit, which makes downstream debugging and \
                        compatibility reasoning harder."
                        .to_string(),
                    suggestion: Some("Add `#[repr(u8)]` above the enum declaration".to_string()),
                });
            }
        }

        let mut visitor = CustomInvalidityVisitor {
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
// SEM014: SubmitTransaction logs should use target: LOG_TARGET
// ---------------------------------------------------------------------------

pub struct SubmitTransactionLogTarget;

impl LintRule for SubmitTransactionLogTarget {
    fn id(&self) -> &str {
        "SEM014"
    }
    fn name(&self) -> &str {
        "submit-transaction-log-target"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        fn is_log_macro_line(line: &str) -> bool {
            [
                "log::trace!(",
                "log::debug!(",
                "log::info!(",
                "log::warn!(",
                "log::error!(",
            ]
            .iter()
            .any(|needle| line.contains(needle))
        }

        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();
        let test_mask = cfg_test_module_mask(ctx.content);

        for (i, line) in lines.iter().enumerate() {
            if test_mask[i]
                || !line.contains("SubmitTransaction::<")
                || !line.contains("submit_transaction")
            {
                continue;
            }

            let search_end = (i + 12).min(lines.len());
            for j in i..search_end {
                if test_mask[j] || !is_log_macro_line(lines[j]) {
                    continue;
                }

                let log_end = (j + 5).min(lines.len()).saturating_sub(1);
                let log_window = (j..=log_end)
                    .filter(|&k| !test_mask[k])
                    .map(|k| lines[k])
                    .collect::<Vec<_>>()
                    .join("\n");

                if !log_window.contains("target:") {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        rule_name: self.name().to_string(),
                        category: RuleCategory::Semantic,
                        severity: config.rule_severity(self.id(), Severity::Advisory),
                        file: ctx.path.clone(),
                        line: j + 1,
                        column: None,
                        end_line: Some(log_end + 1),
                        message: "`SubmitTransaction` logging should include `target: LOG_TARGET`"
                            .to_string(),
                        explanation: "Offchain transaction submission failures are much easier \
                            to trace when pallet logs emit through a stable `LOG_TARGET`. Reviewer \
                            feedback repeatedly called this out for OCW cleanup and submission paths."
                            .to_string(),
                        suggestion: Some(
                            "Add `target: LOG_TARGET,` to the nearby `log::...!` macro".to_string(),
                        ),
                    });
                }
                break;
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
// SEM015: #[pallet::authorize] should have #[pallet::weight_of_authorize]
// ---------------------------------------------------------------------------

pub struct MissingWeightOfAuthorize;

impl LintRule for MissingWeightOfAuthorize {
    fn id(&self) -> &str {
        "SEM015"
    }
    fn name(&self) -> &str {
        "missing-weight-of-authorize"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        fn authorize_attr(attrs: &[Attribute]) -> Option<&Attribute> {
            attrs
                .iter()
                .find(|attr| attr_path_matches(attr, &["pallet", "authorize"]))
        }

        fn has_weight_of_authorize(attrs: &[Attribute]) -> bool {
            attrs
                .iter()
                .any(|attr| attr_path_matches(attr, &["pallet", "weight_of_authorize"]))
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct AuthorizeWeightVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl AuthorizeWeightVisitor<'_> {
            fn inspect_attrs(&mut self, attrs: &[Attribute], fallback_span: Span) {
                let Some(authorize_attr) = authorize_attr(attrs) else {
                    return;
                };
                if is_masked_span(self.mask, authorize_attr.span())
                    || has_weight_of_authorize(attrs)
                {
                    return;
                }

                let span = authorize_attr.span();
                self.diagnostics.push(Diagnostic {
                    rule_id: self.rule_id.to_string(),
                    rule_name: self.rule_name.to_string(),
                    category: RuleCategory::Semantic,
                    severity: self.severity,
                    file: self.file.to_path_buf(),
                    line: span_line(span),
                    column: Some(span_column(span)),
                    end_line: Some(span_line(fallback_span)),
                    message: "`#[pallet::authorize]` is missing a companion `#[pallet::weight_of_authorize(...)]` attribute"
                        .to_string(),
                    explanation: "Authorized calls benchmark validation separately from execution. \
                        Without `#[pallet::weight_of_authorize(...)]`, the authorize path has no \
                        dedicated weight hook and can silently drift from the real validation cost."
                        .to_string(),
                    suggestion: Some(
                        "Add `#[pallet::weight_of_authorize(T::WeightInfo::authorize_your_call())]` near the call attributes".to_string(),
                    ),
                });
            }
        }

        impl<'ast> Visit<'ast> for AuthorizeWeightVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                self.inspect_attrs(&item_fn.attrs, item_fn.sig.ident.span());
                visit::visit_item_fn(self, item_fn);
            }

            fn visit_impl_item(&mut self, item: &'ast ImplItem) {
                if let ImplItem::Fn(item_fn) = item {
                    self.inspect_attrs(&item_fn.attrs, item_fn.sig.ident.span());
                }
                visit::visit_impl_item(self, item);
            }
        }

        let mut visitor = AuthorizeWeightVisitor {
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
// SEM016: CreateAuthorizedTransaction should include AuthorizeCall::new()
// ---------------------------------------------------------------------------

pub struct MissingAuthorizeCallInCreateAuthorizedTransaction;

impl LintRule for MissingAuthorizeCallInCreateAuthorizedTransaction {
    fn id(&self) -> &str {
        "SEM016"
    }
    fn name(&self) -> &str {
        "missing-authorizecall-in-create-authorized-transaction"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;

        struct AuthorizeCallVisitor {
            found: bool,
        }

        impl<'ast> Visit<'ast> for AuthorizeCallVisitor {
            fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                if let Some(path) = expr_call_path(expr_call) {
                    if path_has_segment(path, "AuthorizeCall")
                        && path_last_ident(path).as_deref() == Some("new")
                    {
                        self.found = true;
                    }
                }
                visit::visit_expr_call(self, expr_call);
            }
        }

        struct CreateAuthorizedTransactionVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        impl<'ast> Visit<'ast> for CreateAuthorizedTransactionVisitor<'_> {
            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                if is_masked_span(self.mask, item_impl.span()) {
                    return;
                }

                let Some((_, trait_path, _)) = &item_impl.trait_ else {
                    return;
                };
                if !path_has_segment(trait_path, "CreateAuthorizedTransaction") {
                    return;
                }

                for impl_item in &item_impl.items {
                    let ImplItem::Fn(item_fn) = impl_item else {
                        continue;
                    };
                    if item_fn.sig.ident != "create_extension"
                        || is_masked_span(self.mask, item_fn.span())
                    {
                        continue;
                    }

                    let mut authorize_call_visitor = AuthorizeCallVisitor { found: false };
                    authorize_call_visitor.visit_block(&item_fn.block);
                    if authorize_call_visitor.found {
                        continue;
                    }

                    self.diagnostics.push(Diagnostic {
                        rule_id: self.rule_id.to_string(),
                        rule_name: self.rule_name.to_string(),
                        category: RuleCategory::Semantic,
                        severity: self.severity,
                        file: self.file.to_path_buf(),
                        line: span_line(item_fn.sig.ident.span()),
                        column: Some(span_column(item_fn.sig.ident.span())),
                        end_line: None,
                        message: "`CreateAuthorizedTransaction::create_extension()` should include `AuthorizeCall::new()`"
                            .to_string(),
                        explanation: "`AuthorizeCall` routes `#[pallet::authorize]` closures through the \
                            transaction validation pipeline. Omitting it can make authorized OCW or \
                            mock transactions bypass the authorize stage entirely."
                            .to_string(),
                        suggestion: Some(
                            "Add `frame_system::AuthorizeCall::<T>::new()` (or `AuthorizeCall::new()`) to the returned extension tuple".to_string(),
                        ),
                    });
                }

                visit::visit_item_impl(self, item_impl);
            }
        }

        let mut visitor = CreateAuthorizedTransactionVisitor {
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
        let diagnostics = collect_dispatchables(ast, &test_mask)
            .into_iter()
            .filter(|dispatchable| dispatchable.origin != DispatchableOrigin::Privileged)
            .filter(|dispatchable| !dispatchable.is_deprecated && !dispatchable.has_max_weight())
            .filter(|dispatchable| {
                dispatchable.unbounded_vec_params().any(|param| {
                    !param.is_bounded_in_body
                        && !dispatchable.weight_accounts_for_param(&param.name)
                })
            })
            .map(|dispatchable| Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                category: RuleCategory::Semantic,
                severity: config.rule_severity(self.id(), Severity::Warning),
                file: ctx.path.clone(),
                line: span_line(dispatchable.span),
                column: Some(span_column(dispatchable.span)),
                end_line: None,
                message: format!(
                    "Extrinsic `{}` accepts unbounded `Vec<T>` parameter",
                    dispatchable.name
                ),
                explanation: "Unbounded `Vec<T>` in extrinsic parameters allows attackers \
                    to pass arbitrarily large inputs, causing memory exhaustion or \
                    overweight blocks. Use `BoundedVec<T, MaxLen>` to enforce an upper bound."
                    .to_string(),
                suggestion: Some(
                    "Replace `Vec<T>` with `BoundedVec<T, ConstU32<MAX>>`".to_string(),
                ),
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
        if !is_runtime_or_pallet_security_path(&ctx.path) {
            return None;
        }
        if is_documented_benchmark_test_builder(&ctx.path, ctx.content) {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = merge_masks(
            &cfg_test_module_mask(ctx.content),
            &integrity_test_mask(ctx.content),
        );

        fn text_marks_proven_invariant(text: &str) -> bool {
            let lower = text.to_ascii_lowercase();
            lower.contains("qed")
                || lower.contains("checked in prep")
                || lower.contains("checked by")
                || lower.contains("invariant")
                || lower.contains("precondition")
                || lower.contains("defensive")
                || lower.contains("should not fail")
                || lower.contains("should never")
                || lower.contains("should resolve to")
                || lower.contains("should be handled")
                || lower.contains("was supposed to be")
                || lower.contains("should exist in storage")
                || lower.contains("outgoing must be only item")
                || lower.contains("must be in ready ring")
                || lower.contains("implies there are pages")
                || lower.contains("we never bail")
                || lower.contains("cannot change in any case")
        }

        fn debug_assert_message_marks_proven_invariant(mac: &Macro) -> bool {
            text_marks_proven_invariant(&mac.tokens.to_string())
        }

        fn comment_marks_proven_invariant(comment: &str) -> bool {
            let lower = comment.to_ascii_lowercase();
            text_marks_proven_invariant(comment)
                || lower.contains("state is sane")
                || lower.contains("sane things")
                || lower.contains("sanity")
                || lower.contains("post-condition")
                || lower.contains("postcondition")
                || lower.contains("consistency check")
                || lower.contains("state is not corrupted")
                || lower.contains("assumption of the trait")
                || (lower.contains("safety:")
                    && (lower.contains("validated by the caller")
                        || lower.contains("caller ensures")))
                || lower.contains("verified by the client")
                || lower.contains("verify the expectation")
        }

        fn nearby_comment_marks_proven_invariant(lines: &[&str], line: usize) -> bool {
            let current_idx = line.saturating_sub(1);
            let start = current_idx.saturating_sub(4);
            lines[start..=current_idx.min(lines.len().saturating_sub(1))]
                .iter()
                .any(|source_line| {
                    source_line
                        .split_once("//")
                        .is_some_and(|(_, comment)| comment_marks_proven_invariant(comment))
                })
        }

        fn debug_assert_has_extended_invariant_comment(
            mac: &Macro,
            lines: &[&str],
            line: usize,
        ) -> bool {
            let compact_tokens = mac
                .tokens
                .to_string()
                .split_whitespace()
                .collect::<String>();
            if compact_tokens != "false" && !compact_tokens.contains(".is_ok()") {
                return false;
            }

            let current_idx = line.saturating_sub(1);
            let start = current_idx.saturating_sub(8);
            lines[start..current_idx.min(lines.len())]
                .iter()
                .any(|source_line| {
                    source_line
                        .split_once("//")
                        .is_some_and(|(_, comment)| comment_marks_proven_invariant(comment))
                })
        }

        fn debug_assert_checks_defensive_result(mac: &Macro, lines: &[&str], line: usize) -> bool {
            let current_idx = line.saturating_sub(1);
            let start = current_idx.saturating_sub(10);
            let context = lines[start..current_idx.min(lines.len())].join("\n");
            if !context.contains(".defensive()") {
                return false;
            }

            let tokens = mac.tokens.to_string();
            tokens
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .filter(|token| !token.is_empty())
                .any(|token| {
                    if !token.starts_with('_') && !token.ends_with("_res") {
                        return false;
                    }
                    [format!("let {token}"), format!("let mut {token}")]
                        .iter()
                        .any(|binding| {
                            context
                                .rfind(binding)
                                .is_some_and(|idx| context[idx..].contains(".defensive()"))
                        })
                })
        }

        fn debug_assert_checks_balance_remainder(mac: &Macro, lines: &[&str], line: usize) -> bool {
            let tokens = mac.tokens.to_string();
            let compact_tokens = tokens.split_whitespace().collect::<String>();
            let current_idx = line.saturating_sub(1);
            let start = current_idx.saturating_sub(10);
            let context = lines[start..current_idx.min(lines.len())].join("\n");
            if !(context.contains("unreserve(") || context.contains("slash_reserved(")) {
                return false;
            }

            tokens
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .filter(|token| !token.is_empty())
                .any(|token| {
                    if !compact_tokens.contains(&format!("{token}.is_zero()")) {
                        return false;
                    }
                    let is_unreserve_remainder =
                        context.rfind(&format!("let {token}")).is_some_and(|idx| {
                            context[idx..]
                                .split_once(';')
                                .map(|(statement, _)| statement)
                                .unwrap_or(&context[idx..])
                                .contains("unreserve(")
                        });
                    let is_slash_reserved_remainder = [format!(", {token})"), format!(",{token})")]
                        .iter()
                        .any(|binding| {
                            context.rfind(binding).is_some_and(|idx| {
                                context[idx..]
                                    .split_once(';')
                                    .map(|(statement, _)| statement)
                                    .unwrap_or(&context[idx..])
                                    .contains("slash_reserved(")
                            })
                        });
                    is_unreserve_remainder || is_slash_reserved_remainder
                })
        }

        fn debug_assert_checks_bounded_clear_prefix_result(
            mac: &Macro,
            lines: &[&str],
            line: usize,
        ) -> bool {
            let compact_tokens = mac
                .tokens
                .to_string()
                .split_whitespace()
                .collect::<String>();
            if !compact_tokens.contains(".unique<=") {
                return false;
            }

            let current_idx = line.saturating_sub(1);
            let start = current_idx.saturating_sub(6);
            let context = lines[start..current_idx.min(lines.len())].join("\n");
            let lower_context = context.to_ascii_lowercase();
            context.contains("clear_prefix(")
                && lower_context.contains("safe to remove unbounded")
                && lower_context.contains("at most")
        }

        struct DebugAssertVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
            lines: &'a [&'a str],
        }

        impl<'ast> Visit<'ast> for DebugAssertVisitor<'_> {
            fn visit_macro(&mut self, mac: &'ast Macro) {
                let Some(name) = path_last_ident(&mac.path) else {
                    visit::visit_macro(self, mac);
                    return;
                };
                if matches!(
                    name.as_str(),
                    "debug_assert" | "debug_assert_eq" | "debug_assert_ne"
                ) && !is_masked_span(self.mask, mac.span())
                    && !debug_assert_message_marks_proven_invariant(mac)
                    && !nearby_comment_marks_proven_invariant(self.lines, span_line(mac.span()))
                    && !debug_assert_has_extended_invariant_comment(
                        mac,
                        self.lines,
                        span_line(mac.span()),
                    )
                    && !debug_assert_checks_defensive_result(mac, self.lines, span_line(mac.span()))
                    && !debug_assert_checks_balance_remainder(
                        mac,
                        self.lines,
                        span_line(mac.span()),
                    )
                    && !debug_assert_checks_bounded_clear_prefix_result(
                        mac,
                        self.lines,
                        span_line(mac.span()),
                    )
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
                        message:
                            "`debug_assert!` in production code — panics in debug, stripped in release"
                                .to_string(),
                        explanation: "`debug_assert!` panics in debug builds and is completely removed \
                            in release builds. Neither is correct for runtime code: panics crash the \
                            node, and silent removal means the invariant is unchecked. Use \
                            `defensive!()` or a proper error return instead."
                            .to_string(),
                        suggestion: Some("Replace with `defensive!()` or return an error".to_string()),
                    });
                }
                visit::visit_macro(self, mac);
            }
        }

        let content_lines = ctx.content.lines().collect::<Vec<_>>();
        let mut visitor = DebugAssertVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
            lines: &content_lines,
        };
        visitor.visit_file(ast);
        let diagnostics = visitor.diagnostics;

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

        fn is_recursive_runtime_decode(path: &syn::Path) -> bool {
            let tokens = compact_tokens(path);
            ["RuntimeCall", "UncheckedExtrinsic", "OpaqueExtrinsic"]
                .iter()
                .any(|name| tokens.contains(name))
        }

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
                    && is_recursive_runtime_decode(path)
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
                if expr_method_call.method == "using_encoded" {
                    visit::visit_expr(self, &expr_method_call.receiver);
                    return;
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

        fn is_direct_repatriate_reserved_expr(expr: &Expr) -> bool {
            match strip_expr_wrappers(expr) {
                Expr::Call(expr_call) => expr_call_path(expr_call)
                    .map(|path| path_has_exact_ident(path, "repatriate_reserved"))
                    .unwrap_or(false),
                Expr::MethodCall(expr_method_call) => {
                    expr_method_call.method == "repatriate_reserved"
                }
                _ => false,
            }
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

        fn has_remaining_accounted_in_transfer_amount(
            lines: &[&str],
            mask: &[bool],
            line: usize,
            name: &str,
        ) -> bool {
            let window_end = (line + 12).min(lines.len());
            let saturating_sub = format!("saturating_sub({name})");
            let defensive_saturating_sub = format!("defensive_saturating_sub({name})");
            (line..window_end).any(|j| {
                if mask[j] {
                    return false;
                }
                let compact: String = strip_strings_and_line_comments(lines[j])
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                compact.contains(&saturating_sub) || compact.contains(&defensive_saturating_sub)
            })
        }

        fn has_documented_infallible_debug_assert(
            lines: &[&str],
            mask: &[bool],
            line: usize,
            name: &str,
        ) -> bool {
            let line_idx = line.saturating_sub(1);
            let comment_start = line_idx.saturating_sub(16);
            let docs_say_not_actionable = lines[comment_start..line_idx.min(lines.len())]
                .iter()
                .filter_map(|source_line| source_line.split_once("//").map(|(_, comment)| comment))
                .map(str::to_ascii_lowercase)
                .any(|comment| {
                    (comment.contains("no reason") && comment.contains("should fail"))
                        || (comment.contains("cannot do much") && comment.contains("fail"))
                        || (comment.contains("can't do much") && comment.contains("fail"))
                });
            if !docs_say_not_actionable {
                return false;
            }

            let window_end = (line + 5).min(lines.len());
            (line..window_end).any(|j| {
                !mask[j]
                    && lines[j].contains("debug_assert")
                    && lines[j].contains(name)
                    && (lines[j].contains(".is_ok()") || lines[j].contains("Ok("))
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
                if !is_direct_repatriate_reserved_expr(&init.expr) {
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
                        ) && !has_remaining_accounted_in_transfer_amount(
                            self.lines,
                            self.mask,
                            span_line(local.span()),
                            &name,
                        ) && !has_documented_infallible_debug_assert(
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
        if !is_runtime_or_pallet_security_path(&ctx.path) {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = cfg_test_module_mask(ctx.content);
        let result_hints = [
            "::reserve(",
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

        let lines: Vec<&str> = ctx.content.lines().collect();

        fn comment_marks_intentional_result_discard(comment: &str) -> bool {
            let lower = comment.to_ascii_lowercase();
            lower.contains("ignore errors")
                || lower.contains("ignore the error")
                || lower.contains("ignored error")
                || lower.contains("intentionally ignore")
                || lower.contains("intentionally discard")
                || lower.contains("can only fail")
                || lower.contains("cannot fail")
                || lower.contains("can't fail")
                || lower.contains("shouldn't fail")
                || lower.contains("should not fail")
                || (lower.contains("don't really care") && lower.contains("fail"))
                || (lower.contains("best effort")
                    && (lower.contains("error")
                        || lower.contains("result")
                        || lower.contains("fail")))
        }

        fn nearby_comment_marks_intentional_result_discard(lines: &[&str], line: usize) -> bool {
            let current_idx = line.saturating_sub(1);
            let start = current_idx.saturating_sub(8);
            lines[start..current_idx.min(lines.len())]
                .iter()
                .any(|source_line| {
                    source_line.split_once("//").is_some_and(|(_, comment)| {
                        comment_marks_intentional_result_discard(comment)
                    })
                })
        }

        fn map_err_has_explicit_side_effect(rhs: &str) -> bool {
            let lower = rhs.to_ascii_lowercase();
            lower.contains(".map_err(")
                && (lower.contains("log!(")
                    || lower.contains("tracing::")
                    || lower.contains("failures+="))
        }

        fn try_mutate_uses_unit_error_for_control_flow(rhs: &str) -> bool {
            rhs.contains("::try_mutate(") && rhs.contains("Err(())") && rhs.contains("Ok(())")
        }

        struct LetUnderscoreVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
            lines: &'a [&'a str],
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
                if expr_is_top_level_try(&init.expr) {
                    visit::visit_local(self, local);
                    return;
                }
                let rhs: String = init
                    .expr
                    .to_token_stream()
                    .to_string()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let line = span_line(local.span());
                if self.result_hints.iter().any(|hint| rhs.contains(hint))
                    && !nearby_comment_marks_intentional_result_discard(self.lines, line)
                    && !map_err_has_explicit_side_effect(&rhs)
                    && !try_mutate_uses_unit_error_for_control_flow(&rhs)
                {
                    self.diagnostics.push(Diagnostic {
						rule_id: self.rule_id.to_string(),
						rule_name: self.rule_name.to_string(),
						category: RuleCategory::Semantic,
						severity: self.severity,
						file: self.file.to_path_buf(),
						line,
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
            lines: &lines,
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
        if !is_runtime_or_pallet_security_path(&ctx.path) {
            return None;
        }
        if is_documented_benchmark_test_builder(&ctx.path, ctx.content) {
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

        let ast = ast_file(ctx)?;
        let test_mask = merge_masks(
            &merge_masks(
                &merge_masks(
                    &cfg_test_module_mask(ctx.content),
                    &genesis_build_mask(ctx.content),
                ),
                &merge_masks(
                    &genesis_config_impl_mask(ctx.content),
                    &const_unit_initializer_mask(ctx.content),
                ),
            ),
            &integrity_test_mask(ctx.content),
        );
        let uninhabited_enums: HashSet<String> = ast
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Enum(item_enum) if item_enum.variants.is_empty() => {
                    Some(item_enum.ident.to_string())
                }
                _ => None,
            })
            .collect();
        let const_int_values: HashMap<String, u128> = ast
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Const(item_const) => expr_integer_literal_value(&item_const.expr)
                    .map(|value| (item_const.ident.to_string(), value)),
                _ => None,
            })
            .collect();

        fn const_u32_bound_from_tokens(
            tokens: &str,
            const_int_values: &HashMap<String, u128>,
        ) -> Option<u128> {
            let marker = "ConstU32<";
            let start = tokens.find(marker)? + marker.len();
            let end = tokens[start..].find('>')? + start;
            let bound = &tokens[start..end];
            bound
                .parse::<u128>()
                .ok()
                .or_else(|| const_int_values.get(bound).copied())
        }

        fn vec_macro_literal_len(expr: &Expr) -> Option<usize> {
            let Expr::Macro(expr_macro) = strip_expr_wrappers(expr) else {
                return None;
            };
            if macro_name(&expr_macro.mac).as_deref() != Some("vec")
                || expr_macro.mac.tokens.to_string().contains(';')
            {
                return None;
            }
            expr_macro
                .mac
                .parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
                .ok()
                .map(|items| items.len())
        }

        fn bounded_vec_literal_try_from_fits_bound(
            expr_method_call: &ExprMethodCall,
            const_int_values: &HashMap<String, u128>,
        ) -> bool {
            if !matches!(
                expr_method_call.method.to_string().as_str(),
                "unwrap" | "expect"
            ) {
                return false;
            }
            let Expr::Call(expr_call) = strip_expr_wrappers(&expr_method_call.receiver) else {
                return false;
            };
            let Some(path) = expr_call_path(expr_call) else {
                return false;
            };
            if !path_has_exact_ident(path, "try_from") {
                return false;
            }
            let func_tokens = compact_tokens(&expr_call.func);
            if !func_tokens.contains("BoundedVec") {
                return false;
            }
            let Some(bound) = const_u32_bound_from_tokens(&func_tokens, const_int_values) else {
                return false;
            };
            let Some(arg_len) = expr_call.args.first().and_then(vec_macro_literal_len) else {
                return false;
            };
            (arg_len as u128) <= bound
        }

        fn vec_literal_try_into_fits_min_one_bound(expr_method_call: &ExprMethodCall) -> bool {
            if expr_method_call.method != "expect" {
                return false;
            }
            let Some(message) = expr_method_call.args.first().and_then(expr_string_literal) else {
                return false;
            };
            if !message
                .to_ascii_lowercase()
                .contains("must be greater than or equal to 1")
            {
                return false;
            }

            let Expr::MethodCall(try_into_call) = strip_expr_wrappers(&expr_method_call.receiver)
            else {
                return false;
            };
            if try_into_call.method != "try_into" || !try_into_call.args.is_empty() {
                return false;
            }

            vec_macro_literal_len(&try_into_call.receiver).is_some_and(|len| len <= 1)
        }

        #[derive(Clone, Copy)]
        struct BigUintVarState {
            is_biguint: bool,
            const_value: Option<u128>,
            bounded_to_usize: bool,
        }

        fn biguint_from_call_value(expr_call: &ExprCall) -> Option<Option<u128>> {
            let path = expr_call_path(expr_call)?;
            if path_owner_name(path).as_deref() != Some("BigUint") {
                return None;
            }

            match path_last_ident(path).as_deref() {
                Some("from_bytes_be") => Some(None),
                Some("from") => expr_call
                    .args
                    .first()
                    .and_then(expr_integer_literal_value)
                    .map(Some),
                _ => None,
            }
        }

        fn biguint_state_from_expr(expr: &Expr) -> Option<BigUintVarState> {
            let Expr::Call(expr_call) = strip_expr_wrappers(expr) else {
                return None;
            };
            biguint_from_call_value(expr_call).map(|const_value| BigUintVarState {
                is_biguint: true,
                const_value,
                bounded_to_usize: const_value.is_some_and(|value| value <= u32::MAX as u128),
            })
        }

        fn err_is_propagated(expr: &Expr) -> bool {
            match expr {
                Expr::Try(expr_try) => {
                    let Expr::Call(expr_call) = strip_expr_wrappers(&expr_try.expr) else {
                        return false;
                    };
                    expr_call_path(expr_call)
                        .and_then(path_last_ident)
                        .as_deref()
                        == Some("Err")
                }
                Expr::Return(expr_return) => {
                    let Some(expr) = expr_return.expr.as_deref() else {
                        return false;
                    };
                    let Expr::Call(expr_call) = strip_expr_wrappers(expr) else {
                        return false;
                    };
                    expr_call_path(expr_call)
                        .and_then(path_last_ident)
                        .as_deref()
                        == Some("Err")
                }
                _ => false,
            }
        }

        fn block_rejects_path(block: &syn::Block) -> bool {
            block.stmts.iter().any(|stmt| match stmt {
                Stmt::Expr(expr, _) => err_is_propagated(expr),
                _ => false,
            })
        }

        struct PanicVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            reported_lines: HashSet<usize>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
            uninhabited_enums: &'a HashSet<String>,
            const_int_values: &'a HashMap<String, u128>,
            uninhabited_impl_depth: usize,
            test_default_impl_depth: usize,
            h160_param_stack: Vec<HashSet<String>>,
            biguint_scope_stack: Vec<HashMap<String, BigUintVarState>>,
        }

        impl PanicVisitor<'_> {
            fn current_h160_params(&self) -> HashSet<String> {
                self.h160_param_stack
                    .iter()
                    .flat_map(|params| params.iter().cloned())
                    .collect()
            }

            fn current_biguint_state(&self, name: &str) -> Option<BigUintVarState> {
                self.biguint_scope_stack
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name).copied())
            }

            fn current_scope_mut(&mut self) -> Option<&mut HashMap<String, BigUintVarState>> {
                self.biguint_scope_stack.last_mut()
            }

            fn mark_local(&mut self, local: &Local) {
                let Pat::Ident(pat_ident) = strip_pat_wrappers(&local.pat) else {
                    return;
                };
                let state = local
                    .init
                    .as_ref()
                    .and_then(|init| biguint_state_from_expr(&init.expr))
                    .unwrap_or(BigUintVarState {
                        is_biguint: false,
                        const_value: None,
                        bounded_to_usize: false,
                    });
                if let Some(scope) = self.current_scope_mut() {
                    scope.insert(pat_ident.ident.to_string(), state);
                }
            }

            fn expr_biguint_bound_value(&self, expr: &Expr) -> Option<u128> {
                if let Some(value) = expr_integer_literal_value(expr) {
                    return Some(value);
                }
                if let Some(ident) = expr_path_last_ident(expr) {
                    if let Some(value) = self
                        .current_biguint_state(&ident)
                        .and_then(|state| state.const_value)
                    {
                        return Some(value);
                    }
                }
                let Expr::Call(expr_call) = strip_expr_wrappers(expr) else {
                    return None;
                };
                biguint_from_call_value(expr_call).flatten()
            }

            fn bounded_biguint_from_rejecting_if(&self, expr_if: &ExprIf) -> Option<String> {
                if !block_rejects_path(&expr_if.then_branch) {
                    return None;
                }
                let Expr::Binary(expr_binary) = strip_expr_wrappers(&expr_if.cond) else {
                    return None;
                };

                let (candidate, bound_expr) = match expr_binary.op {
                    syn::BinOp::Gt(_) | syn::BinOp::Ge(_) => {
                        (&expr_binary.left, &expr_binary.right)
                    }
                    syn::BinOp::Lt(_) | syn::BinOp::Le(_) => {
                        (&expr_binary.right, &expr_binary.left)
                    }
                    _ => return None,
                };

                let name = expr_path_last_ident(candidate)?;
                let state = self.current_biguint_state(&name)?;
                if !state.is_biguint {
                    return None;
                }
                let bound = self.expr_biguint_bound_value(bound_expr)?;
                (bound <= u32::MAX as u128).then_some(name)
            }

            fn mark_rejected_biguint_upper_bound(&mut self, expr_if: &ExprIf) {
                let Some(name) = self.bounded_biguint_from_rejecting_if(expr_if) else {
                    return;
                };
                let mut state = self
                    .current_biguint_state(&name)
                    .unwrap_or(BigUintVarState {
                        is_biguint: true,
                        const_value: None,
                        bounded_to_usize: false,
                    });
                state.bounded_to_usize = true;
                if let Some(scope) = self.current_scope_mut() {
                    scope.insert(name, state);
                }
            }

            fn receiver_is_bounded_biguint_to_usize(
                &self,
                expr_method_call: &ExprMethodCall,
            ) -> bool {
                let Expr::MethodCall(to_usize_call) =
                    strip_expr_wrappers(&expr_method_call.receiver)
                else {
                    return false;
                };
                if to_usize_call.method != "to_usize" || !to_usize_call.args.is_empty() {
                    return false;
                }
                let Some(name) = expr_path_last_ident(&to_usize_call.receiver) else {
                    return false;
                };
                self.current_biguint_state(&name)
                    .is_some_and(|state| state.is_biguint && state.bounded_to_usize)
            }

            fn push_diag(&mut self, span: Span, pattern: &str, suggestion: &str) {
                let line = span_line(span);
                if !self.reported_lines.insert(line) {
                    return;
                }
                self.diagnostics.push(Diagnostic {
					rule_id: self.rule_id.to_string(),
					rule_name: self.rule_name.to_string(),
					category: RuleCategory::Semantic,
					severity: self.severity,
					file: self.file.to_path_buf(),
					line,
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
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                self.h160_param_stack.push(h160_param_names(&item_fn.sig));
                self.biguint_scope_stack.push(HashMap::new());
                visit::visit_item_fn(self, item_fn);
                self.biguint_scope_stack.pop();
                self.h160_param_stack.pop();
            }

            fn visit_impl_item_fn(&mut self, item_fn: &'ast syn::ImplItemFn) {
                self.h160_param_stack.push(h160_param_names(&item_fn.sig));
                self.biguint_scope_stack.push(HashMap::new());
                visit::visit_impl_item_fn(self, item_fn);
                self.biguint_scope_stack.pop();
                self.h160_param_stack.pop();
            }

            fn visit_block(&mut self, block: &'ast syn::Block) {
                self.biguint_scope_stack.push(HashMap::new());
                for stmt in &block.stmts {
                    self.visit_stmt(stmt);
                    match stmt {
                        Stmt::Local(local) => self.mark_local(local),
                        Stmt::Expr(Expr::If(expr_if), _) => {
                            self.mark_rejected_biguint_upper_bound(expr_if);
                        }
                        _ => {}
                    }
                }
                self.biguint_scope_stack.pop();
            }

            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                let is_uninhabited_impl = type_last_ident(&item_impl.self_ty)
                    .as_ref()
                    .is_some_and(|ident| self.uninhabited_enums.contains(ident));
                if is_uninhabited_impl {
                    self.uninhabited_impl_depth += 1;
                }
                let is_test_default_impl = type_last_ident(&item_impl.self_ty)
                    .as_deref()
                    .is_some_and(|ident| ident == "TestDefaultConfig");
                if is_test_default_impl {
                    self.test_default_impl_depth += 1;
                }

                visit::visit_item_impl(self, item_impl);

                if is_uninhabited_impl {
                    self.uninhabited_impl_depth -= 1;
                }
                if is_test_default_impl {
                    self.test_default_impl_depth -= 1;
                }
            }

            fn visit_expr_method_call(&mut self, expr_method_call: &'ast ExprMethodCall) {
                if is_masked_span(self.mask, expr_method_call.span()) {
                    visit::visit_expr_method_call(self, expr_method_call);
                    return;
                }
                if self.test_default_impl_depth > 0 {
                    visit::visit_expr_method_call(self, expr_method_call);
                    return;
                }
                match expr_method_call.method.to_string().as_str() {
                    "unwrap"
                        if !unwrap_receiver_is_nonzero_literal_new(expr_method_call)
                            && !panic_receiver_is_fixed_range_try_into(expr_method_call)
                            && !panic_receiver_is_h160_suffix_try_into(
                                expr_method_call,
                                &self.current_h160_params(),
                            )
                            && !self.receiver_is_bounded_biguint_to_usize(expr_method_call)
                            && !bounded_vec_literal_try_from_fits_bound(
                                expr_method_call,
                                self.const_int_values,
                            ) =>
                    {
                        self.push_diag(
                            expr_method_call.span(),
                            ".unwrap()",
                            "Use `.ok_or(Error::...)?`, `.unwrap_or_default()`, or `.defensive()`",
                        )
                    }
                    "expect"
                        if !expect_message_marks_proven_invariant(expr_method_call)
                            && !panic_receiver_is_fixed_range_try_into(expr_method_call)
                            && !panic_receiver_is_h160_suffix_try_into(
                                expr_method_call,
                                &self.current_h160_params(),
                            )
                            && !self.receiver_is_bounded_biguint_to_usize(expr_method_call)
                            && !bounded_vec_literal_try_from_fits_bound(
                                expr_method_call,
                                self.const_int_values,
                            )
                            && !vec_literal_try_into_fits_min_one_bound(expr_method_call) =>
                    {
                        self.push_diag(
                            expr_method_call.span(),
                            ".expect()",
                            "Use `.ok_or(Error::...)?` or `.defensive()`",
                        )
                    }
                    _ => {}
                }
                visit::visit_expr_method_call(self, expr_method_call);
            }

            fn visit_macro(&mut self, mac: &'ast Macro) {
                if is_masked_span(self.mask, mac.span()) {
                    visit::visit_macro(self, mac);
                    return;
                }
                if self.test_default_impl_depth > 0 {
                    visit::visit_macro(self, mac);
                    return;
                }
                let proven_invariant = macro_message_marks_proven_invariant(mac);
                match path_last_ident(&mac.path).as_deref() {
                    Some("panic") if !proven_invariant => self.push_diag(
                        mac.span(),
                        "panic!()",
                        "Return an error or use `defensive!()`",
                    ),
                    Some("unreachable")
                        if !proven_invariant && self.uninhabited_impl_depth == 0 =>
                    {
                        self.push_diag(
                            mac.span(),
                            "unreachable!()",
                            "Use `defensive_unreachable!()` or return an error",
                        )
                    }
                    Some("todo") if !proven_invariant => self.push_diag(
                        mac.span(),
                        "todo!()",
                        "Implement the function or return an error",
                    ),
                    Some("unimplemented") if !proven_invariant => self.push_diag(
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
            reported_lines: HashSet::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
            uninhabited_enums: &uninhabited_enums,
            const_int_values: &const_int_values,
            uninhabited_impl_depth: 0,
            test_default_impl_depth: 0,
            h160_param_stack: Vec::new(),
            biguint_scope_stack: Vec::new(),
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
        if !is_runtime_or_pallet_security_path(&ctx.path) {
            return None;
        }
        if is_documented_benchmark_test_builder(&ctx.path, ctx.content) {
            return None;
        }

        let path_str = ctx.rel_path.to_string_lossy();
        if path_str.ends_with("weights.rs") || path_str.contains("/weights/") {
            return None;
        }

        let ast = ast_file(ctx)?;
        let test_mask = merge_masks(
            &cfg_test_module_mask(ctx.content),
            &integrity_test_mask(ctx.content),
        );

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

        fn ordered_operand_key(expr: &Expr) -> String {
            match strip_expr_wrappers(expr) {
                Expr::Lit(expr_lit) => {
                    if let Lit::Int(int_lit) = &expr_lit.lit {
                        int_lit.base10_digits().to_string()
                    } else {
                        compact_tokens(expr_lit)
                    }
                }
                Expr::MethodCall(method_call)
                    if method_call.method == "into" && method_call.args.is_empty() =>
                {
                    ordered_operand_key(&method_call.receiver)
                }
                stripped => compact_tokens(stripped),
            }
        }

        fn method_receiver_and_single_arg(expr: &Expr, method: &str) -> Option<(String, String)> {
            let Expr::MethodCall(method_call) = strip_expr_wrappers(expr) else {
                return None;
            };
            if method_call.method != method || method_call.args.len() != 1 {
                return None;
            }
            let receiver = compact_tokens(&method_call.receiver);
            let arg = compact_tokens(method_call.args.first()?);
            Some((receiver, arg))
        }

        fn same_unordered_operands(left: &(String, String), right: &(String, String)) -> bool {
            (left.0 == right.0 && left.1 == right.1) || (left.0 == right.1 && left.1 == right.0)
        }

        fn max_min_difference_is_nonnegative(expr_binary: &ExprBinary) -> bool {
            if !matches!(expr_binary.op, syn::BinOp::Sub(_)) {
                return false;
            }
            let Some(max_operands) = method_receiver_and_single_arg(&expr_binary.left, "max")
            else {
                return false;
            };
            let Some(min_operands) = method_receiver_and_single_arg(&expr_binary.right, "min")
            else {
                return false;
            };
            same_unordered_operands(&max_operands, &min_operands)
        }

        fn safe_subtraction_from_ordering(expr_binary: &ExprBinary) -> Option<(String, String)> {
            let left = ordered_operand_key(&expr_binary.left);
            let right = ordered_operand_key(&expr_binary.right);
            match expr_binary.op {
                syn::BinOp::Gt(_) | syn::BinOp::Ge(_) => Some((left, right)),
                syn::BinOp::Lt(_) | syn::BinOp::Le(_) => Some((right, left)),
                _ => None,
            }
        }

        fn safe_subtraction_from_inverse_ordering(
            expr_binary: &ExprBinary,
        ) -> Option<(String, String)> {
            let left = ordered_operand_key(&expr_binary.left);
            let right = ordered_operand_key(&expr_binary.right);
            match expr_binary.op {
                syn::BinOp::Gt(_) | syn::BinOp::Ge(_) => Some((right, left)),
                syn::BinOp::Lt(_) | syn::BinOp::Le(_) => Some((left, right)),
                _ => None,
            }
        }

        fn safe_subtraction_from_method_ordering(expr: &Expr) -> Option<(String, String)> {
            let Expr::MethodCall(method_call) = strip_expr_wrappers(expr) else {
                return None;
            };
            if method_call.args.len() != 1 {
                return None;
            }
            let left = ordered_operand_key(&method_call.receiver);
            let right = ordered_operand_key(method_call.args.first()?);
            match method_call.method.to_string().as_str() {
                "gt" | "ge" => Some((left, right)),
                "lt" | "le" => Some((right, left)),
                _ => None,
            }
        }

        fn inverse_safe_subtraction(pair: &(String, String)) -> (String, String) {
            (pair.1.clone(), pair.0.clone())
        }

        fn safe_subtractions_from_condition(
            expr: &Expr,
        ) -> Option<((String, String), (String, String))> {
            match strip_expr_wrappers(expr) {
                Expr::Binary(condition) => {
                    safe_subtraction_from_ordering(condition).map(|then_pair| {
                        let else_pair = safe_subtraction_from_inverse_ordering(condition)
                            .unwrap_or_else(|| inverse_safe_subtraction(&then_pair));
                        (then_pair, else_pair)
                    })
                }
                Expr::MethodCall(_) => {
                    safe_subtraction_from_method_ordering(expr).map(|then_pair| {
                        let else_pair = inverse_safe_subtraction(&then_pair);
                        (then_pair, else_pair)
                    })
                }
                _ => None,
            }
        }

        fn subtraction_operands(expr_binary: &ExprBinary) -> Option<(String, String)> {
            matches!(expr_binary.op, syn::BinOp::Sub(_)).then(|| {
                (
                    ordered_operand_key(&expr_binary.left),
                    ordered_operand_key(&expr_binary.right),
                )
            })
        }

        fn cmp_match_operands(expr: &Expr) -> Option<(String, String)> {
            let Expr::MethodCall(method_call) = strip_expr_wrappers(expr) else {
                return None;
            };
            if method_call.method != "cmp" || method_call.args.len() != 1 {
                return None;
            }
            Some((
                ordered_operand_key(&method_call.receiver),
                ordered_operand_key(method_call.args.first()?),
            ))
        }

        fn ordering_pat_name(pat: &Pat) -> Option<String> {
            match pat {
                Pat::Ident(ident) => Some(ident.ident.to_string()),
                Pat::Path(path) => path
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string()),
                _ => None,
            }
        }

        fn safe_subtractions_from_cmp_arm(
            operands: &(String, String),
            pat: &Pat,
        ) -> Vec<(String, String)> {
            match ordering_pat_name(pat).as_deref() {
                Some("Greater") => vec![(operands.0.clone(), operands.1.clone())],
                Some("Less") => vec![(operands.1.clone(), operands.0.clone())],
                Some("Equal") => vec![
                    (operands.0.clone(), operands.1.clone()),
                    (operands.1.clone(), operands.0.clone()),
                ],
                _ => Vec::new(),
            }
        }

        struct ArithmeticVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
            skip_overloaded_group_arithmetic: bool,
            safe_subtractions: Vec<(String, String)>,
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
                ) && !self.skip_overloaded_group_arithmetic
                    && !max_min_difference_is_nonnegative(expr_binary)
                    && !subtraction_operands(expr_binary)
                        .is_some_and(|operands| self.safe_subtractions.contains(&operands))
                {
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

            fn visit_expr_if(&mut self, expr_if: &'ast ExprIf) {
                let ordered_condition = safe_subtractions_from_condition(&expr_if.cond);

                if let Some((then_pair, _)) = &ordered_condition {
                    self.safe_subtractions.push(then_pair.clone());
                    self.visit_block(&expr_if.then_branch);
                    self.safe_subtractions.pop();
                } else {
                    self.visit_block(&expr_if.then_branch);
                }

                if let Some((_, else_branch)) = &expr_if.else_branch {
                    if let Some((_, else_pair)) = &ordered_condition {
                        self.safe_subtractions.push(else_pair.clone());
                        self.visit_expr(else_branch);
                        self.safe_subtractions.pop();
                    } else {
                        self.visit_expr(else_branch);
                    }
                }
            }

            fn visit_expr_match(&mut self, expr_match: &'ast syn::ExprMatch) {
                let Some(cmp_operands) = cmp_match_operands(&expr_match.expr) else {
                    visit::visit_expr_match(self, expr_match);
                    return;
                };

                self.visit_expr(&expr_match.expr);
                for arm in &expr_match.arms {
                    if let Some((_, guard)) = &arm.guard {
                        self.visit_expr(guard);
                    }
                    let safe_subtractions = safe_subtractions_from_cmp_arm(&cmp_operands, &arm.pat);
                    let previous_len = self.safe_subtractions.len();
                    self.safe_subtractions.extend(safe_subtractions);
                    self.visit_expr(&arm.body);
                    self.safe_subtractions.truncate(previous_len);
                }
            }

            fn visit_expr_call(&mut self, expr_call: &'ast ExprCall) {
                let entering_group_conversion = expr_call_path(expr_call)
                    .and_then(path_last_ident)
                    .is_some_and(|name| name == "from_jacobian");
                let previous = self.skip_overloaded_group_arithmetic;
                if entering_group_conversion {
                    self.skip_overloaded_group_arithmetic = true;
                }
                visit::visit_expr_call(self, expr_call);
                self.skip_overloaded_group_arithmetic = previous;
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
                        skip_overloaded_group_arithmetic: false,
                        safe_subtractions: Vec::new(),
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
                            skip_overloaded_group_arithmetic: false,
                            safe_subtractions: Vec::new(),
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

        let mut seen_lines = std::collections::BTreeSet::new();
        visitor
            .diagnostics
            .retain(|diag| seen_lines.insert(diag.line));

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
            has_fallible_path: bool,
        }

        impl<'ast> Visit<'ast> for HookStorageVisitor {
            fn visit_expr_try(&mut self, expr_try: &'ast syn::ExprTry) {
                self.has_fallible_path = true;
                visit::visit_expr_try(self, expr_try);
            }

            fn visit_expr_return(&mut self, expr_return: &'ast syn::ExprReturn) {
                if expr_return
                    .expr
                    .as_ref()
                    .is_some_and(|expr| expr_contains_named(expr, &["Err"]))
                {
                    self.has_fallible_path = true;
                }
                visit::visit_expr_return(self, expr_return);
            }

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

            fn visit_macro(&mut self, mac: &'ast Macro) {
                if macro_name(mac).as_deref() == Some("ensure") {
                    self.has_fallible_path = true;
                }
                visit::visit_macro(self, mac);
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
                    has_fallible_path: false,
                };
                visitor.visit_block(&item_fn.block);
                if visitor.write_count >= 2
                    && visitor.has_fallible_path
                    && !visitor.has_transactional
                {
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
        let privileged_dispatchables = collect_dispatchables(ast, &test_mask)
            .into_iter()
            .filter(|dispatchable| dispatchable.origin == DispatchableOrigin::Privileged)
            .map(|dispatchable| dispatchable.name)
            .collect::<HashSet<_>>();

        fn block_has_weight_meter_bounded_iteration(block: &syn::Block) -> bool {
            let tokens = compact_tokens(block);
            (tokens.contains("WeightMeter") || tokens.contains(".try_consume("))
                && tokens.contains(".take(")
        }

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
            privileged_dispatchables: &'a HashSet<String>,
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
                        let fn_name = item_fn.sig.ident.to_string();
                        if !self.privileged_dispatchables.contains(&fn_name) {
                            self.inspect_fn(
                                &format!("Dispatchable `{}`", item_fn.sig.ident),
                                item_fn.span(),
                                &item_fn.block,
                            );
                        }
                    }
                    let hook_name = item_fn.sig.ident.to_string();
                    if matches!(
                        hook_name.as_str(),
                        "on_poll" | "on_idle" | "on_initialize" | "on_finalize"
                    ) && !block_has_weight_meter_bounded_iteration(&item_fn.block)
                    {
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
            privileged_dispatchables: &privileged_dispatchables,
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
        if is_frame_support_migration_infrastructure(&ctx.path) {
            return None;
        }
        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;
        let lines = ctx.content.lines().collect::<Vec<_>>();

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
            lines: &'a [&'a str],
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
                    && !nearby_comment_documents_bounded_unbounded_clear(
                        self.lines,
                        span_line(expr_call.span()),
                    )
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

        fn nearby_comment_documents_bounded_unbounded_clear(lines: &[&str], line: usize) -> bool {
            let current_idx = line.saturating_sub(1);
            let start = current_idx.saturating_sub(3);
            lines[start..current_idx.min(lines.len())]
                .iter()
                .filter_map(|source_line| source_line.split_once("//").map(|(_, comment)| comment))
                .any(|comment| {
                    let lower = comment.to_ascii_lowercase();
                    lower.contains("safe to remove unbounded") && lower.contains("at most")
                })
        }

        let mut visitor = ClearPrefixVisitor {
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
        if has_pallet_dev_mode(ast) {
            return None;
        }

        struct StorageCollectionVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
        }

        fn storage_doc_text(item: &ItemType) -> String {
            item.attrs
                .iter()
                .filter(|attr| path_has_exact_ident(attr.path(), "doc"))
                .filter_map(|attr| match &attr.meta {
                    syn::Meta::NameValue(meta) => match &meta.value {
                        Expr::Lit(expr_lit) => match &expr_lit.lit {
                            Lit::Str(lit) => Some(lit.value()),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        }

        fn docs_mark_bounded_storage_collection(item: &ItemType) -> bool {
            let doc = storage_doc_text(item);
            if doc.contains("could become bounded")
                || doc.contains("no global maximum")
                || doc.contains("no global bound")
                || doc.contains("unbounded")
            {
                return false;
            }

            doc.contains("length is limited by")
                || doc.contains("size is limited by")
                || doc.contains("bounded by")
                || doc.contains("limited by the capacity")
        }

        impl<'ast> Visit<'ast> for StorageCollectionVisitor<'_> {
            fn visit_item_type(&mut self, item: &'ast ItemType) {
                if !has_attr(&item.attrs, &["pallet", "storage"])
                    || has_attr(&item.attrs, &["pallet", "unbounded"])
                    || is_masked_span(self.mask, item.span())
                {
                    return;
                }

                if type_contains_unbounded_storage_collection(&item.ty)
                    && !docs_mark_bounded_storage_collection(item)
                {
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

        fn storage_type_args(item: &ItemType) -> Option<(String, Vec<Type>)> {
            let type_path = match &*item.ty {
                Type::Path(type_path) => type_path,
                _ => return None,
            };
            let segment = type_path.path.segments.last()?;
            match &segment.arguments {
                PathArguments::AngleBracketed(args) => Some((
                    segment.ident.to_string(),
                    args.args
                        .iter()
                        .filter_map(|arg| match arg {
                            GenericArgument::Type(ty) => Some(ty.clone()),
                            _ => None,
                        })
                        .collect(),
                )),
                _ => None,
            }
        }

        fn is_common_storage_key_type(ty: &Type) -> bool {
            type_contains_named(ty, &["AccountId", "Balance", "BlockNumber"])
                || type_is_named(ty, &["u32", "u64"])
        }

        fn storage_doc_text(item: &ItemType) -> String {
            item.attrs
                .iter()
                .filter(|attr| path_has_exact_ident(attr.path(), "doc"))
                .filter_map(|attr| match &attr.meta {
                    syn::Meta::NameValue(meta) => match &meta.value {
                        Expr::Lit(expr_lit) => match &expr_lit.lit {
                            Lit::Str(lit) => Some(lit.value()),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        }

        fn docs_mark_internal_numeric_layout(item: &ItemType) -> bool {
            let doc = storage_doc_text(item);
            doc.contains("ring buffer")
                || doc.contains("index")
                || doc.contains("indices")
                || doc.contains("segment")
                || doc.contains("position")
        }

        fn has_identity_common_key_pair(
            item: &ItemType,
            type_args: &[Type],
            hasher_idx: usize,
            key_idx: usize,
        ) -> bool {
            let has_identity = type_args
                .get(hasher_idx)
                .is_some_and(|ty| type_is_named(ty, &["Identity"]));
            let Some(key_ty) = type_args.get(key_idx) else {
                return false;
            };

            has_identity
                && is_common_storage_key_type(key_ty)
                && !(type_is_named(key_ty, &["u32", "u64"])
                    && docs_mark_internal_numeric_layout(item))
        }

        fn storage_uses_identity_on_common_key(
            item: &ItemType,
            storage_kind: &str,
            type_args: &[Type],
        ) -> bool {
            match storage_kind {
                "StorageMap" | "CountedStorageMap" => {
                    has_identity_common_key_pair(item, type_args, 1, 2)
                }
                "StorageDoubleMap" => {
                    has_identity_common_key_pair(item, type_args, 1, 2)
                        || has_identity_common_key_pair(item, type_args, 3, 4)
                }
                _ => false,
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

                let Some((storage_kind, type_args)) = storage_type_args(item) else {
                    return;
                };

                if storage_uses_identity_on_common_key(item, &storage_kind, &type_args) {
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

        fn is_ensure_root_call(expr: &Expr) -> bool {
            match strip_expr_wrappers(expr) {
                Expr::Call(expr_call) => expr_call_path(expr_call)
                    .map(|path| path_has_exact_ident(path, "ensure_root"))
                    .unwrap_or(false),
                _ => false,
            }
        }

        fn expr_is_root_is_ok(expr: &Expr) -> bool {
            let Expr::MethodCall(method_call) = strip_expr_wrappers(expr) else {
                return false;
            };
            method_call.method == "is_ok" && is_ensure_root_call(&method_call.receiver)
        }

        fn expr_mentions_root_flag(expr: &Expr, root_flags: &HashSet<String>) -> bool {
            struct RootFlagVisitor<'a> {
                root_flags: &'a HashSet<String>,
                found: bool,
            }

            impl<'ast> Visit<'ast> for RootFlagVisitor<'_> {
                fn visit_expr_path(&mut self, expr_path: &'ast syn::ExprPath) {
                    if expr_path
                        .path
                        .segments
                        .last()
                        .is_some_and(|segment| self.root_flags.contains(&segment.ident.to_string()))
                    {
                        self.found = true;
                    }
                    visit::visit_expr_path(self, expr_path);
                }
            }

            let mut visitor = RootFlagVisitor {
                root_flags,
                found: false,
            };
            visitor.visit_expr(expr);
            visitor.found
        }

        fn root_flag_names(block: &syn::Block) -> HashSet<String> {
            struct RootFlagCollector {
                names: HashSet<String>,
            }

            impl<'ast> Visit<'ast> for RootFlagCollector {
                fn visit_local(&mut self, local: &'ast Local) {
                    let Pat::Ident(pat_ident) = &local.pat else {
                        visit::visit_local(self, local);
                        return;
                    };
                    if local
                        .init
                        .as_ref()
                        .is_some_and(|init| expr_is_root_is_ok(&init.expr))
                    {
                        self.names.insert(pat_ident.ident.to_string());
                    }
                    visit::visit_local(self, local);
                }
            }

            let mut collector = RootFlagCollector {
                names: HashSet::new(),
            };
            collector.visit_block(block);
            collector.names
        }

        fn strict_root_guard_lines(block: &syn::Block) -> Vec<usize> {
            struct RootGuardCollector {
                lines: Vec<usize>,
            }

            impl<'ast> Visit<'ast> for RootGuardCollector {
                fn visit_expr_try(&mut self, expr_try: &'ast syn::ExprTry) {
                    if is_ensure_root_call(&expr_try.expr) {
                        self.lines.push(span_line(expr_try.span()));
                    }
                    visit::visit_expr_try(self, expr_try);
                }
            }

            let mut collector = RootGuardCollector { lines: Vec::new() };
            collector.visit_block(block);
            collector.lines
        }

        struct DispatchBypassVisitor<'a> {
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            mask: &'a [bool],
            root_flags: HashSet<String>,
            strict_root_guard_lines: Vec<usize>,
            root_context_depth: usize,
        }

        impl DispatchBypassVisitor<'_> {
            fn has_prior_strict_root_guard(&self, span: Span) -> bool {
                let line = span_line(span);
                self.strict_root_guard_lines
                    .iter()
                    .any(|guard_line| *guard_line < line)
            }
        }

        impl<'ast> Visit<'ast> for DispatchBypassVisitor<'_> {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                let previous_root_flags =
                    std::mem::replace(&mut self.root_flags, root_flag_names(&item_fn.block));
                let previous_guard_lines = std::mem::replace(
                    &mut self.strict_root_guard_lines,
                    strict_root_guard_lines(&item_fn.block),
                );
                visit::visit_item_fn(self, item_fn);
                self.root_flags = previous_root_flags;
                self.strict_root_guard_lines = previous_guard_lines;
            }

            fn visit_impl_item_fn(&mut self, item_fn: &'ast syn::ImplItemFn) {
                let previous_root_flags =
                    std::mem::replace(&mut self.root_flags, root_flag_names(&item_fn.block));
                let previous_guard_lines = std::mem::replace(
                    &mut self.strict_root_guard_lines,
                    strict_root_guard_lines(&item_fn.block),
                );
                visit::visit_impl_item_fn(self, item_fn);
                self.root_flags = previous_root_flags;
                self.strict_root_guard_lines = previous_guard_lines;
            }

            fn visit_expr_if(&mut self, expr_if: &'ast ExprIf) {
                visit::visit_expr(self, &expr_if.cond);
                if expr_mentions_root_flag(&expr_if.cond, &self.root_flags)
                    || expr_is_root_is_ok(&expr_if.cond)
                {
                    self.root_context_depth += 1;
                    self.visit_block(&expr_if.then_branch);
                    self.root_context_depth -= 1;
                } else {
                    self.visit_block(&expr_if.then_branch);
                }
                if let Some((_, else_branch)) = &expr_if.else_branch {
                    self.visit_expr(else_branch);
                }
            }

            fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
                if node.method == "dispatch_bypass_filter"
                    && !is_masked_span(self.mask, node.span())
                    && self.root_context_depth == 0
                    && !self.has_prior_strict_root_guard(node.span())
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
						message: "`dispatch_bypass_filter` is used in production code".to_string(),
						explanation: "Bypassing dispatch filters sidesteps normal call filtering and \
                            can accidentally enable privileged or unsafe execution paths."
							.to_string(),
						suggestion: Some("Use normal `dispatch`/`dispatch_as` flow unless bypassing filters is explicitly justified and reviewed".to_string()),
					});
                }
                visit::visit_expr_method_call(self, node);
            }
        }

        let mut visitor = DispatchBypassVisitor {
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
            mask: &test_mask,
            root_flags: HashSet::new(),
            strict_root_guard_lines: Vec::new(),
            root_context_depth: 0,
        };
        visitor.visit_file(ast);
        let diagnostics = visitor.diagnostics;

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
        if is_frame_support_migration_infrastructure(&ctx.path) {
            return None;
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let ast = ast_file(ctx)?;
        let lines = ctx.content.lines().collect::<Vec<_>>();

        fn nearby_comments_mark_idempotent_migration(lines: &[&str], line: usize) -> bool {
            let current_idx = line.saturating_sub(1);
            let start = current_idx.saturating_sub(18);
            lines[start..current_idx.min(lines.len())]
                .iter()
                .filter_map(|source_line| source_line.split_once("//").map(|(_, comment)| comment))
                .any(|comment| {
                    let lower = comment.to_ascii_lowercase();
                    lower.contains("idempotent")
                        || lower.contains("safe to run multiple times")
                        || lower.contains("no-op if")
                        || lower.contains("does not exist yet")
                        || lower.contains("if out of sync")
                        || lower.contains("only updates if")
                })
        }

        struct UpgradeBodyVisitor {
            has_storage_version_check: bool,
            has_write: bool,
            has_non_reconciliation_write: bool,
            reconciliation_values: Vec<HashSet<String>>,
        }

        impl<'ast> Visit<'ast> for UpgradeBodyVisitor {
            fn visit_expr_if(&mut self, node: &'ast ExprIf) {
                self.visit_expr(&node.cond);

                if let Some(values) = reconciliation_condition_values(&node.cond) {
                    self.reconciliation_values.push(values);
                    self.visit_block(&node.then_branch);
                    self.reconciliation_values.pop();
                } else {
                    self.visit_block(&node.then_branch);
                }

                if let Some((_, else_branch)) = &node.else_branch {
                    self.visit_expr(else_branch);
                }
            }

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
                        if !self.write_is_current_value_reconciliation(name.as_str(), node) {
                            self.has_non_reconciliation_write = true;
                        }
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

        impl UpgradeBodyVisitor {
            fn write_is_current_value_reconciliation(
                &self,
                call_name: &str,
                node: &ExprCall,
            ) -> bool {
                if !matches!(call_name, "put" | "set") {
                    return false;
                }

                let Some(value) = node.args.first().map(compact_tokens) else {
                    return false;
                };

                self.reconciliation_values
                    .last()
                    .is_some_and(|values| values.contains(&value))
            }
        }

        fn reconciliation_condition_values(expr: &Expr) -> Option<HashSet<String>> {
            let Expr::Binary(binary) = strip_expr_wrappers(expr) else {
                return None;
            };
            if !matches!(binary.op, syn::BinOp::Ne(_)) {
                return None;
            }

            let left = compact_tokens(strip_expr_wrappers(&binary.left));
            let right = compact_tokens(strip_expr_wrappers(&binary.right));
            if left.is_empty() || right.is_empty() {
                return None;
            }

            Some(HashSet::from([left, right]))
        }

        struct UpgradeVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            lines: &'a [&'a str],
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
                    has_non_reconciliation_write: false,
                    reconciliation_values: Vec::new(),
                };
                body_visitor.visit_block(&item_fn.block);

                if body_visitor.has_write
                    && body_visitor.has_non_reconciliation_write
                    && !body_visitor.has_storage_version_check
                    && !nearby_comments_mark_idempotent_migration(
                        self.lines,
                        span_line(item_fn.sig.ident.span()),
                    )
                {
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

            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                if item_impl
                    .trait_
                    .as_ref()
                    .map(|(_, path, _)| path_has_segment(path, "UncheckedOnRuntimeUpgrade"))
                    .unwrap_or(false)
                {
                    return;
                }

                for impl_item in &item_impl.items {
                    let ImplItem::Fn(item_fn) = impl_item else {
                        continue;
                    };
                    let wrapper = ItemFn {
                        attrs: item_fn.attrs.clone(),
                        vis: item_fn.vis.clone(),
                        sig: item_fn.sig.clone(),
                        block: Box::new(item_fn.block.clone()),
                    };
                    self.inspect_fn(&wrapper);
                }

                visit::visit_item_impl(self, item_impl);
            }
        }

        let mut visitor = UpgradeVisitor {
            mask: &test_mask,
            diagnostics: Vec::new(),
            file: &ctx.path,
            severity: config.rule_severity(self.id(), Severity::Warning),
            rule_id: self.id(),
            rule_name: self.name(),
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
        let compact_content = ctx
            .content
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();

        fn named_vec_event_fields(item: &ItemEnum) -> Vec<(String, String)> {
            item.variants
                .iter()
                .flat_map(|variant| match &variant.fields {
                    syn::Fields::Named(fields) => fields
                        .named
                        .iter()
                        .filter(|field| type_contains_named(&field.ty, &["Vec"]))
                        .filter_map(|field| {
                            field
                                .ident
                                .as_ref()
                                .map(|ident| (variant.ident.to_string(), ident.to_string()))
                        })
                        .collect::<Vec<_>>(),
                    syn::Fields::Unnamed(_) | syn::Fields::Unit => Vec::new(),
                })
                .collect()
        }

        fn bounded_vec_event_flow(content: &str, variant_name: &str, field_name: &str) -> bool {
            content.contains(&format!("{field_name}:BoundedVec<"))
                && content.contains(&format!("{field_name}.into()"))
                && content.contains(&format!("Event::{}{{", variant_name))
        }

        struct EventVisitor<'a> {
            mask: &'a [bool],
            diagnostics: Vec<Diagnostic>,
            file: &'a Path,
            severity: Severity,
            rule_id: &'a str,
            rule_name: &'a str,
            compact_content: &'a str,
        }

        impl<'ast> Visit<'ast> for EventVisitor<'_> {
            fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
                if !has_attr(&item.attrs, &["pallet", "event"])
                    || is_masked_span(self.mask, item.span())
                {
                    return;
                }

                let named_vec_fields = named_vec_event_fields(item);
                let has_unnamed_vec_field = item.variants.iter().any(|variant| {
                    matches!(
                        &variant.fields,
                        syn::Fields::Unnamed(fields)
                            if fields.unnamed.iter().any(|field| type_contains_named(&field.ty, &["Vec"]))
                    )
                });
                let all_named_vec_fields_are_bounded_flows = !named_vec_fields.is_empty()
                    && named_vec_fields.iter().all(|(variant_name, field_name)| {
                        bounded_vec_event_flow(self.compact_content, variant_name, field_name)
                    });
                let has_vec_field = has_unnamed_vec_field || !named_vec_fields.is_empty();

                if has_vec_field
                    && (has_unnamed_vec_field || !all_named_vec_fields_are_bounded_flows)
                {
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
            compact_content: &compact_content,
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
// SEC018: Weight attributes that ignore unbounded input lengths
// ---------------------------------------------------------------------------

pub struct MissingWeightForUnboundedInput;

impl LintRule for MissingWeightForUnboundedInput {
    fn id(&self) -> &str {
        "SEC018"
    }
    fn name(&self) -> &str {
        "missing-weight-for-unbounded-input"
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
        let mut diagnostics = Vec::new();
        for dispatchable in collect_dispatchables(ast, &test_mask) {
            if dispatchable.is_deprecated
                || dispatchable.has_max_weight()
                || dispatchable.consumes_max_block
            {
                continue;
            }

            for param in dispatchable.unbounded_vec_params() {
                if param.name.starts_with('_') || param.is_bounded_in_body {
                    continue;
                }

                if dispatchable.weight_accounts_for_param(&param.name) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    category: RuleCategory::Semantic,
                    severity: config.rule_severity(self.id(), Severity::Warning),
                    file: ctx.path.clone(),
                    line: span_line(dispatchable.span),
                    column: Some(span_column(dispatchable.span)),
                    end_line: None,
                    message: format!(
                        "Extrinsic `{}` accepts unbounded `{}` but its weight does not account for its length",
                        dispatchable.name, param.name
                    ),
                    explanation: "SCALE decoding and pre-dispatch processing happen before \
                        the call body runs. If an unbounded input length is absent from \
                        `#[pallet::weight(...)]`, callers can submit larger payloads while \
                        paying only the fixed base weight."
                        .to_string(),
                    suggestion: Some(format!(
                        "Pass `{}.len() as u32` or an equivalent encoded-size parameter into the benchmarked `WeightInfo` call",
                        param.name
                    )),
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
