use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, File as SynFile, ImplItem, ItemFn, ItemImpl, ItemTrait, TraitItem,
};

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    frame_model::active_cfg_attr_path_matches,
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

fn has_attr(attrs: &[Attribute], segments: &[&str]) -> bool {
    attrs.iter().any(|attr| attr_path_matches(attr, segments))
}

fn parse_ast(content: &str) -> Option<SynFile> {
    syn::parse_file(content).ok().or_else(|| {
        // Also tolerate the legacy `verify {}` notation in authored v2 examples.
        let source = Regex::new(r"\bverify\s*\{")
            .unwrap()
            .replace_all(content, "verify! {");
        syn::parse_file(&source).ok()
    })
}

fn collect_weight_function_names(content: &str) -> Vec<(String, usize)> {
    if let Some(ast) = parse_ast(content) {
        struct WeightFnVisitor {
            names: Vec<(String, usize)>,
        }

        impl<'ast> Visit<'ast> for WeightFnVisitor {
            fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
                if !test_only(&module.attrs) {
                    visit::visit_item_mod(self, module);
                }
            }

            fn visit_item_trait(&mut self, item_trait: &'ast ItemTrait) {
                if test_only(&item_trait.attrs) {
                    return;
                }
                if item_trait.ident == "WeightInfo" {
                    for item in &item_trait.items {
                        if let TraitItem::Fn(trait_fn) = item {
                            if test_only(&trait_fn.attrs) {
                                continue;
                            }
                            self.names
                                .push((trait_fn.sig.ident.to_string(), span_line(trait_fn.span())));
                        }
                    }
                }
                visit::visit_item_trait(self, item_trait);
            }

            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                if test_only(&item_impl.attrs) {
                    return;
                }
                let implements_weight_info = item_impl
                    .trait_
                    .as_ref()
                    .and_then(|(_, path, _)| path_last_ident(path))
                    .as_deref()
                    == Some("WeightInfo");
                if implements_weight_info {
                    for item in &item_impl.items {
                        if let ImplItem::Fn(item_fn) = item {
                            if test_only(&item_fn.attrs) {
                                continue;
                            }
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
            let mut seen = HashSet::new();
            return visitor
                .names
                .into_iter()
                .filter(|(name, _)| seen.insert(name.clone()))
                .collect();
        }
    }

    Vec::new()
}

fn collect_extrinsic_names(content: &str) -> Vec<(String, usize)> {
    if let Some(ast) = parse_ast(content) {
        struct ExtrinsicVisitor {
            names: Vec<(String, usize)>,
        }

        impl<'ast> Visit<'ast> for ExtrinsicVisitor {
            fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
                if !test_only(&module.attrs) {
                    visit::visit_item_mod(self, module);
                }
            }

            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                if test_only(&item_impl.attrs) {
                    return;
                }
                if has_attr(&item_impl.attrs, &["pallet", "call"]) {
                    for item in &item_impl.items {
                        if let ImplItem::Fn(item_fn) = item {
                            if test_only(&item_fn.attrs) {
                                continue;
                            }
                            // FRAME assigns positional call indices when the attribute is
                            // omitted, so every method in a pallet call impl is an extrinsic.
                            self.names
                                .push((item_fn.sig.ident.to_string(), span_line(item_fn.span())));
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

    Vec::new()
}

// Syntax is intentionally kept here: these macros consume authored verification
// and call markers before compiler expansion.
struct Benchmark {
    name: String,
    line: usize,
    end_line: usize,
    calls: HashSet<String>,
    verification: TokenStream,
    name_only: bool,
}

fn test_only(attrs: &[Attribute]) -> bool {
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

fn excluded_context(ctx: &FileContext) -> bool {
    use crate::project_model::SourceTargetKind;
    ctx.is_test_file
        || ctx.path.components().any(|part| {
            matches!(
                part.as_os_str().to_str(),
                Some("examples" | "tests" | "test" | "integration-tests" | "integration_tests")
            )
        })
        || ctx.source_target_kinds.iter().any(|kind| {
            matches!(
                kind,
                SourceTargetKind::Test | SourceTargetKind::Example | SourceTargetKind::Bench
            )
        })
        || ctx.ast.as_ref().is_some_and(|ast| test_only(&ast.attrs))
}

fn measured_statement(stmt: &syn::Stmt) -> bool {
    let tokens = stmt.to_token_stream().into_iter().collect::<Vec<_>>();
    for pair in tokens.chunks_exact(2) {
        let (TokenTree::Punct(pound), TokenTree::Group(group)) = (&pair[0], &pair[1]) else {
            break;
        };
        if pound.as_char() != '#' || group.delimiter() != Delimiter::Bracket {
            break;
        }
        if let Ok(meta) = syn::parse2::<syn::Meta>(group.stream()) {
            if meta.path().segments.last().is_some_and(|segment| {
                segment.ident == "block" || segment.ident == "extrinsic_call"
            }) {
                return true;
            }
        }
    }
    false
}

fn called_names(
    tokens: TokenStream,
    benchmark: &str,
    bindings: &HashMap<String, String>,
) -> HashSet<String> {
    struct Calls<'a> {
        names: HashSet<String>,
        benchmark: &'a str,
        bindings: &'a HashMap<String, String>,
    }
    impl<'ast> Visit<'ast> for Calls<'_> {
        fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
            if matches!(
                call.method.to_string().as_str(),
                "dispatch" | "dispatch_bypass_filter"
            ) {
                if let syn::Expr::Path(receiver) = &*call.receiver {
                    if let Some(name) = receiver
                        .path
                        .get_ident()
                        .and_then(|id| self.bindings.get(&id.to_string()))
                    {
                        self.names.insert(name.clone());
                    }
                }
            }
            visit::visit_expr_method_call(self, call);
        }
        fn visit_expr_struct(&mut self, expr: &'ast syn::ExprStruct) {
            if expr
                .path
                .segments
                .iter()
                .any(|s| s.ident.to_string().ends_with("Call"))
            {
                if let Some(name) = path_last_ident(&expr.path) {
                    self.names.insert(name);
                }
            }
            visit::visit_expr_struct(self, expr);
        }

        fn visit_macro(&mut self, mac: &'ast syn::Macro) {
            if mac.path.segments.last().is_some_and(|s| {
                matches!(
                    s.ident.to_string().as_str(),
                    "assert_ok" | "assert_err" | "assert_noop"
                )
            }) {
                if let Ok(inner) = syn::parse2::<syn::Expr>(mac.tokens.clone()) {
                    self.visit_expr(&inner);
                }
            }
        }
        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            match &*call.func {
                syn::Expr::Infer(_) => {
                    self.names.insert(self.benchmark.to_string());
                }
                syn::Expr::Path(path) => {
                    if let Some(name) = path_last_ident(&path.path) {
                        self.names.insert(name);
                    }
                }
                _ => {}
            }
            visit::visit_expr_call(self, call);
        }
    }
    let mut calls = Calls {
        names: HashSet::new(),
        benchmark,
        bindings,
    };
    if let Ok(block) = syn::parse2::<syn::Block>(quote::quote!({ #tokens })) {
        calls.visit_block(&block);
    }
    calls.names
}

// Resolve only simple local Call bindings that are later dispatched in the
// measured block. Setup calls themselves are never benchmark coverage.
fn call_bindings(tokens: TokenStream) -> HashMap<String, String> {
    let tokens = tokens.into_iter().collect::<Vec<_>>();
    let mut bindings = HashMap::new();
    for start in 0..tokens.len() {
        if !matches!(&tokens[start], TokenTree::Ident(id) if id == "let") {
            continue;
        }
        let Some(end) = (start..tokens.len())
            .find(|&i| matches!(&tokens[i], TokenTree::Punct(p) if p.as_char() == ';'))
        else {
            continue;
        };
        let local: TokenStream = tokens[start..=end].iter().cloned().collect();
        let Ok(block) = syn::parse2::<syn::Block>(quote::quote!({ #local })) else {
            continue;
        };
        let Some(syn::Stmt::Local(local)) = block.stmts.first() else {
            continue;
        };
        let (syn::Pat::Ident(binding), Some(init)) = (&local.pat, &local.init) else {
            continue;
        };
        if let syn::Expr::Struct(expr) = &*init.expr {
            if expr
                .path
                .segments
                .iter()
                .any(|s| s.ident.to_string().ends_with("Call"))
            {
                if let Some(name) = path_last_ident(&expr.path) {
                    bindings.insert(binding.ident.to_string(), name);
                }
            }
        }
    }
    bindings
}

fn legacy_benchmarks(tokens: TokenStream) -> Vec<Benchmark> {
    let tokens = tokens.into_iter().collect::<Vec<_>>();
    let starts = tokens
        .windows(3)
        .enumerate()
        .filter_map(|(index, window)| {
            let [TokenTree::Ident(_), TokenTree::Group(setup), TokenTree::Punct(colon)] = window
            else {
                return None;
            };
            (setup.delimiter() == Delimiter::Brace && colon.as_char() == ':').then_some(index)
        })
        .collect::<Vec<_>>();
    let mut benchmarks = Vec::new();
    for (index, &start) in starts.iter().enumerate() {
        let name = tokens[start].to_string();
        let end = starts.get(index + 1).copied().unwrap_or(tokens.len());
        let measured = &tokens[start + 3..end];
        let verify = measured.windows(2).position(|pair| {
            let [TokenTree::Ident(id), TokenTree::Group(group)] = pair else {
                return false;
            };
            id == "verify" && group.delimiter() == Delimiter::Brace
        });
        let mut calls = HashSet::new();
        if let Some(TokenTree::Ident(call)) = measured.first() {
            calls.insert(if call == "_" {
                name.clone()
            } else {
                call.to_string()
            });
        } else if let Some(TokenTree::Group(block)) = measured.first() {
            let setup = match &tokens[start + 1] {
                TokenTree::Group(group) => group.stream(),
                _ => TokenStream::new(),
            };
            calls.extend(called_names(block.stream(), &name, &call_bindings(setup)));
        }
        let end_line = verify
            .map(|i| measured[i + 1].span().end().line)
            .unwrap_or_else(|| {
                measured
                    .iter()
                    .take_while(|token| !matches!(token, TokenTree::Punct(p) if p.as_char() == '#'))
                    .last()
                    .map(|token| token.span().end().line)
                    .unwrap_or_else(|| span_line(tokens[start].span()))
            });
        benchmarks.push(Benchmark {
            name,
            line: span_line(tokens[start].span()),
            end_line,
            calls,
            name_only: false,
            verification: verify
                .map(|i| measured[i..i + 2].iter().cloned().collect())
                .unwrap_or_default(),
        });
    }
    benchmarks
}

fn collect_benchmarks(content: &str) -> Vec<Benchmark> {
    struct Benchmarks {
        entries: Vec<Benchmark>,
    }
    impl<'ast> Visit<'ast> for Benchmarks {
        fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
            if !test_only(&module.attrs) {
                visit::visit_item_mod(self, module);
            }
        }
        fn visit_item_fn(&mut self, function: &'ast ItemFn) {
            if test_only(&function.attrs) {
                return;
            }
            if has_attr(&function.attrs, &["benchmark"]) {
                let name = function.sig.ident.to_string();
                let mut calls = HashSet::new();
                let mut verification = TokenStream::new();
                let has_measured = function.block.stmts.iter().any(measured_statement);
                let mut after_measurement = !has_measured;
                for stmt in &function.block.stmts {
                    if measured_statement(stmt) {
                        calls.extend(called_names(
                            stmt.to_token_stream(),
                            &name,
                            &call_bindings(
                                function
                                    .block
                                    .stmts
                                    .iter()
                                    .flat_map(ToTokens::to_token_stream)
                                    .collect(),
                            ),
                        ));
                        after_measurement = true;
                        verification = TokenStream::new();
                    } else if after_measurement {
                        verification.extend(stmt.to_token_stream());
                    }
                }
                // Legacy authored fixtures and block-only benchmark variants may
                // have no explicit dispatch marker. Preserve name coverage there.
                if !has_measured {
                    calls.insert(name.clone());
                }
                self.entries.push(Benchmark {
                    name,
                    line: span_line(function.sig.fn_token.span),
                    end_line: function.block.span().end().line,
                    calls,
                    verification,
                    name_only: !has_measured,
                });
            }
        }
        fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
            if !test_only(&item.attrs)
                && item.mac.path.segments.last().is_some_and(|s| {
                    matches!(
                        s.ident.to_string().as_str(),
                        "benchmarks" | "benchmarks_instance" | "benchmarks_instance_pallet"
                    )
                })
            {
                self.entries
                    .extend(legacy_benchmarks(item.mac.tokens.clone()));
            }
        }
    }
    let mut visitor = Benchmarks {
        entries: Vec::new(),
    };
    if let Some(ast) = parse_ast(content) {
        if !test_only(&ast.attrs) {
            visitor.visit_file(&ast);
        }
    }
    visitor.entries
}

fn has_verification(tokens: TokenStream, patterns: &[String]) -> bool {
    // Token trees exclude comments and keep string contents opaque. Turbofish
    // arguments and qualified helper calls do not change the terminal name.
    let tokens = tokens.into_iter().collect::<Vec<_>>();
    for (i, token) in tokens.iter().enumerate() {
        if let TokenTree::Ident(name) = token {
            let name = name.to_string();
            let accepted = patterns.iter().any(|pattern| pattern == &name)
                || matches!(
                    name.as_str(),
                    "assert"
                        | "assert_eq"
                        | "assert_ne"
                        | "assert_ok"
                        | "assert_noop"
                        | "assert_err"
                        | "ensure"
                        | "assert_last_event"
                        | "assert_has_event"
                        | "verify"
                );
            if accepted && tokens.get(i+1).is_some_and(|next| matches!(next, TokenTree::Group(_)) || matches!(next, TokenTree::Punct(p) if p.as_char() == '!' || p.as_char() == ':')) { return true; }
        }
        if let TokenTree::Group(group) = token {
            if has_verification(group.stream(), patterns) {
                return true;
            }
        }
    }
    false
}

fn manifest(path: &Path) -> Option<toml::Value> {
    std::fs::read_to_string(path).ok()?.parse().ok()
}

fn benchmark_sources(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            !entry.file_type().is_dir()
                || !matches!(
                    entry.file_name().to_str(),
                    Some("target" | ".git" | "tests" | "examples")
                )
        })
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "rs")
                && !matches!(
                    entry.file_name().to_str(),
                    Some("tests.rs" | "mock.rs" | "test.rs")
                )
        })
        .map(|entry| entry.into_path())
        .collect()
}

fn owned_benchmarks(path: &Path) -> Vec<Benchmark> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let crate_root = parent
        .ancestors()
        .find(|dir| dir.join("Cargo.toml").is_file());
    let mut roots = vec![crate_root
        .map(|dir| dir.join("src"))
        .unwrap_or_else(|| parent.to_path_buf())];
    if let Some(crate_root) = crate_root {
        if let Some(package_name) = manifest(&crate_root.join("Cargo.toml"))
            .and_then(|m| m.get("package")?.get("name")?.as_str().map(str::to_owned))
        {
            // Resolve companion crates through Cargo's declared path dependency,
            // never through a globally shared benchmark name.
            let workspace = crate_root
                .ancestors()
                .find(|dir| {
                    manifest(&dir.join("Cargo.toml"))
                        .is_some_and(|cargo| cargo.get("workspace").is_some())
                })
                .unwrap_or(crate_root);
            let deps = manifest(&workspace.join("Cargo.toml"))
                .and_then(|cargo| {
                    cargo
                        .get("workspace")?
                        .get("dependencies")?
                        .as_table()
                        .cloned()
                })
                .unwrap_or_default();
            let mut candidates = deps
                .values()
                .filter_map(|dep| dep.get("path")?.as_str().map(|p| workspace.join(p)))
                .collect::<Vec<_>>();
            // Nested companions (e.g. session/benchmarking) and standalone
            // fixture crates need not be listed as workspace dependencies.
            for base in [Some(crate_root), crate_root.parent()]
                .into_iter()
                .flatten()
            {
                if let Ok(entries) = std::fs::read_dir(base) {
                    candidates.extend(
                        entries
                            .flatten()
                            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                            .map(|e| e.path()),
                    );
                }
            }
            for candidate in candidates {
                let Some(companion) = manifest(&candidate.join("Cargo.toml")) else {
                    continue;
                };
                if !companion
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(toml::Value::as_str)
                    .is_some_and(|name| {
                        name == format!("{package_name}-benchmarking")
                            || (name.contains("benchmark")
                                && candidate.parent().and_then(|p| p.canonicalize().ok())
                                    == crate_root.canonicalize().ok())
                    })
                {
                    continue;
                }
                let owns = companion
                    .get("dependencies")
                    .and_then(toml::Value::as_table)
                    .is_some_and(|dependencies| {
                        dependencies.iter().any(|(alias, dependency)| {
                            let dep_package = dependency
                                .get("package")
                                .and_then(toml::Value::as_str)
                                .unwrap_or(alias);
                            if dep_package != package_name {
                                return false;
                            }
                            let declared =
                                if dependency.get("workspace").and_then(toml::Value::as_bool)
                                    == Some(true)
                                {
                                    deps.get(alias)
                                } else {
                                    Some(dependency)
                                };
                            let base = if dependency.get("workspace").and_then(toml::Value::as_bool)
                                == Some(true)
                            {
                                workspace
                            } else {
                                &candidate
                            };
                            declared
                                .and_then(|d| d.get("path"))
                                .and_then(toml::Value::as_str)
                                .is_some_and(|p| {
                                    base.join(p).canonicalize().ok()
                                        == crate_root.canonicalize().ok()
                                })
                        })
                    });
                if owns {
                    roots.push(candidate.join("src"));
                }
            }
        }
    }
    let mut files = HashSet::new();
    roots
        .into_iter()
        .flat_map(|root| benchmark_sources(&root))
        .filter(|file| files.insert(file.clone()))
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|content| collect_benchmarks(&content))
        .collect()
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
        if !ctx.is_rust || excluded_context(ctx) {
            return None;
        }

        // This rule works on weights.rs files to find weight functions
        // and cross-references benchmarks owned by the same pallet.
        let path_str = ctx.rel_path.to_string_lossy();
        if !path_str.ends_with("weights.rs") {
            return None;
        }

        let weight_fns = collect_weight_function_names(ctx.content);

        if weight_fns.is_empty() {
            return None;
        }

        let benchmark_names = owned_benchmarks(&ctx.path)
            .into_iter()
            .map(|bench| bench.name)
            .collect::<HashSet<_>>();

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
                        "Weight function `{}` has no corresponding benchmark in the pallet or its benchmarking crate",
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
        if !ctx.is_rust || excluded_context(ctx) {
            return None;
        }
        let mut diagnostics = Vec::new();
        for benchmark in collect_benchmarks(ctx.content) {
            if !has_verification(
                benchmark.verification,
                &config.benchmarking.verification_patterns,
            ) {
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    category: RuleCategory::Benchmark,
                    severity: config.rule_severity(self.id(), Severity::Warning),
                    file: ctx.path.clone(),
                    line: benchmark.line,
                    column: None,
                    end_line: Some(benchmark.end_line),
                    message: format!(
                        "Benchmark `{}` has no verification/postcondition check",
                        benchmark.name
                    ),
                    explanation: "Benchmarks should verify the expected state after the measured call or block.".to_string(),
                    suggestion: Some("Add postcondition assertions after the measured code, or a v1 `verify { ... }` block".to_string()),
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
        if !ctx.is_rust || excluded_context(ctx) || ctx.is_benchmark_file {
            return None;
        }

        let path_str = ctx.rel_path.to_string_lossy();
        if !path_str.ends_with("lib.rs") {
            return None;
        }
        let extrinsic_fns = collect_extrinsic_names(ctx.content);

        if extrinsic_fns.is_empty() {
            return None;
        }

        let benchmarks = owned_benchmarks(&ctx.path);
        let mut diagnostics = Vec::new();
        for (fn_name, line) in &extrinsic_fns {
            if !benchmarks.iter().any(|benchmark| {
                benchmark.calls.contains(fn_name)
                    || (benchmark.name_only
                        && benchmark
                            .name
                            .strip_prefix(fn_name)
                            .is_some_and(|suffix| suffix.starts_with('_')))
            }) {
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
                        "Extrinsic `{}` has no benchmark measuring it in the pallet or its benchmarking crate",
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
