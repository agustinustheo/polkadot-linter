use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    rc::Rc,
};

use proc_macro2::Span;
use quote::ToTokens;
use syn::{
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, Expr, ExprCall, ExprForLoop, ExprIf, ExprMethodCall, File as SynFile, ImplItem,
    Item, ItemEnum, ItemFn, Lit, Local, Macro, Meta, Pat, Stmt, Token, TraitItemFn, Type, UseTree,
    Visibility,
};

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::{crate_root, scan_cached, FileContext},
    frame_model::{active_cfg_attr_args, active_cfg_attr_path_matches},
    project_model::SourceTargetKind,
    rules::LintRule,
    syn_util::{
        ast_file, attr_expr, compact_tokens, expr_call_path, is_masked_span, macro_name,
        path_last_ident, span_column, span_line, strip_expr_wrappers, type_contains_named,
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

fn attr_expr_for_path(attr: &Attribute, segments: &[&str]) -> Option<Expr> {
    attr_expr(attr).or_else(|| {
        active_cfg_attr_args(attr, segments).and_then(|arguments| syn::parse2(arguments).ok())
    })
}

fn use_tree_has_disallowed_glob(tree: &UseTree) -> bool {
    fn walk(tree: &UseTree, allow_glob: bool) -> bool {
        match tree {
            UseTree::Glob(_) => !allow_glob,
            UseTree::Group(group) => group.items.iter().any(|item| walk(item, allow_glob)),
            UseTree::Name(_) | UseTree::Rename(_) => false,
            // `use crate::*;` is documented as allowed, also as a member of a
            // `crate::{..}` group, which imports the same items. Globs of crate
            // submodules (`use crate::module::*;`) still hide provenance.
            UseTree::Path(path) if path.ident == "crate" => match &*path.tree {
                UseTree::Glob(_) => false,
                UseTree::Group(group) => group
                    .items
                    .iter()
                    .any(|item| !matches!(item, UseTree::Glob(_)) && walk(item, allow_glob)),
                tree => walk(tree, allow_glob),
            },
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
        let ast = ast_file(ctx);
        let validation_guard_tokens = ast
            .map(|ast| val001_ensure_guard_tokens(ast, lines.len()))
            .unwrap_or_else(|| vec![String::new(); lines.len()]);
        let heavy_read_bindings = ast
            .map(|ast| val001_heavy_read_bindings(ast, lines.len()))
            .unwrap_or_else(|| vec![Vec::new(); lines.len()]);
        let conditional_read_mask = ast
            .map(|ast| val001_if_condition_mask(ast, lines.len()))
            .unwrap_or_else(|| vec![false; lines.len()]);
        let diagnostic_macro_mask = ast
            .map(|ast| val001_diagnostic_macro_mask(ast, lines.len()))
            .unwrap_or_else(|| vec![false; lines.len()]);
        let heavy_operations = &config.validation_order.heavy_operations;
        let helpers = Val001Helpers::for_file(ctx, ast, heavy_operations);
        let guard_costs = ast
            .map(|ast| val001_guard_costs(ast, lines.len(), heavy_operations, &helpers))
            .unwrap_or_else(|| vec![Val001GuardCost::default(); lines.len()]);
        let functions = ast
            .map(|ast| val001_function_facts(ast, heavy_operations, &helpers))
            .unwrap_or_default();
        // Validation order is a production-dispatch concern. Inline test and
        // try-runtime items share source files with production code, so the
        // file-level test check above is not sufficient here.
        let non_production_mask = item_mask_by_attr(ctx.content, is_masked_cfg_attribute);

        // Track function boundaries
        let mut in_fn = false;
        let mut brace_depth: i32 = 0;
        let mut function_body_started = false;
        let mut first_heavy_op: Option<(usize, String)> = None;
        let mut heavy_op_bindings: Option<Vec<String>> = None;

        let mut sanitizer = SourceSanitizer::default();
        for (i, line) in lines.iter().enumerate() {
            if non_production_mask.get(i).copied().unwrap_or(false) {
                continue;
            }
            let trimmed = line.trim();
            let code = sanitizer.strip_line(trimmed);

            // Skip comments and attributes
            if trimmed.starts_with("//") || trimmed.starts_with("#[") || trimmed.starts_with("///")
            {
                continue;
            }

            // Detect function start
            if !in_fn
                && (trimmed.starts_with("fn ") || trimmed.contains(" fn "))
                && trimmed.contains('(')
            {
                in_fn = true;
                first_heavy_op = None;
                heavy_op_bindings = None;
                brace_depth = 0;
                function_body_started = false;
            }

            if in_fn {
                let opens = line.chars().filter(|&c| c == '{').count() as i32;
                let closes = line.chars().filter(|&c| c == '}').count() as i32;
                function_body_started |= opens > 0;
                brace_depth += opens;
                brace_depth -= closes;

                // Check for heavy operations
                if first_heavy_op.is_none() {
                    for pattern in &config.validation_order.heavy_operations {
                        if validation_guard_tokens.get(i).is_none_or(String::is_empty)
                            && val001_has_heavy_operation(&code, pattern)
                        {
                            if conditional_read_mask.get(i).copied().unwrap_or(false)
                                || diagnostic_macro_mask.get(i).copied().unwrap_or(false)
                            {
                                break;
                            }
                            // Track every local introduced by the read so destructuring patterns
                            // such as `let (owner, _) = Storage::get(...)` remain data-dependent.
                            heavy_op_bindings = Some(
                                heavy_read_bindings
                                    .get(i)
                                    .cloned()
                                    .filter(|bindings| !bindings.is_empty())
                                    .unwrap_or_else(|| {
                                        let lhs =
                                            code.split('=').next().unwrap_or("").trim().to_string();
                                        val001_binding_names(&lhs)
                                    }),
                            );
                            first_heavy_op = Some((i, pattern.clone()));
                            break;
                        }
                    }
                }

                // If we already saw a heavy op, check for cheap validations after it
                if let Some((heavy_line, ref heavy_pattern)) = first_heavy_op {
                    // A name consumes the read only while it still holds the read value.
                    let function = val001_function_at(&functions, heavy_line);
                    let holds_read = |binding: &str| {
                        function.is_none_or(|function| function.holds_read(binding, heavy_line, i))
                    };
                    let mut consumed_heavy_read = false;
                    for pattern in &config.validation_order.cheap_validations {
                        if code.contains(pattern.as_str()) && i > heavy_line {
                            // A predicate by itself is not a validation. It becomes one when a
                            // fallible FRAME guard consumes it; otherwise post-operation checks
                            // such as `if !remaining.is_zero()` are not reorderable
                            // preconditions.
                            if !is_val001_predicate_validation(pattern, &code) {
                                continue;
                            }
                            // FIX #5: Skip if the validation references the variable
                            // assigned from the heavy read (data-dependent validation).
                            if let Some(ref bindings) = heavy_op_bindings {
                                let guard_tokens = validation_guard_tokens
                                    .get(i)
                                    .map(String::as_str)
                                    .unwrap_or("");
                                if bindings.iter().any(|binding| {
                                    (val001_references_binding(&code, binding)
                                        || val001_references_binding(guard_tokens, binding))
                                        && holds_read(binding)
                                }) {
                                    consumed_heavy_read = true;
                                    break;
                                }
                            }

                            // A guard that reads storage or calls into pallet/config code is
                            // not a cheap precondition, and one that consumes storage-derived
                            // or later-computed values, or sits in a deferred closure or a
                            // sibling branch, cannot be hoisted above the read. Later guards
                            // in the same window are still considered.
                            let cost = guard_costs.get(i).cloned().unwrap_or_default();
                            let guard_text = format!(
                                "{code} {}",
                                validation_guard_tokens
                                    .get(i)
                                    .map(String::as_str)
                                    .unwrap_or("")
                            );
                            if cost.reads_storage
                                || cost.opaque_call
                                || function.is_some_and(|function| {
                                    function.guard_is_not_hoistable(heavy_line, i, &guard_text)
                                })
                            {
                                break;
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
                            heavy_op_bindings = None;
                            break;
                        }
                    }

                    // Once an intervening statement consumes the read result, a later guard is
                    // part of the state-dependent control flow rather than an independent
                    // precondition that can be moved before the read.
                    if !consumed_heavy_read {
                        if let Some(ref bindings) = heavy_op_bindings {
                            consumed_heavy_read = i > heavy_line
                                && bindings.iter().any(|binding| {
                                    val001_references_binding(&code, binding) && holds_read(binding)
                                });
                        }
                    }
                    if consumed_heavy_read {
                        first_heavy_op = None;
                        heavy_op_bindings = None;
                    }
                }

                // Function end
                if function_body_started && brace_depth <= 0 {
                    in_fn = false;
                    first_heavy_op = None;
                    heavy_op_bindings = None;
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

fn val001_has_heavy_operation(code: &str, pattern: &str) -> bool {
    code.contains(pattern)
        // `T::Max*::get()` is a type-level `Get` constant, not a storage read.
        && !(pattern == "::get(" && code.contains("T::"))
}

fn is_val001_predicate_validation(pattern: &str, line: &str) -> bool {
    !pattern.starts_with('.') || line.contains("ensure!") || line.contains("ensure_none")
}

fn val001_binding_names(lhs: &str) -> Vec<String> {
    let pattern = lhs
        .trim()
        .strip_prefix("let ")
        .unwrap_or(lhs)
        .split(':')
        .next()
        .unwrap_or(lhs);

    pattern
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|name| !name.is_empty() && !matches!(*name, "let" | "mut" | "ref" | "_" | "else"))
        .map(str::to_string)
        .collect()
}

fn val001_heavy_read_bindings(ast: &SynFile, line_count: usize) -> Vec<Vec<String>> {
    struct BindingVisitor {
        bindings_by_line: Vec<Vec<String>>,
    }

    impl<'ast> Visit<'ast> for BindingVisitor {
        fn visit_local(&mut self, local: &'ast Local) {
            if let Some(init) = &local.init {
                let bindings = val001_pat_binding_names(&local.pat);
                if !bindings.is_empty() {
                    let start = span_line(init.expr.span()).saturating_sub(1);
                    let end = init.expr.span().end().line.saturating_sub(1);
                    for line in start..=end.min(self.bindings_by_line.len().saturating_sub(1)) {
                        self.bindings_by_line[line] = bindings.clone();
                    }
                }
            }
            visit::visit_local(self, local);
        }
    }

    let mut visitor = BindingVisitor {
        bindings_by_line: vec![Vec::new(); line_count],
    };
    visitor.visit_file(ast);
    visitor.bindings_by_line
}

fn val001_if_condition_mask(ast: &SynFile, line_count: usize) -> Vec<bool> {
    struct ConditionVisitor {
        mask: Vec<bool>,
    }

    impl<'ast> Visit<'ast> for ConditionVisitor {
        fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
            let start = span_line(expression.cond.span()).saturating_sub(1);
            let end = expression.cond.span().end().line.saturating_sub(1);
            for line in start..=end.min(self.mask.len().saturating_sub(1)) {
                self.mask[line] = true;
            }
            visit::visit_expr_if(self, expression);
        }
    }

    let mut visitor = ConditionVisitor {
        mask: vec![false; line_count],
    };
    visitor.visit_file(ast);
    visitor.mask
}

fn val001_pat_binding_names(pat: &Pat) -> Vec<String> {
    fn collect(pat: &Pat, bindings: &mut Vec<String>) {
        match pat {
            Pat::Ident(ident) => bindings.push(ident.ident.to_string()),
            Pat::Or(or) => {
                for case in &or.cases {
                    collect(case, bindings);
                }
            }
            Pat::Paren(paren) => collect(&paren.pat, bindings),
            Pat::Reference(reference) => collect(&reference.pat, bindings),
            Pat::Slice(slice) => {
                for element in &slice.elems {
                    collect(element, bindings);
                }
            }
            Pat::Struct(structure) => {
                for field in &structure.fields {
                    collect(&field.pat, bindings);
                }
            }
            Pat::Tuple(tuple) => {
                for element in &tuple.elems {
                    collect(element, bindings);
                }
            }
            Pat::TupleStruct(tuple_struct) => {
                for element in &tuple_struct.elems {
                    collect(element, bindings);
                }
            }
            Pat::Type(typed) => collect(&typed.pat, bindings),
            _ => {}
        }
    }

    let mut bindings = Vec::new();
    collect(pat, &mut bindings);
    bindings
}

fn val001_references_binding(source: &str, binding: &str) -> bool {
    source
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .any(|token| token == binding)
}

fn val001_ensure_guard_tokens(ast: &SynFile, line_count: usize) -> Vec<String> {
    struct EnsureGuardVisitor {
        tokens_by_line: Vec<String>,
    }

    impl<'ast> Visit<'ast> for EnsureGuardVisitor {
        fn visit_macro(&mut self, mac: &'ast Macro) {
            if macro_name(mac).as_deref() == Some("ensure") {
                let start = span_line(mac.span()).saturating_sub(1);
                let end = mac.span().end().line.saturating_sub(1);
                for line in start..=end.min(self.tokens_by_line.len().saturating_sub(1)) {
                    self.tokens_by_line[line] = mac.tokens.to_string();
                }
            }
            visit::visit_macro(self, mac);
        }
    }

    let mut visitor = EnsureGuardVisitor {
        tokens_by_line: vec![String::new(); line_count],
    };
    visitor.visit_file(ast);
    visitor.tokens_by_line
}

/// What a cheap guard's condition does beyond comparing locals: a guard that
/// reads storage itself, or calls into pallet/config code whose cost the
/// linter cannot see, is not a cheap precondition.
#[derive(Clone, Default)]
struct Val001GuardCost {
    reads_storage: bool,
    opaque_call: bool,
}

fn val001_guard_costs(
    ast: &SynFile,
    line_count: usize,
    heavy_operations: &[String],
    helpers: &Val001Helpers,
) -> Vec<Val001GuardCost> {
    struct GuardCostVisitor<'a> {
        costs: Vec<Val001GuardCost>,
        heavy_operations: &'a [String],
        helpers: &'a Val001Helpers,
    }

    impl<'ast> Visit<'ast> for GuardCostVisitor<'_> {
        fn visit_macro(&mut self, mac: &'ast Macro) {
            if macro_name(mac).as_deref() == Some("ensure") {
                let condition = mac
                    .parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
                    .ok()
                    .and_then(|arguments| arguments.into_iter().next());
                if let Some(condition) = condition {
                    let scan =
                        Val001CallScan::scan(&condition, self.heavy_operations, Some(self.helpers));
                    let start = span_line(mac.span()).saturating_sub(1);
                    let end = mac.span().end().line.saturating_sub(1);
                    for line in start..=end.min(self.costs.len().saturating_sub(1)) {
                        self.costs[line] = Val001GuardCost {
                            reads_storage: scan.heavy,
                            opaque_call: scan.opaque,
                        };
                    }
                }
            }
            visit::visit_macro(self, mac);
        }
    }

    let mut visitor = GuardCostVisitor {
        costs: vec![Val001GuardCost::default(); line_count],
        heavy_operations,
        helpers,
    };
    visitor.visit_file(ast);
    visitor.costs
}

/// Lines inside debug-only assertions and logging macros. Reads there are
/// diagnostics (compiled out of release builds or evaluated only when the log
/// level is enabled), not part of the dispatch's validation order.
fn val001_diagnostic_macro_mask(ast: &SynFile, line_count: usize) -> Vec<bool> {
    struct DiagnosticMacroVisitor {
        mask: Vec<bool>,
    }

    impl<'ast> Visit<'ast> for DiagnosticMacroVisitor {
        fn visit_macro(&mut self, mac: &'ast Macro) {
            let is_log_path = mac
                .path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == "log");
            if is_log_path
                || matches!(
                    macro_name(mac).as_deref(),
                    Some(
                        "debug_assert"
                            | "debug_assert_eq"
                            | "debug_assert_ne"
                            | "log"
                            | "trace"
                            | "debug"
                            | "info"
                            | "warn"
                            | "error"
                    )
                )
            {
                let start = span_line(mac.span()).saturating_sub(1);
                let end = mac.span().end().line.saturating_sub(1);
                for line in start..=end.min(self.mask.len().saturating_sub(1)) {
                    self.mask[line] = true;
                }
            }
            visit::visit_macro(self, mac);
        }
    }

    let mut visitor = DiagnosticMacroVisitor {
        mask: vec![false; line_count],
    };
    visitor.visit_file(ast);
    visitor.mask
}

/// Calls found in an expression: storage reads matching the configured heavy
/// patterns, macros, and calls into pallet or config code. Same-pallet helpers
/// (`Self::f`, `Pallet::<T>::f`) and same-crate free functions (`f(..)`) are
/// resolved through `helpers`: they count as storage reads when their body
/// reads storage and as opaque when they cannot be resolved; a bare call to a
/// function the crate does not define is ignored. Config and trait calls (`T::..`, `<T as ..>::..`) stay opaque;
/// `T::Constant::get()` is a type-level constant.
#[derive(Default)]
struct Val001CallScan<'a> {
    heavy_operations: &'a [String],
    /// `None` while summarising helper bodies: helper calls are only recorded.
    helpers: Option<&'a Val001Helpers>,
    heavy: bool,
    opaque: bool,
    macro_call: bool,
    helper_calls: Vec<(String, bool)>,
}

impl<'a> Val001CallScan<'a> {
    fn scan(
        expr: &Expr,
        heavy_operations: &'a [String],
        helpers: Option<&'a Val001Helpers>,
    ) -> Self {
        let mut scan = Self {
            heavy_operations,
            helpers,
            ..Self::default()
        };
        scan.visit_expr(expr);
        scan
    }
}

/// The helper name of a same-pallet call: `Self::f`, `Pallet::<T>::f`,
/// `crate::Pallet::<T>::f`.
fn val001_helper_call_name(path: &syn::ExprPath) -> Option<String> {
    if path.qself.is_some() {
        return None;
    }
    let segments = path.path.segments.iter().collect::<Vec<_>>();
    let (owner, rest) = match segments.as_slice() {
        [owner, name] => (owner, name),
        [root, owner, name]
            if matches!(root.ident.to_string().as_str(), "crate" | "self" | "super") =>
        {
            (owner, name)
        }
        _ => return None,
    };
    (owner.ident == "Self" || owner.ident == "Pallet").then(|| rest.ident.to_string())
}

/// The name of a bare call `f(..)`, which may be a same-crate free function.
fn val001_free_call_name(path: &syn::ExprPath) -> Option<String> {
    match path.path.segments.iter().collect::<Vec<_>>().as_slice() {
        [name] if path.qself.is_none() && path.path.leading_colon.is_none() => {
            Some(name.ident.to_string())
        }
        _ => None,
    }
}

impl<'ast> Visit<'ast> for Val001CallScan<'_> {
    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(path) = &*call.func {
            let head = format!("{}(", compact_tokens(&call.func));
            let full = format!("{head}{})", compact_tokens(&call.args));
            let segments = &path.path.segments;
            let first = segments.first().map(|segment| segment.ident.to_string());
            let is_constant_get = call.args.is_empty()
                && segments
                    .last()
                    .is_some_and(|segment| segment.ident == "get")
                && (path.qself.is_some() || first.as_deref() == Some("T"));
            let is_type_constant = head.starts_with("T::") || is_constant_get;
            self.heavy |= self.heavy_operations.iter().any(|pattern| {
                !(pattern == "::get(" && is_type_constant)
                    && full
                        .match_indices(pattern.as_str())
                        .any(|(index, _)| index < head.len())
            });

            let helper = val001_helper_call_name(path)
                .map(|name| (name, false))
                .or_else(|| val001_free_call_name(path).map(|name| (name, true)));
            if let Some((name, free)) = helper {
                match self.helpers {
                    None => self.helper_calls.push((name, free)),
                    Some(helpers) => match helpers.cost(&name, free) {
                        Val001HelperCost::Cheap => {}
                        Val001HelperCost::Heavy => self.heavy = true,
                        Val001HelperCost::Unknown => self.opaque = true,
                    },
                }
            } else {
                let crosses_pallet_boundary = path.qself.is_some()
                    || matches!(first.as_deref(), Some("Self" | "T"))
                    || segments.iter().any(|segment| segment.ident == "Pallet");
                self.opaque |= segments.len() > 1 && crosses_pallet_boundary && !is_constant_get;
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        self.macro_call = true;
        // Look into expression-like macro bodies (`ensure!`, `vec!`, ...) so
        // reads and helper calls inside them are still seen.
        if let Ok(arguments) = mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
        {
            for argument in &arguments {
                self.visit_expr(argument);
            }
        }
    }
}

/// What calling a same-pallet helper costs, judged from its body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Val001HelperCost {
    /// No storage read and no opaque call, transitively.
    Cheap,
    /// Reads storage, transitively.
    Heavy,
    /// Not defined in the crate, calls config/trait code, or too deep.
    Unknown,
}

/// A helper body's direct calls; helper calls are resolved lazily. Calls are
/// `(name, free)`, where `free` marks a bare `f(..)` call.
#[derive(Clone)]
struct Val001HelperSummary {
    heavy: bool,
    opaque: bool,
    calls: Vec<(String, bool)>,
}

#[derive(Default)]
struct Val001HelperIndex {
    summaries: HashMap<String, Vec<Val001HelperSummary>>,
}

impl Val001HelperIndex {
    fn add_file(&mut self, ast: &SynFile, heavy_operations: &[String]) {
        struct Collector<'a> {
            index: &'a mut Val001HelperIndex,
            heavy_operations: &'a [String],
        }

        impl Collector<'_> {
            fn add(&mut self, name: &syn::Ident, block: &syn::Block) {
                let mut scan = Val001CallScan {
                    heavy_operations: self.heavy_operations,
                    ..Val001CallScan::default()
                };
                scan.visit_block(block);
                self.index
                    .summaries
                    .entry(name.to_string())
                    .or_default()
                    .push(Val001HelperSummary {
                        heavy: scan.heavy,
                        opaque: scan.opaque,
                        calls: scan.helper_calls,
                    });
            }
        }

        impl<'ast> Visit<'ast> for Collector<'_> {
            fn visit_item_fn(&mut self, item: &'ast ItemFn) {
                self.add(&item.sig.ident, &item.block);
                visit::visit_item_fn(self, item);
            }

            fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
                self.add(&item.sig.ident, &item.block);
                visit::visit_impl_item_fn(self, item);
            }

            fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
                if let Some(block) = &item.default {
                    self.add(&item.sig.ident, block);
                }
                visit::visit_trait_item_fn(self, item);
            }
        }

        Collector {
            index: self,
            heavy_operations,
        }
        .visit_file(ast);
    }
}

/// Helper functions a `Self::f` call may resolve to: every function in the
/// enclosing crate's `src/` tree, or in the scanned file when it is not part
/// of a crate on disk.
struct Val001Helpers {
    index: Rc<Val001HelperIndex>,
}

/// Helper calls are followed at most this deep. This only bounds the cost of
/// the lookup: a deeper chain is unknown, so the guard is suppressed.
const VAL001_HELPER_DEPTH: usize = 4;

impl Val001Helpers {
    fn for_file(ctx: &FileContext, ast: Option<&SynFile>, heavy_operations: &[String]) -> Self {
        let crate_dir = fs::canonicalize(&ctx.path)
            .ok()
            .and_then(|path| crate_root(&path).map(Path::to_path_buf));
        let Some(crate_dir) = crate_dir else {
            let mut index = Val001HelperIndex::default();
            if let Some(ast) = ast {
                index.add_file(ast, heavy_operations);
            }
            return Self {
                index: Rc::new(index),
            };
        };
        let index = scan_cached(crate_dir, |crate_dir| {
            let mut index = Val001HelperIndex::default();
            for entry in walkdir::WalkDir::new(crate_dir.join("src"))
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_type().is_file()
                        && entry.path().extension().is_some_and(|ext| ext == "rs")
                })
            {
                if let Some(ast) = fs::read_to_string(entry.path())
                    .ok()
                    .and_then(|source| syn::parse_file(&source).ok())
                {
                    index.add_file(&ast, heavy_operations);
                }
            }
            Rc::new(index)
        });
        Self { index }
    }

    fn cost(&self, name: &str, free: bool) -> Val001HelperCost {
        self.cost_at(name, free, 0, &mut HashSet::new())
    }

    /// Combines every definition named `name`: a heavy one wins, then an
    /// unknown one. Recursion back into a helper being resolved adds nothing.
    fn cost_at(
        &self,
        name: &str,
        free: bool,
        depth: usize,
        resolving: &mut HashSet<String>,
    ) -> Val001HelperCost {
        let Some(definitions) = self.index.summaries.get(name) else {
            // A bare call to something the crate does not define is a
            // constructor or an imported function, judged as before.
            return if free {
                Val001HelperCost::Cheap
            } else {
                Val001HelperCost::Unknown
            };
        };
        if depth >= VAL001_HELPER_DEPTH {
            return Val001HelperCost::Unknown;
        }
        if !resolving.insert(name.to_string()) {
            return Val001HelperCost::Cheap;
        }
        let mut cost = Val001HelperCost::Cheap;
        for definition in definitions {
            if definition.heavy {
                cost = Val001HelperCost::Heavy;
                break;
            }
            if definition.opaque {
                cost = Val001HelperCost::Unknown;
            }
            for (call, free) in &definition.calls {
                match self.cost_at(call, *free, depth + 1, resolving) {
                    Val001HelperCost::Cheap => {}
                    Val001HelperCost::Heavy => {
                        resolving.remove(name);
                        return Val001HelperCost::Heavy;
                    }
                    Val001HelperCost::Unknown => cost = Val001HelperCost::Unknown,
                }
            }
        }
        resolving.remove(name);
        cost
    }
}

const VAL001_STORAGE_MUTATORS: &[&str] = &[
    "mutate",
    "try_mutate",
    "mutate_exists",
    "try_mutate_exists",
    "mutate_extant",
    "try_mutate_extant",
];

/// A zero-based (line, column) source position.
type Val001Position = (usize, usize);

/// A local introduced by `let`, `if let`/`while let`, a `match` arm, a `for`
/// pattern or a closure parameter, with the lexical region it is visible in.
#[derive(Clone)]
struct Val001Binding {
    name: String,
    /// Visible from this position to the end of line `scope_end`.
    visible_from: Val001Position,
    scope_end: usize,
    /// The initializer, then later assignments, in source order.
    sources: Vec<Val001Source>,
}

/// A value a binding receives from its initializer or an assignment.
#[derive(Clone)]
struct Val001Source {
    text: String,
    /// Where the source's names resolve; sources are ordered by it.
    position: Val001Position,
    /// The source expression's lines.
    lines: (usize, usize),
    reads_storage: bool,
    /// The line the value is computed on, when it comes from storage,
    /// pallet/config code, or an unanalysed macro; pure arithmetic on
    /// arguments can move along with the guard that uses it.
    computed_at: Option<usize>,
    /// The bindings the source references.
    deps: Vec<usize>,
}

fn val001_identifiers(source: &str) -> impl Iterator<Item = &str> {
    source
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| {
            token
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        })
}

/// Per-function facts deciding whether a later guard could be hoisted above a
/// heavy read. Lines are zero-based and inclusive.
#[derive(Default)]
struct Val001FunctionFacts {
    start: usize,
    end: usize,
    bindings: Vec<Val001Binding>,
    /// Bodies of closures that may run zero or many times. Closures passed to
    /// storage mutators run exactly once and are not recorded.
    deferred_closures: Vec<(usize, usize)>,
    /// Mutually exclusive branches: `if`/`else` arms and `match` arms.
    branch_groups: Vec<Vec<(usize, usize)>>,
}

impl Val001FunctionFacts {
    /// The innermost binding of `name` visible at `position`; `None` for
    /// parameters, items and names that are not locals.
    fn resolve(&self, name: &str, position: Val001Position) -> Option<usize> {
        self.bindings
            .iter()
            .enumerate()
            .filter(|(_, binding)| {
                binding.name == name
                    && binding.visible_from <= position
                    && position.0 <= binding.scope_end
            })
            .max_by_key(|(_, binding)| binding.visible_from)
            .map(|(index, _)| index)
    }

    /// The latest value binding `index` received before `position`.
    fn source_before(&self, index: usize, position: Val001Position) -> Option<&Val001Source> {
        self.bindings[index]
            .sources
            .iter()
            .rev()
            .find(|source| source.position < position)
    }

    /// Whether `name` still holds the value read at `heavy_line` when used on
    /// `line`. Names that are not locals are assumed to. A use inside a
    /// reassignment of `name` whose right-hand side references `name` sees the
    /// value it replaces.
    fn holds_read(&self, name: &str, heavy_line: usize, line: usize) -> bool {
        let position = (line, usize::MAX);
        self.resolve(name, position).is_none_or(|index| {
            let source = match self.source_before(index, position) {
                Some(source)
                    if source.lines.0 <= line
                        && line <= source.lines.1
                        && val001_identifiers(&source.text).any(|ident| ident == name) =>
                {
                    self.source_before(index, source.position).or(Some(source))
                }
                source => source,
            };
            source.is_none_or(|source| source.lines.0 <= heavy_line && heavy_line <= source.lines.1)
        })
    }

    fn link_dependencies(&mut self) {
        for index in 0..self.bindings.len() {
            for source in 0..self.bindings[index].sources.len() {
                let Val001Source { text, position, .. } = &self.bindings[index].sources[source];
                let mut deps = Vec::new();
                for name in val001_identifiers(text) {
                    if let Some(dep) = self.resolve(name, *position) {
                        if !deps.contains(&dep) {
                            deps.push(dep);
                        }
                    }
                }
                self.bindings[index].sources[source].deps = deps;
            }
        }
    }

    /// Whether the guard at `guard_line` is not an independent precondition
    /// of the read at `heavy_line`: it consumes storage-derived data, a value
    /// computed after the read, or it lives where it cannot be hoisted from.
    /// Only bindings visible at the guard count, each with the latest value
    /// it received before the guard, followed through aliases.
    fn guard_is_not_hoistable(&self, heavy_line: usize, guard_line: usize, guard: &str) -> bool {
        let position = (guard_line, usize::MAX);
        let mut pending = val001_identifiers(guard)
            .filter_map(|name| self.resolve(name, position))
            .map(|index| (index, position))
            .collect::<Vec<_>>();
        let mut seen = HashSet::new();
        while let Some((index, position)) = pending.pop() {
            if !seen.insert((index, position)) {
                continue;
            }
            let Some(source) = self.source_before(index, position) else {
                continue;
            };
            if source.reads_storage || source.computed_at.is_some_and(|line| line > heavy_line) {
                return true;
            }
            pending.extend(source.deps.iter().map(|&dep| (dep, source.position)));
        }
        let within = |(start, end): (usize, usize), line: usize| start <= line && line <= end;
        if self
            .deferred_closures
            .iter()
            .any(|&range| within(range, guard_line) && !within(range, heavy_line))
        {
            return true;
        }
        self.branch_groups.iter().any(|branches| {
            let heavy_branch = branches.iter().position(|&range| within(range, heavy_line));
            let guard_branch = branches.iter().position(|&range| within(range, guard_line));
            matches!((heavy_branch, guard_branch), (Some(heavy), Some(guard)) if heavy != guard)
        })
    }
}

fn val001_function_facts(
    ast: &SynFile,
    heavy_operations: &[String],
    helpers: &Val001Helpers,
) -> Vec<Val001FunctionFacts> {
    #[derive(Clone, Default)]
    struct ClosureContext {
        /// The call's source text without the closure argument.
        source: String,
        /// The call reads storage or is a storage mutator.
        reads_storage: bool,
        /// Storage mutators run their closure exactly once, synchronously.
        runs_once: bool,
    }

    struct BodyVisitor<'a> {
        heavy_operations: &'a [String],
        helpers: &'a Val001Helpers,
        facts: Val001FunctionFacts,
        /// Closure argument context keyed by the closure's start position.
        closure_context: HashMap<Val001Position, ClosureContext>,
        /// Last lines of the enclosing blocks, innermost last.
        scopes: Vec<usize>,
        /// The end of the `if`/`while` body an `if let` condition binds into.
        let_scope_end: Option<usize>,
        /// `name = value` assignments, resolved once all bindings are known.
        assignments: Vec<(String, Val001Position, Val001Source)>,
    }

    fn line_range<T: Spanned>(node: &T) -> (usize, usize) {
        let span = node.span();
        (
            span_line(span).saturating_sub(1),
            span.end().line.saturating_sub(1),
        )
    }

    fn start_position<T: Spanned>(node: &T) -> Val001Position {
        let start = node.span().start();
        (start.line.saturating_sub(1), start.column)
    }

    fn end_position<T: Spanned>(node: &T) -> Val001Position {
        let end = node.span().end();
        (end.line.saturating_sub(1), end.column)
    }

    impl<'a> BodyVisitor<'a> {
        fn scan(&self, expr: &Expr) -> Val001CallScan<'a> {
            Val001CallScan::scan(expr, self.heavy_operations, Some(self.helpers))
        }

        fn current_scope_end(&self) -> usize {
            self.scopes.last().copied().unwrap_or(self.facts.end)
        }

        /// Binds each of `names` with the facts of `binding`.
        fn bind(&mut self, names: Vec<String>, binding: Val001Binding) {
            for name in names {
                self.facts.bindings.push(Val001Binding {
                    name,
                    ..binding.clone()
                });
            }
        }

        fn bind_expr(
            &mut self,
            pat: &Pat,
            source: &Expr,
            visible_from: Val001Position,
            scope_end: usize,
        ) {
            let names = val001_pat_binding_names(pat);
            if names.is_empty() {
                return;
            }
            let scan = self.scan(source);
            let computed = scan.heavy || scan.opaque || scan.macro_call;
            self.bind(
                names,
                Val001Binding {
                    name: String::new(),
                    visible_from,
                    scope_end,
                    sources: vec![Val001Source {
                        text: compact_tokens(source),
                        position: start_position(source),
                        lines: line_range(source),
                        reads_storage: scan.heavy,
                        computed_at: computed.then_some(visible_from.0),
                        deps: Vec::new(),
                    }],
                },
            );
        }

        fn register_closure_arguments<'e>(
            &mut self,
            callee: String,
            arguments: impl Iterator<Item = &'e Expr> + Clone,
            mutates_storage: bool,
            storage_receiver: bool,
        ) {
            let mut source = callee;
            let mut reads_storage = mutates_storage || storage_receiver;
            for argument in arguments.clone() {
                if !matches!(argument, Expr::Closure(_)) {
                    source.push_str(&compact_tokens(argument));
                    reads_storage |= self.scan(argument).heavy;
                }
            }
            for argument in arguments {
                if let Expr::Closure(closure) = argument {
                    self.closure_context.insert(
                        start_position(closure),
                        ClosureContext {
                            source: source.clone(),
                            reads_storage,
                            runs_once: mutates_storage,
                        },
                    );
                }
            }
        }
    }

    impl<'ast> Visit<'ast> for BodyVisitor<'_> {
        fn visit_block(&mut self, block: &'ast syn::Block) {
            self.scopes.push(line_range(block).1);
            visit::visit_block(self, block);
            self.scopes.pop();
        }

        fn visit_local(&mut self, local: &'ast Local) {
            if let Some(init) = &local.init {
                let scope_end = self.current_scope_end();
                self.bind_expr(&local.pat, &init.expr, end_position(local), scope_end);
            }
            visit::visit_local(self, local);
        }

        fn visit_expr_let(&mut self, expr: &'ast syn::ExprLet) {
            let scope_end = self
                .let_scope_end
                .unwrap_or_else(|| self.current_scope_end());
            self.bind_expr(&expr.pat, &expr.expr, end_position(expr), scope_end);
            visit::visit_expr_let(self, expr);
        }

        fn visit_expr_assign(&mut self, expr: &'ast syn::ExprAssign) {
            if let Expr::Path(path) = strip_expr_wrappers(&expr.left) {
                if let Some(name) = path.path.get_ident() {
                    let scan = self.scan(&expr.right);
                    self.assignments.push((
                        name.to_string(),
                        start_position(expr),
                        Val001Source {
                            text: compact_tokens(&expr.right),
                            position: start_position(&expr.right),
                            lines: line_range(&expr.right),
                            reads_storage: scan.heavy,
                            computed_at: (scan.heavy || scan.opaque || scan.macro_call)
                                .then_some(expr.span().start().line.saturating_sub(1)),
                            deps: Vec::new(),
                        },
                    ));
                }
            }
            visit::visit_expr_assign(self, expr);
        }

        fn visit_expr_for_loop(&mut self, expr: &'ast ExprForLoop) {
            let (body_start, body_end) = line_range(&expr.body);
            self.bind_expr(&expr.pat, &expr.expr, (body_start, 0), body_end);
            visit::visit_expr_for_loop(self, expr);
        }

        fn visit_expr_match(&mut self, expr: &'ast syn::ExprMatch) {
            let mut branches = Vec::new();
            for arm in &expr.arms {
                let range = line_range(arm);
                self.bind_expr(&arm.pat, &expr.expr, start_position(arm), range.1);
                branches.push(range);
            }
            self.facts.branch_groups.push(branches);
            visit::visit_expr_match(self, expr);
        }

        fn visit_expr_if(&mut self, expr: &'ast ExprIf) {
            let mut branches = vec![line_range(&expr.then_branch)];
            if let Some((_, else_branch)) = &expr.else_branch {
                branches.push(line_range(else_branch));
            }
            self.facts.branch_groups.push(branches);

            let outer = self.let_scope_end.replace(line_range(&expr.then_branch).1);
            self.visit_expr(&expr.cond);
            self.let_scope_end = outer;
            self.visit_block(&expr.then_branch);
            if let Some((_, else_branch)) = &expr.else_branch {
                self.visit_expr(else_branch);
            }
        }

        fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
            let outer = self.let_scope_end.replace(line_range(&expr.body).1);
            self.visit_expr(&expr.cond);
            self.let_scope_end = outer;
            self.visit_block(&expr.body);
        }

        fn visit_expr_call(&mut self, call: &'ast ExprCall) {
            let mutates_storage = matches!(
                &*call.func,
                Expr::Path(path) if path_last_ident(&path.path)
                    .is_some_and(|name| VAL001_STORAGE_MUTATORS.contains(&name.as_str()))
            );
            self.register_closure_arguments(
                compact_tokens(&call.func),
                call.args.iter(),
                mutates_storage,
                false,
            );
            visit::visit_expr_call(self, call);
        }

        fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
            let method = call.method.to_string();
            let mutates_storage = VAL001_STORAGE_MUTATORS.contains(&method.as_str());
            let receiver_reads_storage = self.scan(&call.receiver).heavy;
            self.register_closure_arguments(
                format!("{}.{method}", compact_tokens(&call.receiver)),
                call.args.iter(),
                mutates_storage,
                receiver_reads_storage,
            );
            visit::visit_expr_method_call(self, call);
        }

        fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
            let context = self
                .closure_context
                .get(&start_position(closure))
                .cloned()
                .unwrap_or_default();
            let names = closure
                .inputs
                .iter()
                .flat_map(val001_pat_binding_names)
                .collect::<Vec<_>>();
            let body = line_range(&closure.body);
            self.bind(
                names,
                Val001Binding {
                    name: String::new(),
                    visible_from: start_position(&closure.body),
                    scope_end: body.1,
                    sources: vec![Val001Source {
                        text: context.source,
                        position: start_position(closure),
                        lines: line_range(closure),
                        reads_storage: context.reads_storage,
                        computed_at: Some(body.0),
                        deps: Vec::new(),
                    }],
                },
            );
            if !context.runs_once {
                self.facts.deferred_closures.push(body);
            }
            visit::visit_expr_closure(self, closure);
        }
    }

    fn analyze(
        block: &syn::Block,
        heavy_operations: &[String],
        helpers: &Val001Helpers,
    ) -> Val001FunctionFacts {
        let (start, end) = line_range(block);
        let mut visitor = BodyVisitor {
            heavy_operations,
            helpers,
            facts: Val001FunctionFacts {
                start,
                end,
                ..Val001FunctionFacts::default()
            },
            closure_context: HashMap::new(),
            scopes: Vec::new(),
            let_scope_end: None,
            assignments: Vec::new(),
        };
        visitor.visit_block(block);

        let mut facts = visitor.facts;
        for (name, position, source) in visitor.assignments {
            if let Some(index) = facts.resolve(&name, position) {
                facts.bindings[index].sources.push(source);
            }
        }
        facts.link_dependencies();
        facts
    }

    struct FunctionVisitor<'a> {
        heavy_operations: &'a [String],
        helpers: &'a Val001Helpers,
        functions: Vec<Val001FunctionFacts>,
    }

    impl<'ast> Visit<'ast> for FunctionVisitor<'_> {
        fn visit_item_fn(&mut self, item: &'ast ItemFn) {
            self.functions
                .push(analyze(&item.block, self.heavy_operations, self.helpers));
            visit::visit_item_fn(self, item);
        }

        fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
            self.functions
                .push(analyze(&item.block, self.heavy_operations, self.helpers));
            visit::visit_impl_item_fn(self, item);
        }

        fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
            if let Some(block) = &item.default {
                self.functions
                    .push(analyze(block, self.heavy_operations, self.helpers));
            }
            visit::visit_trait_item_fn(self, item);
        }
    }

    let mut visitor = FunctionVisitor {
        heavy_operations,
        helpers,
        functions: Vec::new(),
    };
    visitor.visit_file(ast);
    visitor.functions
}

/// The innermost function body containing `line`.
fn val001_function_at(
    functions: &[Val001FunctionFacts],
    line: usize,
) -> Option<&Val001FunctionFacts> {
    functions
        .iter()
        .filter(|function| function.start <= line && line <= function.end)
        .min_by_key(|function| function.end - function.start)
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
                if let Pat::Type(pat_type) = &local.pat {
                    if type_contains_named(&pat_type.ty, &["Vec"]) {
                        if let Some(init) = &local.init {
                            if let Expr::MethodCall(method_call) = &*init.expr {
                                if method_call.method == "collect"
                                    && method_call.turbofish.is_none()
                                {
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
                        }
                    }
                }
                visit::visit_local(self, local);
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
        if is_pure_test_path(&ctx.classification_path) {
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
                    let Some(expr) = attr_expr_for_path(attr, &["pallet", "weight"]) else {
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

        let mut sanitizer = SourceSanitizer::default();
        let diagnostics = ctx
            .content
            .lines()
            .enumerate()
            .filter_map(|(idx, line)| {
                let sanitized = sanitizer.strip_line(line);
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
            matches!(name, "CustomInvalidity" | "CustomValidity")
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

        fn log_macro_end(lines: &[&str], start: usize) -> usize {
            let mut paren_depth = 0i32;
            for (offset, line) in lines[start..start.saturating_add(32).min(lines.len())]
                .iter()
                .enumerate()
            {
                paren_depth += line.chars().filter(|character| *character == '(').count() as i32;
                paren_depth -= line.chars().filter(|character| *character == ')').count() as i32;
                if paren_depth <= 0 {
                    return start + offset;
                }
            }
            lines.len().saturating_sub(1)
        }

        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();
        let test_mask = cfg_test_module_mask(ctx.content);
        let mut reported_log_lines = HashSet::new();

        for (i, line) in lines.iter().enumerate() {
            if test_mask[i] || !line.contains("SubmitTransaction::<") {
                continue;
            }

            let search_end = (i + 24).min(lines.len());
            let call_window = lines[i..search_end].join("\n");
            if !call_window.contains("submit_transaction")
                && !call_window.contains("submit_unsigned_transaction")
            {
                continue;
            }
            for j in i..search_end {
                if test_mask[j] || !is_log_macro_line(lines[j]) {
                    continue;
                }

                let log_end = log_macro_end(&lines, j);
                let log_window = (j..=log_end)
                    .filter(|&k| !test_mask[k])
                    .map(|k| lines[k])
                    .collect::<Vec<_>>()
                    .join("\n");

                if !log_window.contains("target:") && reported_log_lines.insert(j) {
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
