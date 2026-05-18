use regex::Regex;
use std::collections::HashSet;
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, File as SynFile, ImplItem, ItemFn, ItemImpl, ItemTrait, TraitItem,
};

use super::semantic::strip_strings_and_line_comments;
use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    rules::LintRule,
};

fn span_line(span: proc_macro2::Span) -> usize {
    span.start().line
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

fn parse_ast(content: &str) -> Option<SynFile> {
    syn::parse_file(content).ok()
}

fn collect_weight_function_names(content: &str) -> Vec<(String, usize)> {
    if let Some(ast) = parse_ast(content) {
        struct WeightFnVisitor {
            names: Vec<(String, usize)>,
        }

        impl<'ast> Visit<'ast> for WeightFnVisitor {
            fn visit_item_trait(&mut self, item_trait: &'ast ItemTrait) {
                if item_trait.ident == "WeightInfo" {
                    for item in &item_trait.items {
                        if let TraitItem::Fn(trait_fn) = item {
                            self.names
                                .push((trait_fn.sig.ident.to_string(), span_line(trait_fn.span())));
                        }
                    }
                }
                visit::visit_item_trait(self, item_trait);
            }

            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                let implements_weight_info = item_impl
                    .trait_
                    .as_ref()
                    .and_then(|(_, path, _)| path_last_ident(path))
                    .as_deref()
                    == Some("WeightInfo");
                if implements_weight_info {
                    for item in &item_impl.items {
                        if let ImplItem::Fn(item_fn) = item {
                            self.names
                                .push((item_fn.sig.ident.to_string(), span_line(item_fn.span())));
                        }
                    }
                }
                visit::visit_item_impl(self, item_impl);
            }
        }

        let mut visitor = WeightFnVisitor { names: Vec::new() };
        visitor.visit_file(&ast);
        if !visitor.names.is_empty() {
            return visitor.names;
        }
    }

    let weight_fn_re = Regex::new(r"fn\s+(\w+)\s*\(").unwrap();
    let mut weight_fns = Vec::new();
    let mut in_trait = false;
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.contains("trait WeightInfo")
            || trimmed.contains("impl<T: frame_system::Config> WeightInfo")
            || trimmed.contains("impl<T: frame_system::Config>") && trimmed.contains("WeightInfo")
        {
            in_trait = true;
            continue;
        }
        if in_trait {
            if let Some(captures) = weight_fn_re.captures(trimmed) {
                let fn_name = captures.get(1).unwrap().as_str().to_string();
                weight_fns.push((fn_name, i + 1));
            }
        }
    }
    weight_fns
}

fn collect_benchmark_names(content: &str) -> HashSet<String> {
    let mut benchmark_names = HashSet::new();

    if let Some(ast) = parse_ast(content) {
        struct BenchmarkFnVisitor {
            names: HashSet<String>,
        }

        impl<'ast> Visit<'ast> for BenchmarkFnVisitor {
            fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
                if has_attr(&item_fn.attrs, &["benchmark"]) {
                    self.names.insert(item_fn.sig.ident.to_string());
                }
                visit::visit_item_fn(self, item_fn);
            }
        }

        let mut visitor = BenchmarkFnVisitor {
            names: HashSet::new(),
        };
        visitor.visit_file(&ast);
        benchmark_names.extend(visitor.names);
    }

    let bench_lines: Vec<&str> = content.lines().collect();
    let mut in_benchmarks_macro = false;
    let mut next_fn_is_benchmark = false;
    let fn_re = Regex::new(r"fn\s+(\w+)").unwrap();
    let bare_fn_re = Regex::new(r"^(\w+)\s*\{").unwrap();

    for line in &bench_lines {
        let trimmed = line.trim();
        if trimmed.starts_with("benchmarks!")
            || trimmed.contains("benchmarks! {")
            || trimmed == "#[benchmarks]"
        {
            in_benchmarks_macro = true;
        }
        if trimmed == "#[benchmark]" {
            next_fn_is_benchmark = true;
            continue;
        }
        if next_fn_is_benchmark {
            if let Some(caps) = fn_re.captures(trimmed) {
                benchmark_names.insert(caps.get(1).unwrap().as_str().to_string());
                next_fn_is_benchmark = false;
                continue;
            }
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            next_fn_is_benchmark = false;
        }
        if in_benchmarks_macro && !trimmed.starts_with("//") {
            if let Some(caps) = bare_fn_re.captures(trimmed) {
                let name = caps.get(1).unwrap().as_str();
                if !matches!(
                    name,
                    "mod" | "use" | "impl" | "pub" | "fn" | "let" | "if" | "for" | "where" | "type"
                ) {
                    benchmark_names.insert(name.to_string());
                }
            }
        }
    }

    benchmark_names
}

fn has_matching_benchmark_name(benchmark_names: &HashSet<String>, target_name: &str) -> bool {
    benchmark_names.iter().any(|benchmark_name| {
        benchmark_name == target_name
            || benchmark_name
                .strip_prefix(target_name)
                .is_some_and(|suffix| suffix.starts_with('_'))
    })
}

fn collect_extrinsic_names(content: &str) -> Vec<(String, usize)> {
    if let Some(ast) = parse_ast(content) {
        struct ExtrinsicVisitor {
            names: Vec<(String, usize)>,
        }

        impl<'ast> Visit<'ast> for ExtrinsicVisitor {
            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                if has_attr(&item_impl.attrs, &["pallet", "call"]) {
                    for item in &item_impl.items {
                        if let ImplItem::Fn(item_fn) = item {
                            if has_attr(&item_fn.attrs, &["pallet", "call_index"]) {
                                self.names.push((
                                    item_fn.sig.ident.to_string(),
                                    span_line(item_fn.span()),
                                ));
                            }
                        }
                    }
                }
                visit::visit_item_impl(self, item_impl);
            }
        }

        let mut visitor = ExtrinsicVisitor { names: Vec::new() };
        visitor.visit_file(&ast);
        if !visitor.names.is_empty() {
            return visitor.names;
        }
    }

    let call_index_re = Regex::new(r"#\[pallet::call_index\s*\(").unwrap();
    let fn_re = Regex::new(r"pub fn\s+(\w+)").unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let mut extrinsic_fns = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if call_index_re.is_match(line) {
            for (j, line) in lines
                .iter()
                .enumerate()
                .take((i + 5).min(lines.len()))
                .skip(i + 1)
            {
                if let Some(caps) = fn_re.captures(line) {
                    extrinsic_fns.push((caps.get(1).unwrap().as_str().to_string(), j + 1));
                    break;
                }
            }
        }
    }
    extrinsic_fns
}

fn parse_fn_name_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("fn ")?;
    let name: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn parse_macro_benchmark_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let brace_idx = trimmed.find('{')?;
    let candidate = trimmed[..brace_idx].trim();
    if candidate.is_empty() || candidate.contains(' ') || candidate.contains('(') {
        return None;
    }
    if matches!(
        candidate,
        "mod"
            | "use"
            | "impl"
            | "pub"
            | "fn"
            | "let"
            | "if"
            | "for"
            | "where"
            | "type"
            | "benchmarks"
    ) {
        return None;
    }
    Some(candidate.to_string())
}

fn find_block_end_from(lines: &[&str], start: usize) -> usize {
    let mut depth: i32 = 0;
    let mut body_started = false;
    for (idx, line) in lines.iter().enumerate().skip(start) {
        depth += line.chars().filter(|&c| c == '{').count() as i32;
        if depth > 0 {
            body_started = true;
        }
        depth -= line.chars().filter(|&c| c == '}').count() as i32;
        if body_started && depth == 0 {
            return idx;
        }
    }
    lines.len().saturating_sub(1)
}

fn has_verification_in_text(body_lines: &[&str], verification_patterns: &[String]) -> bool {
    let body_outside_measured_blocks = body_lines_outside_measured_blocks(body_lines);
    let sanitized_body = body_outside_measured_blocks
        .iter()
        .map(|line| strip_strings_and_line_comments(line))
        .collect::<Vec<_>>();

    sanitized_body.iter().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("verify ") || trimmed == "verify{" || trimmed.starts_with("verify{")
    }) || sanitized_body.iter().any(|line| {
        verification_patterns.iter().any(|pattern| {
            line.contains(&format!("{pattern}(")) || line.contains(&format!("{pattern} {{"))
        })
    }) || sanitized_body.iter().any(|line| {
        line.contains("assert!(")
            || line.contains("assert_eq!(")
            || line.contains("assert_ne!(")
            || line.contains("assert_ok!(")
            || line.contains("assert_noop!(")
            || line.contains("assert_err!(")
            || line.contains("ensure!(")
            || line.contains("assert_last_event(")
            || line.contains("assert_has_event(")
    })
}

fn body_lines_outside_measured_blocks<'a>(body_lines: &'a [&'a str]) -> Vec<&'a str> {
    let mut kept = Vec::with_capacity(body_lines.len());
    let mut pending_block = false;
    let mut in_block = false;
    let mut block_depth: i32 = 0;

    for line in body_lines {
        let trimmed = line.trim();
        if trimmed.starts_with("#[block]") {
            pending_block = true;
            continue;
        }

        if pending_block {
            let open = line.chars().filter(|&c| c == '{').count() as i32;
            let close = line.chars().filter(|&c| c == '}').count() as i32;
            if open > 0 {
                in_block = true;
                pending_block = false;
                block_depth += open;
                block_depth -= close;
                if block_depth <= 0 {
                    in_block = false;
                    block_depth = 0;
                }
            }
            continue;
        }

        if in_block {
            block_depth += line.chars().filter(|&c| c == '{').count() as i32;
            block_depth -= line.chars().filter(|&c| c == '}').count() as i32;
            if block_depth <= 0 {
                in_block = false;
                block_depth = 0;
            }
            continue;
        }

        kept.push(*line);
    }

    kept
}

// ---------------------------------------------------------------------------
// BEN001: Every weight function must have a corresponding benchmark
// ---------------------------------------------------------------------------
// Reviewers consistently flag missing benchmarks for new extrinsics and
// weight functions.

pub struct BenchmarkForWeightFunction;

impl LintRule for BenchmarkForWeightFunction {
    fn id(&self) -> &str {
        "BEN001"
    }
    fn name(&self) -> &str {
        "benchmark-for-weight-function"
    }
    fn family(&self) -> &str {
        "benchmark"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust {
            return None;
        }

        // This rule works on weights.rs files to find weight functions
        // and cross-references with benchmarking.rs in the same directory
        let path_str = ctx.rel_path.to_string_lossy();
        if !path_str.ends_with("weights.rs") {
            return None;
        }

        let weight_fns = collect_weight_function_names(ctx.content);

        if weight_fns.is_empty() {
            return None;
        }

        // Try to find the corresponding benchmarking.rs or benchmarks.rs
        let parent = ctx.path.parent()?;
        let bench_path = if parent.join("benchmarking.rs").exists() {
            parent.join("benchmarking.rs")
        } else {
            parent.join("benchmarks.rs")
        };

        let bench_content = match std::fs::read_to_string(&bench_path) {
            Ok(c) => c,
            Err(_) => {
                // FIX #1: benchmarking.rs is absent — report ALL weight fns as missing
                let mut diagnostics = Vec::new();
                for (fn_name, line) in &weight_fns {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        rule_name: self.name().to_string(),
                        category: RuleCategory::Benchmark,
                        severity: config.rule_severity(self.id(), Severity::Warning),
                        file: ctx.path.clone(),
                        line: *line,
                        column: None,
                        end_line: None,
                        message: format!(
                            "Weight function `{}` has no benchmark — `benchmarking.rs` not found in the same directory",
                            fn_name
                        ),
                        explanation: "Every weight function must have a matching benchmark. \
                            No `benchmarking.rs` file was found alongside this `weights.rs`."
                            .to_string(),
                        suggestion: Some("Create a `benchmarking.rs` with benchmarks for all weight functions".to_string()),
                    });
                }
                return Some(diagnostics);
            }
        };

        let benchmark_names = collect_benchmark_names(&bench_content);

        let mut diagnostics = Vec::new();

        for (fn_name, line) in &weight_fns {
            if !benchmark_names.contains(fn_name) {
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    category: RuleCategory::Benchmark,
                    severity: config.rule_severity(self.id(), Severity::Warning),
                    file: ctx.path.clone(),
                    line: *line,
                    column: None,
                    end_line: None,
                    message: format!(
                        "Weight function `{}` has no corresponding benchmark in `benchmarking.rs`",
                        fn_name
                    ),
                    explanation: "Every weight function must have a matching benchmark. \
                        Without a benchmark, the weight is likely a guess or copy-paste, \
                        which can lead to under- or over-charging."
                        .to_string(),
                    suggestion: Some(format!(
                        "Add a `#[benchmark] fn {}` in `benchmarking.rs`",
                        fn_name
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

// ---------------------------------------------------------------------------
// BEN002: Benchmark must include verification
// ---------------------------------------------------------------------------
// Reviewers flag benchmarks that don't verify postconditions.

pub struct BenchmarkVerification;

impl LintRule for BenchmarkVerification {
    fn id(&self) -> &str {
        "BEN002"
    }
    fn name(&self) -> &str {
        "benchmark-verification"
    }
    fn family(&self) -> &str {
        "benchmark"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || !ctx.is_benchmark_file {
            return None;
        }

        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();

        let mut in_benchmarks_macro = false;
        let mut macro_depth: i32 = 0;
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim();

            if trimmed.starts_with("benchmarks!") {
                in_benchmarks_macro = true;
                macro_depth = 0;
            }

            let is_proc_benchmark = trimmed == "#[benchmark]";
            let is_macro_benchmark = in_benchmarks_macro
                && !is_proc_benchmark
                && parse_macro_benchmark_name(trimmed).is_some();

            if is_proc_benchmark || is_macro_benchmark {
                let (fn_name, fn_start) = if is_proc_benchmark {
                    let fs = (i + 1..lines.len())
                        .find(|&j| parse_fn_name_line(lines[j]).is_some())
                        .unwrap_or(i + 1);
                    (
                        parse_fn_name_line(lines.get(fs).copied().unwrap_or(""))
                            .unwrap_or_else(|| "unknown".to_string()),
                        fs,
                    )
                } else {
                    (
                        parse_macro_benchmark_name(trimmed)
                            .unwrap_or_else(|| "unknown".to_string()),
                        i,
                    )
                };

                let body_end = find_block_end_from(&lines, fn_start);
                let body_lines = &lines[fn_start..=body_end];
                if !has_verification_in_text(body_lines, &config.benchmarking.verification_patterns)
                {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        rule_name: self.name().to_string(),
                        category: RuleCategory::Benchmark,
                        severity: config.rule_severity(self.id(), Severity::Warning),
                        file: ctx.path.clone(),
                        line: fn_start + 1,
                        column: None,
                        end_line: Some(body_end + 1),
                        message: format!(
                            "Benchmark `{}` has no verification/postcondition check",
                            fn_name
                        ),
                        explanation: "Benchmarks should include a `verify` block that asserts \
                            the expected state change occurred. Without verification, a benchmark \
                            can silently measure a no-op."
                            .to_string(),
                        suggestion: Some(
                            "Add a `verify { ... }` block with assertions like `assert_last_event` \
                            or `assert_has_event`"
                                .to_string(),
                        ),
                    });
                }
                i = body_end + 1;
            } else {
                if in_benchmarks_macro {
                    macro_depth += lines[i].chars().filter(|&c| c == '{').count() as i32;
                    macro_depth -= lines[i].chars().filter(|&c| c == '}').count() as i32;
                    if macro_depth <= 0 {
                        in_benchmarks_macro = false;
                    }
                }
                i += 1;
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
// BEN003: #[pallet::call] extrinsic with no matching benchmark
// ---------------------------------------------------------------------------
// reviewer (~10), reviewer (~4): Every dispatchable extrinsic needs a
// benchmark. Checks lib.rs call_index functions -> benchmarking.rs.

pub struct ExtrinsicWithoutBenchmark;

impl LintRule for ExtrinsicWithoutBenchmark {
    fn id(&self) -> &str {
        "BEN003"
    }
    fn name(&self) -> &str {
        "extrinsic-without-benchmark"
    }
    fn family(&self) -> &str {
        "benchmark"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if !ctx.is_rust || ctx.is_test_file || ctx.is_benchmark_file {
            return None;
        }

        let path_str = ctx.rel_path.to_string_lossy();
        if !path_str.ends_with("lib.rs") {
            return None;
        }
        if !ctx.content.contains("#[pallet::call]") {
            return None;
        }

        let extrinsic_fns = collect_extrinsic_names(ctx.content);

        if extrinsic_fns.is_empty() {
            return None;
        }

        let parent = ctx.path.parent()?;
        let bench_path = if parent.join("benchmarking.rs").exists() {
            parent.join("benchmarking.rs")
        } else if parent.join("benchmarks.rs").exists() {
            parent.join("benchmarks.rs")
        } else {
            let mut diagnostics = Vec::new();
            for (fn_name, line) in &extrinsic_fns {
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    category: RuleCategory::Benchmark,
                    severity: config.rule_severity(self.id(), Severity::Warning),
                    file: ctx.path.clone(),
                    line: *line,
                    column: None,
                    end_line: None,
                    message: format!(
                        "Extrinsic `{}` has no benchmark — no benchmarking file found",
                        fn_name
                    ),
                    explanation:
                        "Every dispatchable extrinsic needs a benchmark for accurate weights."
                            .to_string(),
                    suggestion: Some(
                        "Create `benchmarking.rs` with benchmarks for all extrinsics".to_string(),
                    ),
                });
            }
            return Some(diagnostics);
        };

        let bench_content = match std::fs::read_to_string(&bench_path) {
            Ok(c) => c,
            Err(_) => return None,
        };

        let benchmark_names = collect_benchmark_names(&bench_content);

        let mut diagnostics = Vec::new();
        for (fn_name, line) in &extrinsic_fns {
            if !has_matching_benchmark_name(&benchmark_names, fn_name) {
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    category: RuleCategory::Benchmark,
                    severity: config.rule_severity(self.id(), Severity::Warning),
                    file: ctx.path.clone(),
                    line: *line,
                    column: None,
                    end_line: None,
                    message: format!(
                        "Extrinsic `{}` has no matching benchmark in `benchmarking.rs`",
                        fn_name
                    ),
                    explanation:
                        "Every dispatchable extrinsic needs a benchmark for accurate weights."
                            .to_string(),
                    suggestion: Some(format!(
                        "Add `#[benchmark] fn {}` in `benchmarking.rs`",
                        fn_name
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
