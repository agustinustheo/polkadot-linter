use std::{fs, path::Path};

use proc_macro2::Span;
use syn::{
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, Expr, ImplItem, ItemFn, Lit, Meta, Stmt, Token, TraitItemFn, Type,
};

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    frame_model::{active_cfg_attr_args, active_cfg_attr_path_matches},
    project_model::SourceTargetKind,
    rules::{runtime_config::cfg_matches, LintRule},
    syn_util::{
        ast_file, attr_expr, compact_tokens, expr_call_path, is_masked_span, path_last_ident,
        privileged_origin, span_column, span_line,
    },
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
        || is_pure_test_path(&ctx.classification_path)
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
        || trimmed.starts_with("const fn ")
        || trimmed.starts_with("unsafe fn ")
        || trimmed.starts_with("pub unsafe fn ")
        || trimmed.starts_with("pub(crate) fn ")
        || trimmed.starts_with("pub(crate) async fn ")
        || trimmed.starts_with("pub(super) fn ")
        || trimmed.starts_with("pub(super) async fn ")
        || trimmed.starts_with("pub(in ") && trimmed.contains(" fn ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("pub struct ")
        || trimmed.starts_with("pub(crate) struct ")
        || trimmed.starts_with("pub(super) struct ")
        || trimmed.starts_with("pub enum ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("union ")
        || trimmed.starts_with("pub union ")
        || trimmed.starts_with("pub(crate) union ")
        || trimmed.starts_with("pub(super) union ")
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

    cfg_condition(trimmed).is_some_and(|condition| cfg_is_non_production(&condition))
}

fn cfg_condition(attribute: &str) -> Option<Meta> {
    let condition = attribute
        .strip_prefix("#[cfg(")
        .or_else(|| attribute.strip_prefix("#![cfg("))?
        .strip_suffix(")]")?;
    syn::parse_str(condition).ok()
}

fn cfg_is_non_production(condition: &Meta) -> bool {
    const NON_PRODUCTION_FEATURES: &[&str] = &[
        "runtime-benchmarks",
        "try-runtime",
        "test-helpers",
        "remote-test",
        "integrity-test",
    ];

    let is_non_production_atom = |meta: &Meta| match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::NameValue(name_value) => {
            name_value.path.is_ident("feature")
                && matches!(
                    &name_value.value,
                    Expr::Lit(expr_lit)
                        if matches!(
                            &expr_lit.lit,
                            Lit::Str(feature) if NON_PRODUCTION_FEATURES.contains(&feature.value().as_str())
                        )
                )
        }
        Meta::List(_) => false,
    };

    if is_non_production_atom(condition) {
        return true;
    }

    let Meta::List(list) = condition else {
        return false;
    };
    let Ok(conditions) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
    else {
        return false;
    };

    if list.path.is_ident("all") {
        // A conjunction is non-production when at least one required clause is.
        conditions.iter().any(cfg_is_non_production)
    } else if list.path.is_ident("any") {
        // Every alternative must be non-production before the whole condition is.
        conditions.iter().all(cfg_is_non_production)
    } else {
        // `not(...)` and unknown predicates can enable production code.
        false
    }
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

        let masking_attr = is_masking_attr(trimmed) && masked_depth.is_none();
        if masking_attr {
            mask[i] = true;
            // Attributes can annotate a complete item on the same line, e.g.
            // `#[cfg(test)] use crate::mock;`. In that case there is no later
            // item to mask.
            next_item_is_masked = trimmed
                .find(']')
                .is_none_or(|end| trimmed[end + 1..].trim().is_empty());
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

fn merge_masks(left: &[bool], right: &[bool]) -> Vec<bool> {
    let len = left.len().max(right.len());
    (0..len)
        .map(|idx| {
            left.get(idx).copied().unwrap_or(false) || right.get(idx).copied().unwrap_or(false)
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct SourceSanitizer {
    state: SourceSanitizerState,
}

#[derive(Default)]
enum SourceSanitizerState {
    #[default]
    Code,
    BlockComment {
        depth: usize,
    },
    String {
        escaped: bool,
    },
    RawString {
        hashes: usize,
    },
}

impl SourceSanitizer {
    pub(crate) fn strip_line(&mut self, line: &str) -> String {
        let chars = line.chars().collect::<Vec<_>>();
        let mut stripped = String::with_capacity(line.len());
        let mut index = 0;

        while index < chars.len() {
            match &mut self.state {
                SourceSanitizerState::Code => {
                    if chars[index] == '/' && chars.get(index + 1) == Some(&'/') {
                        stripped.extend(std::iter::repeat_n(' ', chars.len() - index));
                        break;
                    }
                    if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                        self.state = SourceSanitizerState::BlockComment { depth: 1 };
                        stripped.push_str("  ");
                        index += 2;
                        continue;
                    }
                    if let Some((length, hashes)) = raw_string_start(&chars, index) {
                        self.state = SourceSanitizerState::RawString { hashes };
                        stripped.extend(std::iter::repeat_n(' ', length));
                        index += length;
                        continue;
                    }
                    if chars[index] == '"' {
                        self.state = SourceSanitizerState::String { escaped: false };
                        stripped.push(' ');
                        index += 1;
                        continue;
                    }
                    if chars[index] == 'b' && chars.get(index + 1) == Some(&'"') {
                        self.state = SourceSanitizerState::String { escaped: false };
                        stripped.push_str("  ");
                        index += 2;
                        continue;
                    }
                    if let Some(length) = char_literal_length(&chars, index) {
                        stripped.extend(std::iter::repeat_n(' ', length));
                        index += length;
                        continue;
                    }
                    stripped.push(chars[index]);
                    index += 1;
                }
                SourceSanitizerState::BlockComment { depth } => {
                    if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                        *depth += 1;
                        stripped.push_str("  ");
                        index += 2;
                    } else if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                        *depth -= 1;
                        stripped.push_str("  ");
                        index += 2;
                        if *depth == 0 {
                            self.state = SourceSanitizerState::Code;
                        }
                    } else {
                        stripped.push(' ');
                        index += 1;
                    }
                }
                SourceSanitizerState::String { escaped } => {
                    let character = chars[index];
                    stripped.push(' ');
                    index += 1;
                    if *escaped {
                        *escaped = false;
                    } else if character == '\\' {
                        *escaped = true;
                    } else if character == '"' {
                        self.state = SourceSanitizerState::Code;
                    }
                }
                SourceSanitizerState::RawString { hashes } => {
                    if chars[index] == '"' && chars[index + 1..].starts_with(&vec!['#'; *hashes]) {
                        stripped.extend(std::iter::repeat_n(' ', hashes.saturating_add(1)));
                        index += hashes.saturating_add(1);
                        self.state = SourceSanitizerState::Code;
                    } else {
                        stripped.push(' ');
                        index += 1;
                    }
                }
            }
        }

        stripped
    }
}

pub(crate) fn strip_strings_and_line_comments(line: &str) -> String {
    SourceSanitizer::default().strip_line(line)
}

fn raw_string_start(chars: &[char], index: usize) -> Option<(usize, usize)> {
    let mut cursor = index;
    if chars.get(cursor) == Some(&'b') {
        cursor += 1;
    }
    if chars.get(cursor) != Some(&'r') {
        return None;
    }
    cursor += 1;
    let hashes_start = cursor;
    while chars.get(cursor) == Some(&'#') {
        cursor += 1;
    }
    (chars.get(cursor) == Some(&'"')).then_some((cursor - index + 1, cursor - hashes_start))
}

fn char_literal_length(chars: &[char], index: usize) -> Option<usize> {
    if chars.get(index) != Some(&'\'') {
        return None;
    }
    let mut cursor = index + 1;
    if chars.get(cursor) == Some(&'\\') {
        cursor += 1;
        if chars.get(cursor) == Some(&'u') && chars.get(cursor + 1) == Some(&'{') {
            cursor += 2;
            while chars.get(cursor).is_some_and(|character| *character != '}') {
                cursor += 1;
            }
            cursor += 1;
        } else {
            cursor += 1;
        }
    } else {
        cursor += 1;
    }
    (chars.get(cursor) == Some(&'\'')).then_some(cursor - index + 1)
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
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    attr_segments
        .iter()
        .map(String::as_str)
        .eq(segments.iter().copied())
        || (segments.len() > 1
            && attr_segments
                .iter()
                .map(String::as_str)
                .rev()
                .zip(segments.iter().rev().copied())
                .all(|(actual, expected)| actual == expected)
            && attr_segments.len() >= segments.len())
        || active_cfg_attr_path_matches(attr, segments)
}

fn attr_expr_for_path(attr: &Attribute, segments: &[&str]) -> Option<Expr> {
    attr_expr(attr).or_else(|| {
        active_cfg_attr_args(attr, segments).and_then(|arguments| syn::parse2(arguments).ok())
    })
}

// ---------------------------------------------------------------------------
// SEM011: Weight::zero() placeholder in weight attributes
// ---------------------------------------------------------------------------

/// A root-only call compiled only for test networks: a `#[cfg(..)]` that
/// requires a feature whose name contains `testnet`, and a body whose origin
/// check is `ensure_root(origin)?;`. Its zero weight is reachable only by the
/// sudo key of a test chain.
fn testnet_root_only(attrs: &[Attribute], block: &syn::Block) -> bool {
    let testnet_feature = |meta: &Meta| {
        matches!(meta, Meta::NameValue(nv) if nv.path.is_ident("feature")
            && matches!(&nv.value, Expr::Lit(lit)
                if matches!(&lit.lit, Lit::Str(s) if s.value().contains("testnet"))))
    };
    let gated = attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<Meta>()
                .is_ok_and(|meta| cfg_matches(&meta, &testnet_feature))
    });
    let root_check = block.stmts.iter().any(|stmt| {
        matches!(stmt, Stmt::Expr(Expr::Try(t), Some(_))
            if matches!(&*t.expr, Expr::Call(call)
                if expr_call_path(call).and_then(path_last_ident).as_deref() == Some("ensure_root")))
    });
    gated && root_check && privileged_origin(block)
}

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
            fn inspect_attrs(&mut self, attrs: &[Attribute], block: &syn::Block) {
                if testnet_root_only(attrs, block) {
                    return;
                }
                for attr in attrs.iter().filter(|attr| is_weight_attr(attr)) {
                    let Some(expr) = attr_expr_for_path(attr, &["pallet", "weight"])
                        .or_else(|| attr_expr_for_path(attr, &["pallet", "weight_of_authorize"]))
                    else {
                        continue;
                    };
                    let is_zero_lit = |expr: &Expr| matches!(expr, Expr::Lit(lit) if matches!(&lit.lit, Lit::Int(int) if !integer_literal_is_nonzero(int)));
                    // `0` (a `u64` ref-time weight) or `Weight::from_parts(0, 0)`.
                    let zero = is_zero_lit(&expr)
                        || matches!(&expr, Expr::Call(expr_call) if expr_call_path(expr_call).is_some_and(|path| {
                            path_has_segment(path, "Weight")
                                && (path_has_exact_ident(path, "zero")
                                    || (["from_parts", "from_all", "from_ref_time"]
                                        .iter()
                                        .any(|ctor| path_has_exact_ident(path, ctor))
                                        && expr_call.args.iter().all(is_zero_lit)))
                        }));
                    if zero {
                        self.diagnostics.push(Diagnostic {
							rule_id: self.rule_id.to_string(),
							rule_name: self.rule_name.to_string(),
							category: RuleCategory::Semantic,
							severity: self.severity,
							file: self.file.to_path_buf(),
							line: span_line(attr.span()),
							column: Some(span_column(attr.span())),
							end_line: None,
							message: format!("zero weight `{}` in weight attribute", compact_tokens(&expr)),
							explanation: "A zero weight (`Weight::zero()`, `0`, \
                                `Weight::from_parts(0, 0)`) means the extrinsic is free, which is \
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
                self.inspect_attrs(&item_fn.attrs, &item_fn.block);
                visit::visit_item_fn(self, item_fn);
            }

            fn visit_impl_item(&mut self, item: &'ast ImplItem) {
                if let ImplItem::Fn(item_fn) = item {
                    self.inspect_attrs(&item_fn.attrs, &item_fn.block);
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
// SEM019: verifier trait method whose default body accepts everything
// ---------------------------------------------------------------------------

pub struct VerifierDefaultsToTrue;

impl LintRule for VerifierDefaultsToTrue {
    fn id(&self) -> &str {
        "SEM019"
    }
    fn name(&self) -> &str {
        "verifier-defaults-to-true"
    }
    fn family(&self) -> &str {
        "semantic"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if should_skip_production_rule(ctx) {
            return None;
        }

        /// `fn ownership_proof_is_valid(&self, _proof: &[u8]) -> bool { true }`:
        /// a verifier-named default that takes something to check and
        /// returns `true` without reading it (polkadot-sdk #1739).
        fn defaults_to_true(item: &TraitItemFn) -> bool {
            let name = item.sig.ident.to_string();
            let words = name.split('_').collect::<Vec<_>>();
            let has = |word| words.contains(&word);
            let verifier = ["verify", "verified", "valid", "authorized", "authorised"]
                .into_iter()
                .any(has)
                || (has("ownership") && has("proof"));
            // Negated or inverted names (`proof_is_invalid`, `needs_proof`)
            // return `true` to reject, not to accept.
            let inverted = [
                "invalid",
                "unauthorized",
                "unauthorised",
                "validator",
                "not",
                "no",
            ]
            .into_iter()
            .any(has)
                || words.iter().any(|word| {
                    ["deny", "reject", "needs", "requires"]
                        .iter()
                        .any(|prefix| word.starts_with(prefix))
                });
            let returns_bool = matches!(&item.sig.output, syn::ReturnType::Type(_, ty)
                if matches!(&**ty, Type::Path(path) if path.path.is_ident("bool")));
            let takes_input = item
                .sig
                .inputs
                .iter()
                .any(|input| matches!(input, syn::FnArg::Typed(_)));
            let accepts_all = item.default.as_ref().is_some_and(|body| {
                matches!(body.stmts.as_slice(), [Stmt::Expr(Expr::Lit(lit), None)]
                    if matches!(lit.lit, Lit::Bool(ref value) if value.value))
            });
            verifier && !inverted && returns_bool && takes_input && accepts_all
        }

        struct Visitor<'a> {
            mask: &'a [bool],
            lines: Vec<(usize, String)>,
        }
        impl<'ast> Visit<'ast> for Visitor<'_> {
            fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
                if !is_masked_span(self.mask, item.span()) && defaults_to_true(item) {
                    self.lines
                        .push((span_line(item.sig.ident.span()), item.sig.ident.to_string()));
                }
            }
        }

        let test_mask = cfg_test_module_mask(ctx.content);
        let mut visitor = Visitor {
            mask: &test_mask,
            lines: Vec::new(),
        };
        visitor.visit_file(ast_file(ctx)?);
        let severity = config.rule_severity(self.id(), Severity::Warning);
        let diagnostics = visitor
            .lines
            .into_iter()
            .map(|(line, name)| Diagnostic {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                category: RuleCategory::Semantic,
                severity,
                file: ctx.path.clone(),
                line,
                column: None,
                end_line: None,
                message: format!(
                    "trait method `{name}` defaults to `true` without inspecting its arguments"
                ),
                explanation: "A verification hook whose default body is `true` accepts every \
                    input for each implementer that does not override it. `OpaqueKeys::\
                    ownership_proof_is_valid` defaulted to `true`, so session keys were set \
                    without a proof of ownership until polkadot-sdk #1739 made the method \
                    required."
                    .to_string(),
                suggestion: Some(
                    "Remove the default so every implementer must decide, or default to \
                     `false` (fail closed)."
                        .to_string(),
                ),
            })
            .collect::<Vec<_>>();
        (!diagnostics.is_empty()).then_some(diagnostics)
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
