use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, File as SynFile, ImplItem, ItemFn, ItemImpl, ItemTrait, TraitItem,
};

use crate::{
    config::Config,
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::{crate_root, scan_cached, FileContext},
    rules::LintRule,
    syn_util::{has_attr, path_last_ident, span_line},
};

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
    crate::engine::LintEngine::is_test_file(&ctx.classification_path, ctx.content)
        || crate_root(&ctx.path)
            .and_then(|root| ctx.path.strip_prefix(root).ok())
            .is_some_and(|relative| relative.starts_with("examples"))
        || ctx.source_target_kinds.iter().any(|kind| {
            matches!(
                kind,
                SourceTargetKind::Test | SourceTargetKind::Example | SourceTargetKind::Bench
            )
        })
        || ctx.ast.as_ref().is_some_and(|ast| test_only(&ast.attrs))
}

#[derive(Clone)]
struct Owners {
    modules: HashSet<String>,
    pallets: HashSet<String>,
    calls: HashSet<String>,
}

impl Owners {
    fn new(modules: HashSet<String>) -> Self {
        Self {
            modules,
            pallets: ["Pallet", "Self"].into_iter().map(str::to_owned).collect(),
            calls: ["Call"].into_iter().map(str::to_owned).collect(),
        }
    }

    fn owns_type(&self, path: &syn::Path, call: bool) -> bool {
        let parts = path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>();
        let names = if call { &self.calls } else { &self.pallets };
        let canonical = if call { "Call" } else { "Pallet" };
        match parts.as_slice() {
            [name] => names.contains(name),
            [module, name] => {
                self.modules.contains(module) && (name == canonical || names.contains(name))
            }
            [module, pallet, name] => {
                self.modules.contains(module)
                    && pallet == "pallet"
                    && (name == canonical || names.contains(name))
            }
            _ => false,
        }
    }

    fn variant(&self, path: &syn::Path) -> Option<String> {
        let mut receiver = path.clone();
        let name = receiver.segments.pop()?.into_value().ident.to_string();
        self.owns_type(&receiver, true).then_some(name)
    }

    fn in_block(&self, block: &syn::Block) -> Self {
        self.in_scope(block.stmts.iter().filter_map(|stmt| match stmt {
            syn::Stmt::Item(item) => Some(item),
            _ => None,
        }))
    }

    fn in_scope<'a>(&self, items: impl IntoIterator<Item = &'a syn::Item>) -> Self {
        fn imports(tree: &syn::UseTree, prefix: Vec<String>, out: &mut Vec<(syn::Path, String)>) {
            match tree {
                syn::UseTree::Path(p) => {
                    let mut prefix = prefix;
                    prefix.push(p.ident.to_string());
                    imports(&p.tree, prefix, out);
                }
                syn::UseTree::Group(g) => {
                    for t in &g.items {
                        imports(t, prefix.clone(), out);
                    }
                }
                syn::UseTree::Name(n) => {
                    let mut prefix = prefix;
                    prefix.push(n.ident.to_string());
                    if let Ok(path) = syn::parse_str(&prefix.join("::")) {
                        out.push((path, n.ident.to_string()));
                    }
                }
                syn::UseTree::Rename(r) => {
                    let mut prefix = prefix;
                    prefix.push(r.ident.to_string());
                    if let Ok(path) = syn::parse_str(&prefix.join("::")) {
                        out.push((path, r.rename.to_string()));
                    }
                }
                _ => {}
            }
        }
        let mut aliases = Vec::new();
        for item in items {
            match item {
                syn::Item::Use(u) if !test_only(&u.attrs) => {
                    imports(&u.tree, Vec::new(), &mut aliases)
                }
                syn::Item::Type(t) if !test_only(&t.attrs) => {
                    if let syn::Type::Path(p) = &*t.ty {
                        if p.qself.is_none() {
                            aliases.push((p.path.clone(), t.ident.to_string()));
                        }
                    }
                }
                _ => {}
            }
        }
        let mut owners = self.clone();
        // Explicit imports/type aliases can shadow inherited names, including
        // the conventional Pallet and Call names.
        for (path, alias) in &aliases {
            if !self.owns_type(path, false) {
                owners.pallets.remove(alias);
            }
            if !self.owns_type(path, true) {
                owners.calls.remove(alias);
            }
        }
        // Resolve chains of local aliases independently of declaration order.
        for _ in 0..=aliases.len() {
            for (path, alias) in &aliases {
                if owners.owns_type(path, false) {
                    owners.pallets.insert(alias.clone());
                }
                if owners.owns_type(path, true) {
                    owners.calls.insert(alias.clone());
                }
            }
        }
        owners
    }
}

fn measured_statement(stmt: &syn::Stmt) -> Option<bool> {
    let tokens = stmt.to_token_stream().into_iter().collect::<Vec<_>>();
    for pair in tokens.chunks_exact(2) {
        let (TokenTree::Punct(pound), TokenTree::Group(group)) = (&pair[0], &pair[1]) else {
            break;
        };
        if pound.as_char() != '#' || group.delimiter() != Delimiter::Bracket {
            break;
        }
        if let Ok(meta) = syn::parse2::<syn::Meta>(group.stream()) {
            match meta
                .path()
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .as_deref()
            {
                Some("extrinsic_call") => return Some(true),
                Some("block") => return Some(false),
                _ => {}
            }
        }
    }
    None
}

fn called_names(
    tokens: TokenStream,
    benchmark: &str,
    bindings: &HashMap<String, String>,
    owners: &Owners,
    extrinsic_marker: bool,
) -> HashSet<String> {
    struct Calls<'a> {
        names: HashSet<String>,
        benchmark: &'a str,
        bindings: HashMap<String, String>,
        owners: Owners,
        extrinsic_marker: bool,
    }
    impl<'ast> Visit<'ast> for Calls<'_> {
        fn visit_block(&mut self, block: &'ast syn::Block) {
            let previous = self.owners.clone();
            self.owners = self.owners.in_block(block);
            visit::visit_block(self, block);
            self.owners = previous;
        }
        fn visit_local(&mut self, local: &'ast syn::Local) {
            visit::visit_local(self, local);
            bind_call(local, &self.owners, &mut self.bindings);
        }
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
        fn visit_macro(&mut self, mac: &'ast syn::Macro) {
            if mac.path.segments.last().is_some_and(|s| {
                matches!(
                    s.ident.to_string().as_str(),
                    "assert_ok"
                        | "assert_err"
                        | "assert_noop"
                        | "assert"
                        | "assert_eq"
                        | "assert_ne"
                        | "debug_assert"
                        | "debug_assert_eq"
                        | "debug_assert_ne"
                )
            }) {
                use syn::parse::Parser;
                if let Ok(args) =
                    syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                        .parse2(mac.tokens.clone())
                {
                    for arg in args {
                        self.visit_expr(&arg);
                    }
                }
            }
        }
        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            match &*call.func {
                syn::Expr::Infer(_) => {
                    self.names.insert(self.benchmark.to_string());
                }
                syn::Expr::Path(path) => {
                    let mut receiver = path.path.clone();
                    let name = receiver
                        .segments
                        .pop()
                        .map(|p| p.into_value().ident.to_string());
                    if path.qself.is_none()
                        && (self.owners.owns_type(&receiver, false)
                            || (self.extrinsic_marker && path.path.segments.len() == 1))
                    {
                        self.names.extend(name);
                    }
                }
                _ => {}
            }
            let marker = self.extrinsic_marker;
            self.extrinsic_marker = false;
            visit::visit_expr_call(self, call);
            self.extrinsic_marker = marker;
        }
    }
    let mut calls = Calls {
        names: HashSet::new(),
        benchmark,
        bindings: bindings.clone(),
        owners: owners.clone(),
        extrinsic_marker,
    };
    if let Ok(block) = syn::parse2::<syn::Block>(quote::quote!({ #tokens })) {
        calls.visit_block(&block);
    }
    calls.names
}

// Keep only the latest simple binding, in statement order. Setup calls
// themselves are never benchmark coverage.
fn bind_call(local: &syn::Local, owners: &Owners, bindings: &mut HashMap<String, String>) {
    let syn::Pat::Ident(binding) = &local.pat else {
        return;
    };
    let ident = binding.ident.to_string();
    bindings.remove(&ident);
    if let Some(init) = &local.init {
        if let syn::Expr::Struct(expr) = &*init.expr {
            if let Some(name) = owners.variant(&expr.path) {
                bindings.insert(ident, name);
            }
        }
    }
}

fn legacy_benchmarks(tokens: TokenStream, owners: &Owners) -> Vec<Benchmark> {
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
            let mut bindings = HashMap::new();
            // v1 setup may contain non-Rust `let c in 0 .. 10` components.
            let setup = setup.into_iter().collect::<Vec<_>>();
            for stmt in setup.split(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ';')) {
                let stmt: TokenStream = stmt.iter().cloned().collect();
                if let Ok(block) = syn::parse2::<syn::Block>(quote::quote!({ #stmt; })) {
                    if let Some(syn::Stmt::Local(local)) = block.stmts.first() {
                        bind_call(local, owners, &mut bindings);
                    }
                }
            }
            calls.extend(called_names(
                block.stream(),
                &name,
                &bindings,
                owners,
                false,
            ));
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
                .map(|i| match &measured[i + 1] {
                    TokenTree::Group(group) if !group.stream().is_empty() => {
                        measured[i..i + 2].iter().cloned().collect()
                    }
                    _ => TokenStream::new(),
                })
                .unwrap_or_default(),
        });
    }
    benchmarks
}

fn collect_benchmarks(content: &str) -> Vec<Benchmark> {
    parse_ast(content)
        .map(|ast| collect_benchmarks_ast(&ast, &Owners::new(HashSet::new())))
        .unwrap_or_default()
}

fn collect_benchmarks_ast(ast: &SynFile, owners: &Owners) -> Vec<Benchmark> {
    struct Benchmarks {
        entries: Vec<Benchmark>,
        owners: Owners,
    }
    impl<'ast> Visit<'ast> for Benchmarks {
        fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
            if !test_only(&module.attrs) {
                let previous = self.owners.clone();
                if let Some((_, items)) = &module.content {
                    self.owners = self.owners.in_scope(items);
                }
                visit::visit_item_mod(self, module);
                self.owners = previous;
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
                let owners = self.owners.in_block(&function.block);
                let mut bindings = HashMap::new();
                let has_measured = function
                    .block
                    .stmts
                    .iter()
                    .any(|s| measured_statement(s).is_some());
                let mut after_measurement = !has_measured;
                for stmt in &function.block.stmts {
                    if let Some(extrinsic_marker) = measured_statement(stmt) {
                        calls.extend(called_names(
                            stmt.to_token_stream(),
                            &name,
                            &bindings,
                            &owners,
                            extrinsic_marker,
                        ));
                        after_measurement = true;
                        verification = TokenStream::new();
                    } else if after_measurement {
                        verification.extend(stmt.to_token_stream());
                    }
                    if let syn::Stmt::Local(local) = stmt {
                        bind_call(local, &owners, &mut bindings);
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
                    .extend(legacy_benchmarks(item.mac.tokens.clone(), &self.owners));
            }
        }
    }
    let mut visitor = Benchmarks {
        entries: Vec::new(),
        owners: owners.in_scope(&ast.items),
    };
    if !test_only(&ast.attrs) {
        visitor.visit_file(ast);
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
            if accepted
                && tokens.get(i + 1).is_some_and(|next| {
                    matches!(next, TokenTree::Group(_))
                        || matches!(next, TokenTree::Punct(p) if p.as_char() == '!' || p.as_char() == ':')
                })
            {
                return true;
            }
        }
        if let TokenTree::Group(group) = token {
            if has_verification(group.stream(), patterns) {
                return true;
            }
        }
    }
    false
}

fn manifest(path: &Path) -> Option<Rc<toml::Value>> {
    scan_cached(path.to_path_buf(), |path| {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.parse().ok())
            .map(Rc::new)
    })
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

fn discover_benchmarks(path: &Path) -> Discovery {
    let Some(parent) = path.parent() else {
        return Discovery::default();
    };
    let crate_root = crate_root(path);
    let mut roots = vec![crate_root.unwrap_or(parent).to_path_buf()];
    let mut modules = ["crate", "self", "super"]
        .into_iter()
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    if let Some(crate_root) = crate_root {
        if let Some(package_name) = manifest(&crate_root.join("Cargo.toml"))
            .and_then(|m| m.get("package")?.get("name")?.as_str().map(str::to_owned))
        {
            modules.insert(package_name.replace('-', "_"));
            // Find companion crates by directory scan, exact package name, and a
            // declared dependency back on this crate.
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
            // frame-election-provider-support's companion covers phragmen/phragmms.
            let companion_name = if package_name == "frame-election-provider-support" {
                "pallet-election-provider-support-benchmarking".to_owned()
            } else {
                format!("{package_name}-benchmarking")
            };
            let mut candidates = Vec::new();
            // Nested companions (e.g. session/benchmarking) and sibling crates.
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
                if companion
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(toml::Value::as_str)
                    .is_none_or(|name| name != companion_name)
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
                    if let Some(dependencies) = companion
                        .get("dependencies")
                        .and_then(toml::Value::as_table)
                    {
                        for (alias, dep) in dependencies {
                            if dep
                                .get("package")
                                .and_then(toml::Value::as_str)
                                .unwrap_or(alias)
                                == package_name
                            {
                                modules.insert(alias.replace('-', "_"));
                            }
                        }
                    }
                    roots.push(candidate);
                }
            }
        }
    }
    let mut discovery = Discovery::default();
    let owners = Owners::new(modules);
    for root in roots {
        if crate_root.is_some() {
            let cargo = manifest(&root.join("Cargo.toml"));
            let lib = cargo
                .as_ref()
                .and_then(|c| c.get("lib")?.get("path")?.as_str())
                .unwrap_or("src/lib.rs");
            let lib = root.join(lib);
            discover_module(&lib, Some(&lib), &owners, &mut discovery);
        } else {
            // Standalone source snippets have no Cargo module root.
            for file in benchmark_sources(&root) {
                discover_module(&file, None, &owners, &mut discovery);
            }
        }
    }
    discovery
}

#[derive(Default)]
struct Discovery {
    files: HashSet<PathBuf>,
    benchmarks: Vec<Benchmark>,
}

impl Discovery {
    fn contains_source(&self, path: &Path) -> bool {
        path.canonicalize()
            .ok()
            .is_some_and(|path| self.files.contains(&path))
    }
}

fn discover_module(
    file: &Path,
    crate_root_file: Option<&Path>,
    owners: &Owners,
    discovery: &mut Discovery,
) {
    let is_crate_root = Some(file) == crate_root_file;
    let Ok(canonical) = file.canonicalize() else {
        return;
    };
    let file = canonical.as_path();
    if discovery.files.contains(file) {
        return;
    }
    let Some(ast) = std::fs::read_to_string(file)
        .ok()
        .and_then(|s| parse_ast(&s))
    else {
        return;
    };
    if test_only(&ast.attrs) {
        return;
    }
    discovery.files.insert(file.to_path_buf());
    discovery
        .benchmarks
        .extend(collect_benchmarks_ast(&ast, owners));
    let parent = file.parent().unwrap_or(Path::new("."));
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let module_dir = if is_crate_root || matches!(stem, "lib" | "main" | "mod") {
        parent.to_path_buf()
    } else {
        parent.join(stem)
    };
    discover_children(&ast.items, &module_dir, parent, owners, discovery);
}

fn discover_children(
    items: &[syn::Item],
    dir: &Path,
    path_base: &Path,
    owners: &Owners,
    discovery: &mut Discovery,
) {
    let owners = owners.in_scope(items);
    for item in items {
        let syn::Item::Mod(module) = item else {
            continue;
        };
        if test_only(&module.attrs) {
            continue;
        }
        let explicit = module.attrs.iter().find_map(|a| {
            if !a.path().is_ident("path") {
                return None;
            }
            let syn::Meta::NameValue(value) = &a.meta else {
                return None;
            };
            let syn::Expr::Lit(lit) = &value.value else {
                return None;
            };
            let syn::Lit::Str(path) = &lit.lit else {
                return None;
            };
            Some(path.value())
        });
        if let Some((_, items)) = &module.content {
            let child_dir = explicit
                .map(|p| dir.join(p))
                .unwrap_or_else(|| dir.join(module.ident.to_string()));
            discover_children(items, &child_dir, &child_dir, &owners, discovery);
        } else {
            let file = explicit.map(|p| path_base.join(p)).unwrap_or_else(|| {
                let flat = dir.join(format!("{}.rs", module.ident));
                if flat.is_file() {
                    flat
                } else {
                    dir.join(module.ident.to_string()).join("mod.rs")
                }
            });
            discover_module(&file, None, &owners, discovery);
        }
    }
}

fn owned_benchmarks(path: &Path) -> Rc<Discovery> {
    let key = crate_root(path)
        .or_else(|| path.parent())
        .unwrap_or(path)
        .to_path_buf();
    scan_cached(key, |_| Rc::new(discover_benchmarks(path)))
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

        let discovery = owned_benchmarks(&ctx.path);
        if crate_root(&ctx.path).is_some() && !discovery.contains_source(&ctx.path) {
            return None;
        }
        let benchmark_names = discovery
            .benchmarks
            .iter()
            .map(|bench| &bench.name)
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
        let benchmarks = ctx
            .ast
            .as_ref()
            .map(|ast| collect_benchmarks_ast(ast, &Owners::new(HashSet::new())))
            .unwrap_or_else(|| collect_benchmarks(ctx.content));
        if benchmarks.is_empty() {
            return None;
        }
        if crate_root(&ctx.path).is_some()
            && !owned_benchmarks(&ctx.path).contains_source(&ctx.path)
        {
            return None;
        }
        let mut diagnostics = Vec::new();
        for benchmark in benchmarks {
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

        let discovery = owned_benchmarks(&ctx.path);
        if crate_root(&ctx.path).is_some() && !discovery.contains_source(&ctx.path) {
            return None;
        }
        let mut diagnostics = Vec::new();
        for (fn_name, line) in &extrinsic_fns {
            if !discovery.benchmarks.iter().any(|benchmark| {
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
