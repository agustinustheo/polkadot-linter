//! Runtime configuration rules (RTC001-RTC005).
//!
//! These rules inspect the *runtime* crate rather than individual pallets:
//! `impl <pallet>::Config for Runtime` blocks, `construct_runtime!` /
//! `#[frame_support::runtime]` declarations, `define_benchmarks!`,
//! `parameter_types!` and the XCM executor configuration. Every pattern they
//! report was raised as a finding in at least one public Security Research
//! Labs audit of a Polkadot SDK runtime.
//!
//! Scope is deliberately narrow:
//!
//! - a file is only inspected when it belongs to a *runtime crate*: the
//!   nearest Cargo package whose `src/lib.rs` declares the runtime with
//!   `construct_runtime!`, `#[frame_support::runtime]` or
//!   `#[frame_construct_runtime]`. Files that are not under any Cargo
//!   manifest fall back to inspecting the file itself;
//! - test code is always skipped: `#[cfg(test)]` items, `mock`/`tests`/
//!   `benches` paths, and test runtime crates (package or directory names
//!   containing `test` or `mock`);
//! - dev runtimes listed in `[runtime_config].dev_runtime_packages` are
//!   skipped unless `include_dev_runtimes = true`.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, Expr, File as SynFile, ImplItem, ImplItemType, Item, ItemConst, ItemImpl, ItemMacro,
    ItemMod, ItemType, Lit, Macro, Meta, Type, TypePath,
};
use walkdir::WalkDir;

use crate::{
    config::{Config, RuntimeConfigConfig},
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    rules::LintRule,
};

// ---------------------------------------------------------------------------
// Runtime crate discovery and scope
// ---------------------------------------------------------------------------

/// What the rules know about the Cargo package that owns a source file.
#[derive(Debug, Default)]
struct RuntimeCrate {
    /// Cargo package name, when the file belongs to a Cargo package.
    package: Option<String>,
    /// Package root directory (the directory holding `Cargo.toml`).
    root: Option<PathBuf>,
    /// The package's `src/lib.rs` declares a FRAME runtime.
    is_runtime: bool,
    /// Benchmark keys collected from every `define_benchmarks!` invocation in
    /// the package's non-test sources. `None` when the runtime has no
    /// `define_benchmarks!` at all.
    benchmarks: Option<HashSet<String>>,
}

fn crate_cache() -> &'static Mutex<HashMap<PathBuf, Arc<RuntimeCrate>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<RuntimeCrate>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Locate the Cargo package owning `path`, or fall back to the file itself
/// when no manifest exists (single-file fixtures).
fn runtime_crate_for(ctx: &FileContext) -> Arc<RuntimeCrate> {
    let manifest_dir = ctx
        .path
        .ancestors()
        .skip(1)
        .filter(|dir| !dir.as_os_str().is_empty())
        .find(|dir| dir.join("Cargo.toml").is_file())
        .map(Path::to_path_buf);

    let Some(manifest_dir) = manifest_dir else {
        let ast = ctx.ast.as_ref();
        let is_runtime = ast.is_some_and(declares_runtime);
        let benchmarks = ast.and_then(|file| {
            let mut keys = HashSet::new();
            collect_define_benchmarks(file, &mut keys).then_some(keys)
        });
        return Arc::new(RuntimeCrate {
            package: None,
            root: None,
            is_runtime,
            benchmarks,
        });
    };

    let mut cache = crate_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    cache
        .entry(manifest_dir.clone())
        .or_insert_with(|| Arc::new(load_runtime_crate(&manifest_dir)))
        .clone()
}

fn load_runtime_crate(root: &Path) -> RuntimeCrate {
    let package = fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|manifest| manifest.parse::<toml::Table>().ok())
        .and_then(|manifest| {
            manifest
                .get("package")?
                .get("name")?
                .as_str()
                .map(ToOwned::to_owned)
        });

    // A manifest without a package (a virtual workspace) does not own the file
    // as a crate source; the file is an orphan and is not a runtime.
    if package.is_none() {
        return RuntimeCrate {
            root: Some(root.to_path_buf()),
            ..RuntimeCrate::default()
        };
    }

    let is_runtime = fs::read_to_string(root.join("src/lib.rs"))
        .ok()
        .and_then(|content| syn::parse_file(&content).ok())
        .is_some_and(|file| declares_runtime(&file));

    let benchmarks = if is_runtime {
        let mut keys = HashSet::new();
        let mut found = false;
        for entry in WalkDir::new(root.join("src"))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs")
                || is_test_path(path.strip_prefix(root).unwrap_or(path))
            {
                continue;
            }
            let Some(file) = fs::read_to_string(path)
                .ok()
                .and_then(|content| syn::parse_file(&content).ok())
            else {
                continue;
            };
            found |= collect_define_benchmarks(&file, &mut keys);
        }
        found.then_some(keys)
    } else {
        None
    };

    RuntimeCrate {
        package,
        root: Some(root.to_path_buf()),
        is_runtime,
        benchmarks,
    }
}

/// `construct_runtime!`, `#[frame_support::runtime]` or
/// `#[frame_construct_runtime]` at any module level of the file.
fn declares_runtime(file: &SynFile) -> bool {
    struct Finder(bool);
    impl<'ast> Visit<'ast> for Finder {
        fn visit_item_macro(&mut self, item: &'ast ItemMacro) {
            if last_ident(&item.mac.path).as_deref() == Some("construct_runtime") {
                self.0 = true;
            }
        }
        fn visit_item_mod(&mut self, item: &'ast ItemMod) {
            if item.attrs.iter().any(|attr| {
                let segments = attr
                    .path()
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>();
                matches!(
                    segments.last().map(String::as_str),
                    Some("frame_construct_runtime")
                ) || (segments.len() >= 2 && segments.last().map(String::as_str) == Some("runtime"))
            }) {
                self.0 = true;
            }
            visit::visit_item_mod(self, item);
        }
    }
    let mut finder = Finder(false);
    finder.visit_file(file);
    finder.0
}

/// Path components that mark test, mock, or benchmark-only sources.
fn is_test_component(component: &str) -> bool {
    let lower = component.to_ascii_lowercase();
    let stem = lower.strip_suffix(".rs").unwrap_or(&lower);
    matches!(stem, "test" | "tests" | "benches" | "mock" | "mocks")
        || stem.contains("mock")
        || stem.contains("-test")
        || stem.contains("test-")
        || stem.contains("_test")
        || stem.starts_with("test_")
}

fn is_test_path(path: &Path) -> bool {
    path.components()
        .any(|component| is_test_component(&component.as_os_str().to_string_lossy()))
}

/// Package or directory names that identify a test-only runtime crate.
fn is_test_package(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("test") || lower.contains("mock")
}

/// Shared gate: the file is non-test source of a production (or opted-in dev)
/// runtime crate.
fn runtime_scope(ctx: &FileContext, config: &RuntimeConfigConfig) -> Option<Arc<RuntimeCrate>> {
    if !ctx.is_rust || ctx.is_test_file || ctx.is_benchmark_file || ctx.ast.is_none() {
        return None;
    }
    if is_test_path(&ctx.rel_path) {
        return None;
    }

    let krate = runtime_crate_for(ctx);
    if !krate.is_runtime {
        return None;
    }
    if let Some(root) = &krate.root {
        let in_crate = ctx.path.strip_prefix(root).unwrap_or(&ctx.path);
        if is_test_path(in_crate) {
            return None;
        }
        let dir_name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_test_package(&dir_name) {
            return None;
        }
    }
    if let Some(package) = &krate.package {
        if is_test_package(package) {
            return None;
        }
        if !config.include_dev_runtimes && config.dev_runtime_packages.contains(package) {
            return None;
        }
    }
    Some(krate)
}

// ---------------------------------------------------------------------------
// Non-production cfg filtering
// ---------------------------------------------------------------------------

const NON_PRODUCTION_FEATURES: &[&str] = &["runtime-benchmarks", "try-runtime", "test-helpers"];

fn cfg_is_non_production(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::NameValue(name_value) => {
            name_value.path.is_ident("feature")
                && matches!(
                    &name_value.value,
                    Expr::Lit(expr) if matches!(
                        &expr.lit,
                        Lit::Str(feature) if NON_PRODUCTION_FEATURES.contains(&feature.value().as_str())
                    )
                )
        }
        Meta::List(list) => {
            let Ok(nested) = list.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return false;
            };
            if list.path.is_ident("all") {
                nested.iter().any(cfg_is_non_production)
            } else if list.path.is_ident("any") {
                !nested.is_empty() && nested.iter().all(cfg_is_non_production)
            } else {
                false
            }
        }
    }
}

fn is_non_production(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        if !attr.path().is_ident("cfg") {
            return false;
        }
        attr.parse_args::<Meta>()
            .is_ok_and(|meta| cfg_is_non_production(&meta))
    })
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn impl_item_attrs(item: &ImplItem) -> &[Attribute] {
    match item {
        ImplItem::Const(item) => &item.attrs,
        ImplItem::Fn(item) => &item.attrs,
        ImplItem::Type(item) => &item.attrs,
        ImplItem::Macro(item) => &item.attrs,
        _ => &[],
    }
}

/// Clone the file without `#[cfg(test)]`, `#[test]` and benchmark/try-runtime
/// gated items, so rules only see production runtime configuration.
fn production_ast(file: &SynFile) -> SynFile {
    fn strip_items(items: &mut Vec<Item>) {
        items.retain(|item| !is_non_production(item_attrs(item)));
        for item in items {
            match item {
                Item::Mod(module) => {
                    if let Some((_, items)) = &mut module.content {
                        strip_items(items);
                    }
                }
                Item::Impl(item_impl) => {
                    item_impl
                        .items
                        .retain(|item| !is_non_production(impl_item_attrs(item)));
                }
                _ => {}
            }
        }
    }
    let mut file = file.clone();
    if is_non_production(&file.attrs) {
        file.items.clear();
    }
    strip_items(&mut file.items);
    file
}

// ---------------------------------------------------------------------------
// Small syntax helpers
// ---------------------------------------------------------------------------

fn last_ident(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn path_idents(path: &syn::Path) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect()
}

fn line_of<T: Spanned>(node: &T) -> usize {
    node.span().start().line
}

fn is_unit_type(ty: &Type) -> bool {
    matches!(ty, Type::Tuple(tuple) if tuple.elems.is_empty())
}

fn type_path(ty: &Type) -> Option<&syn::Path> {
    match ty {
        Type::Path(TypePath { qself: None, path }) => Some(path),
        _ => None,
    }
}

fn tokens_contain_ident(tokens: TokenStream, needles: &[&str]) -> bool {
    tokens.into_iter().any(|tree| match tree {
        TokenTree::Ident(ident) => needles.iter().any(|needle| ident == needle),
        TokenTree::Group(group) => tokens_contain_ident(group.stream(), needles),
        _ => false,
    })
}

/// A `pallet::Config` (or `pallet::Config<Instance>`) implementation for the
/// runtime type. Returns the trait path idents.
fn runtime_config_impl(item_impl: &ItemImpl) -> Option<Vec<String>> {
    let (_, trait_path, _) = item_impl.trait_.as_ref()?;
    if last_ident(trait_path).as_deref() != Some("Config") || trait_path.segments.len() < 2 {
        return None;
    }
    let self_ident = type_path(&item_impl.self_ty).and_then(last_ident)?;
    (self_ident == "Runtime").then(|| path_idents(trait_path))
}

fn impl_types(item_impl: &ItemImpl) -> impl Iterator<Item = &ImplItemType> {
    item_impl.items.iter().filter_map(|item| match item {
        ImplItem::Type(item_type) => Some(item_type),
        _ => None,
    })
}

fn impl_type_named<'a>(item_impl: &'a ItemImpl, name: &str) -> Option<&'a ImplItemType> {
    impl_types(item_impl).find(|item_type| item_type.ident == name)
}

fn is_weight_info_ident(ident: &str) -> bool {
    ident.ends_with("WeightInfo")
}

fn literal_is_zero(literal: &str) -> bool {
    const INTEGER_SUFFIXES: &[&str] = &[
        "u128", "u64", "u32", "u16", "u8", "usize", "i128", "i64", "i32", "i16", "i8", "isize",
    ];
    let digits = literal.replace('_', "");
    let digits = INTEGER_SUFFIXES
        .iter()
        .find_map(|suffix| digits.strip_suffix(suffix))
        .unwrap_or(&digits);
    !digits.is_empty() && digits.chars().all(|ch| ch == '0')
}

fn expr_tokens_are_zero(tokens: &[TokenTree]) -> bool {
    match tokens {
        [TokenTree::Literal(literal)] => literal_is_zero(&literal.to_string()),
        [TokenTree::Group(group)] if group.delimiter() == Delimiter::Parenthesis => {
            let inner = group.stream().into_iter().collect::<Vec<_>>();
            expr_tokens_are_zero(&inner)
        }
        _ => false,
    }
}

fn expr_is_zero(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(expr) => {
            matches!(&expr.lit, Lit::Int(int) if int.base10_digits().chars().all(|ch| ch == '0'))
        }
        Expr::Paren(expr) => expr_is_zero(&expr.expr),
        Expr::Cast(expr) => expr_is_zero(&expr.expr),
        _ => false,
    }
}

/// `ConstU128<0>`, `ConstU64<0>`, `ConstU32<0>`, ...
fn const_type_is_zero(ty: &Type) -> bool {
    let Some(path) = type_path(ty) else {
        return false;
    };
    let Some(segment) = path.segments.last() else {
        return false;
    };
    if !segment.ident.to_string().starts_with("Const") {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    matches!(
        args.args.first(),
        Some(syn::GenericArgument::Const(expr)) if expr_is_zero(expr)
    ) || matches!(
        args.args.first(),
        Some(syn::GenericArgument::Type(Type::Verbatim(tokens)))
            if expr_tokens_are_zero(&tokens.clone().into_iter().collect::<Vec<_>>())
    )
}

fn diagnostic(
    rule: &dyn LintRule,
    ctx: &FileContext,
    severity: Severity,
    line: usize,
    message: String,
    explanation: &str,
    suggestion: &str,
) -> Diagnostic {
    Diagnostic {
        rule_id: rule.id().to_string(),
        rule_name: rule.name().to_string(),
        category: RuleCategory::RuntimeConfig,
        severity,
        file: ctx.path.clone(),
        line,
        column: None,
        end_line: None,
        message,
        explanation: explanation.to_string(),
        suggestion: Some(suggestion.to_string()),
    }
}

fn non_empty(diagnostics: Vec<Diagnostic>) -> Option<Vec<Diagnostic>> {
    (!diagnostics.is_empty()).then_some(diagnostics)
}

/// Visit every runtime `Config` impl in the production AST.
fn for_each_runtime_impl(file: &SynFile, mut f: impl FnMut(&ItemImpl, &[String])) {
    struct Impls<'a, F: FnMut(&ItemImpl, &[String])>(&'a mut F);
    impl<'ast, F: FnMut(&ItemImpl, &[String])> Visit<'ast> for Impls<'_, F> {
        fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
            if let Some(trait_path) = runtime_config_impl(item_impl) {
                (self.0)(item_impl, &trait_path);
            }
            visit::visit_item_impl(self, item_impl);
        }
    }
    Impls(&mut f).visit_file(file);
}

// ---------------------------------------------------------------------------
// RTC001: placeholder WeightInfo in a runtime Config impl
// ---------------------------------------------------------------------------

pub struct PlaceholderWeightInfoInRuntime;

impl LintRule for PlaceholderWeightInfoInRuntime {
    fn id(&self) -> &str {
        "RTC001"
    }
    fn name(&self) -> &str {
        "placeholder-weight-info-in-runtime"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        let settings = &config.runtime_config;
        runtime_scope(ctx, settings)?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Warning);
        let substrate_weight_severity = config
            .rules
            .severity
            .get(self.id())
            .and_then(|severity| severity.parse().ok())
            .or_else(|| settings.substrate_weight_severity.parse().ok())
            .unwrap_or(Severity::Advisory);

        let mut diagnostics = Vec::new();
        for_each_runtime_impl(&file, |item_impl, trait_path| {
            let pallet = trait_path[..trait_path.len() - 1].join("::");
            let allowlisted = trait_path[..trait_path.len() - 1]
                .iter()
                .any(|segment| settings.unit_weight_info_allowlist.contains(segment));

            for item_type in impl_types(item_impl) {
                let ident = item_type.ident.to_string();
                if !is_weight_info_ident(&ident) {
                    continue;
                }
                let line = line_of(&item_type.ty);
                if is_unit_type(&item_type.ty) {
                    if allowlisted {
                        continue;
                    }
                    diagnostics.push(diagnostic(
                        self,
                        ctx,
                        severity,
                        line,
                        format!("`{pallet}::Config for Runtime` sets `type {ident} = ();`"),
                        "The unit `()` implementation of a pallet's `WeightInfo` exists for tests and \
                         backwards compatibility. It is not benchmarked against this runtime, and for \
                         some pallets it returns zero weight, so extrinsics can be underweight and \
                         under-charged.",
                        "Add the pallet to `define_benchmarks!`, generate runtime weights with \
                         frame-omni-bencher, and bind `type WeightInfo = weights::<pallet>::WeightInfo<Runtime>;`.",
                    ));
                    continue;
                }
                let Some(path) = type_path(&item_type.ty) else {
                    continue;
                };
                let idents = path_idents(path);
                if idents
                    .iter()
                    .any(|segment| segment.starts_with("TestWeightInfo"))
                {
                    diagnostics.push(diagnostic(
                        self,
                        ctx,
                        severity,
                        line,
                        format!(
                            "`{pallet}::Config for Runtime` uses test weights `{}`",
                            path.to_token_stream().to_string().replace(' ', "")
                        ),
                        "`TestWeightInfo` types are placeholders for mocks and unit tests; they are \
                         not benchmarked and frequently return zero or arbitrary weights.",
                        "Benchmark the pallet in this runtime via `define_benchmarks!` and bind the \
                         generated `weights::<pallet>::WeightInfo<Runtime>`.",
                    ));
                    continue;
                }
                let upstream = !matches!(
                    idents.first().map(String::as_str),
                    Some("crate" | "self" | "super" | "weights")
                );
                if settings.flag_substrate_weight
                    && upstream
                    && idents.iter().any(|segment| segment == "SubstrateWeight")
                {
                    diagnostics.push(diagnostic(
                        self,
                        ctx,
                        substrate_weight_severity,
                        line,
                        format!(
                            "`{pallet}::Config for Runtime` uses upstream reference weights `{}`",
                            path.to_token_stream().to_string().replace(' ', "")
                        ),
                        "A pallet's `SubstrateWeight` was benchmarked on the SDK's reference runtime, \
                         not on this runtime's configuration, storage layout, or hardware profile.",
                        "Benchmark the pallet in this runtime and bind the generated weights, or \
                         document why the reference weights are acceptable.",
                    ));
                }
            }
        });
        non_empty(diagnostics)
    }
}

// ---------------------------------------------------------------------------
// RTC002: runtime-local weights bound for a pallet missing from define_benchmarks!
// ---------------------------------------------------------------------------

/// Record benchmark keys from every `define_benchmarks!` invocation in the
/// file. Returns whether any invocation was found.
fn collect_define_benchmarks(file: &SynFile, keys: &mut HashSet<String>) -> bool {
    struct Collector<'a> {
        keys: &'a mut HashSet<String>,
        found: bool,
    }
    impl<'ast> Visit<'ast> for Collector<'_> {
        fn visit_macro(&mut self, mac: &'ast Macro) {
            if last_ident(&mac.path).as_deref() == Some("define_benchmarks") {
                self.found = true;
                for tree in mac.tokens.clone() {
                    let TokenTree::Group(group) = tree else {
                        continue;
                    };
                    if group.delimiter() != Delimiter::Bracket {
                        continue;
                    }
                    let segments = group
                        .stream()
                        .into_iter()
                        .take_while(|tree| !matches!(tree, TokenTree::Punct(punct) if punct.as_char() == ','))
                        .filter_map(|tree| match tree {
                            TokenTree::Ident(ident) => Some(ident.to_string()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    if let Some(last) = segments.last() {
                        self.keys.insert(last.clone());
                        self.keys.insert(segments.join("_"));
                    }
                }
            }
            visit::visit_macro(self, mac);
        }
    }
    let mut collector = Collector { keys, found: false };
    collector.visit_file(file);
    collector.found
}

/// `weights::<module>::...` or `crate::weights::<module>::...`; returns the
/// module name.
fn local_weights_module(path: &syn::Path) -> Option<String> {
    let idents = path_idents(path);
    let start = match idents.first().map(String::as_str) {
        Some("weights") => 0,
        Some("crate" | "self" | "super")
            if idents.get(1).map(String::as_str) == Some("weights") =>
        {
            1
        }
        _ => return None,
    };
    // `weights::<module>::<Type>` needs at least a module and a type.
    (idents.len() >= start + 3).then(|| idents[start + 1].clone())
}

pub struct PalletMissingFromDefineBenchmarks;

impl LintRule for PalletMissingFromDefineBenchmarks {
    fn id(&self) -> &str {
        "RTC002"
    }
    fn name(&self) -> &str {
        "pallet-missing-from-define-benchmarks"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        let krate = runtime_scope(ctx, &config.runtime_config)?;
        let benchmarks = krate.benchmarks.as_ref()?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Warning);

        let mut diagnostics = Vec::new();
        for_each_runtime_impl(&file, |item_impl, trait_path| {
            let pallet_segments = &trait_path[..trait_path.len() - 1];
            for item_type in impl_types(item_impl) {
                if !is_weight_info_ident(&item_type.ident.to_string()) {
                    continue;
                }
                let Some(module) = type_path(&item_type.ty).and_then(local_weights_module) else {
                    continue;
                };
                let covered = benchmarks.contains(&module)
                    || pallet_segments
                        .iter()
                        .any(|segment| benchmarks.contains(segment))
                    || benchmarks.contains(&pallet_segments.join("_"));
                if covered {
                    continue;
                }
                diagnostics.push(diagnostic(
                    self,
                    ctx,
                    severity,
                    line_of(&item_type.ty),
                    format!(
                        "`{}::Config for Runtime` binds runtime weights `weights::{module}`, but the \
                         pallet is missing from `define_benchmarks!`",
                        pallet_segments.join("::"),
                    ),
                    "Runtime-local weight files are regenerated only for pallets listed in \
                     `define_benchmarks!`. A pallet missing from the list keeps stale weights after \
                     its code, its configuration, or the reference hardware changes.",
                    "Add `[<pallet>, <PalletName>]` to the runtime's `define_benchmarks!` block so \
                     the weight file is regenerated with the rest of the runtime.",
                ));
            }
        });
        non_empty(diagnostics)
    }
}

// ---------------------------------------------------------------------------
// RTC003: XCM delivery fees burned or waived
// ---------------------------------------------------------------------------

const DELIVERY_PRICE_TYPES: &[&str] = &[
    "PriceForSiblingDelivery",
    "PriceForParentDelivery",
    "PriceForChildParachainDelivery",
];

/// Routers whose third generic parameter is the delivery price.
const PRICED_ROUTERS: &[&str] = &["ParentAsUmp", "ChildParachainRouter"];

fn is_free_delivery_price(ty: &Type) -> bool {
    is_unit_type(ty)
        || type_path(ty)
            .and_then(last_ident)
            .is_some_and(|ident| ident == "NoPriceForMessageDelivery")
}

fn is_xcm_executor_impl(item_impl: &ItemImpl) -> bool {
    let Some((_, trait_path, _)) = &item_impl.trait_ else {
        return false;
    };
    if last_ident(trait_path).as_deref() != Some("Config") {
        return false;
    }
    path_idents(trait_path)
        .iter()
        .any(|segment| segment == "xcm_executor")
        || type_path(&item_impl.self_ty)
            .and_then(last_ident)
            .is_some_and(|ident| ident == "XcmConfig")
}

pub struct XcmFeesWaived;

impl LintRule for XcmFeesWaived {
    fn id(&self) -> &str {
        "RTC003"
    }
    fn name(&self) -> &str {
        "xcm-fees-waived"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        runtime_scope(ctx, &config.runtime_config)?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Error);

        struct Visitor<'a> {
            rule: &'a XcmFeesWaived,
            ctx: &'a FileContext<'a>,
            severity: Severity,
            diagnostics: Vec<Diagnostic>,
            lines: HashSet<usize>,
        }

        impl Visitor<'_> {
            fn push(&mut self, line: usize, message: String) {
                if !self.lines.insert(line) {
                    return;
                }
                self.diagnostics.push(diagnostic(
                    self.rule,
                    self.ctx,
                    self.severity,
                    line,
                    message,
                    "XCM delivery fees are the congestion control for cross-chain messaging. When \
                     they are not charged, any account can fill the outbound XCMP/UMP/DMP queues for \
                     free, delaying or dropping other users' messages and exhausting storage.",
                    "Use `XcmFeeManagerFromComponents<WaivedLocations, SendXcmFeeToAccount<..>>` for \
                     `FeeManager` and an `ExponentialPrice<..>` for delivery prices, as the SDK \
                     system parachains do.",
                ));
            }

            fn check_price(&mut self, ident: &str, ty: &Type) {
                if DELIVERY_PRICE_TYPES.contains(&ident) && is_free_delivery_price(ty) {
                    self.push(
                        line_of(ty),
                        format!(
                            "`{ident}` is `{}`, so XCM messages are delivered without a fee",
                            ty.to_token_stream().to_string().replace(' ', "")
                        ),
                    );
                }
            }
        }

        impl<'ast> Visit<'ast> for Visitor<'_> {
            fn visit_item_use(&mut self, _: &'ast syn::ItemUse) {}

            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                if is_xcm_executor_impl(item_impl) {
                    let sends = impl_type_named(item_impl, "XcmSender")
                        .is_none_or(|item| !is_unit_type(&item.ty));
                    let exports = impl_type_named(item_impl, "MessageExporter")
                        .is_none_or(|item| !is_unit_type(&item.ty));
                    if let Some(fee_manager) = impl_type_named(item_impl, "FeeManager") {
                        let waived = is_unit_type(&fee_manager.ty)
                            || type_path(&fee_manager.ty)
                                .and_then(last_ident)
                                .is_some_and(|ident| ident == "WaiveDeliveryFees");
                        if waived && (sends || exports) {
                            self.push(
                                line_of(&fee_manager.ty),
                                format!(
                                    "XCM executor `FeeManager` is `{}`: delivery and export fees are \
                                     never charged",
                                    fee_manager.ty.to_token_stream().to_string().replace(' ', "")
                                ),
                            );
                        }
                    }
                }
                for item_type in impl_types(item_impl) {
                    self.check_price(&item_type.ident.to_string(), &item_type.ty);
                }
                visit::visit_item_impl(self, item_impl);
            }

            fn visit_item_type(&mut self, item_type: &'ast ItemType) {
                self.check_price(&item_type.ident.to_string(), &item_type.ty);
                visit::visit_item_type(self, item_type);
            }

            fn visit_type_path(&mut self, type_path: &'ast TypePath) {
                if let Some(segment) = type_path.path.segments.last() {
                    let ident = segment.ident.to_string();
                    if PRICED_ROUTERS.contains(&ident.as_str()) {
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(price)) = args.args.iter().nth(2)
                            {
                                if is_free_delivery_price(price) {
                                    self.push(
                                        line_of(price),
                                        format!(
                                            "`{ident}` router is configured with delivery price `{}`",
                                            price.to_token_stream().to_string().replace(' ', "")
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
                visit::visit_type_path(self, type_path);
            }
        }

        let mut visitor = Visitor {
            rule: self,
            ctx,
            severity,
            diagnostics: Vec::new(),
            lines: HashSet::new(),
        };
        visitor.visit_file(&file);
        non_empty(visitor.diagnostics)
    }
}

// ---------------------------------------------------------------------------
// RTC004: zero existential deposit / zero storage deposits
// ---------------------------------------------------------------------------

fn is_existential_deposit_name(name: &str) -> bool {
    name.replace('_', "")
        .to_ascii_lowercase()
        .contains("existentialdeposit")
}

fn is_deposit_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("deposit") && !lower.starts_with("max") && !lower.starts_with("min")
}

/// Deposits that only guard origin-gated calls, with the gating origin type.
/// A zero deposit is not a spam vector when the origin cannot be any signed
/// account.
fn gating_origin(deposit: &str) -> Option<&'static str> {
    if deposit.ends_with("SubmissionDeposit") {
        Some("SubmitOrigin")
    } else if deposit.ends_with("AssetDeposit")
        || deposit.ends_with("MetadataDepositBase")
        || deposit.ends_with("MetadataDepositPerByte")
    {
        Some("CreateOrigin")
    } else {
        None
    }
}

/// Origin filters that only admit a privileged caller.
const PRIVILEGED_ORIGINS: &[&str] = &[
    "EnsureMember",
    "EnsureMembers",
    "EnsureNever",
    "EnsureOfRank",
    "EnsureProportionAtLeast",
    "EnsureProportionMoreThan",
    "EnsureRankedMember",
    "EnsureRoot",
    "EnsureRootWithSuccess",
    "EnsureSignedBy",
];

/// Origin filters that admit any signed account or any location.
const PERMISSIONLESS_ORIGINS: &[&str] = &["EnsureSigned", "Everything", "ForeignCreators"];

/// The origin is provably privileged: it names a privileged filter and no
/// permissionless one. Unknown origin types are treated as permissionless.
fn origin_is_privileged(ty: &Type) -> bool {
    let tokens = ty.to_token_stream();
    tokens_contain_ident(tokens.clone(), PRIVILEGED_ORIGINS)
        && !tokens_contain_ident(tokens, PERMISSIONLESS_ORIGINS)
}

fn deposit_is_origin_gated(item_impl: &ItemImpl, deposit_type: &str) -> bool {
    let Some(origin) = gating_origin(deposit_type) else {
        return false;
    };
    impl_type_named(item_impl, origin).is_some_and(|item| origin_is_privileged(&item.ty))
}

/// `const` declarations inside a `parameter_types!` body: name, line, zero?
fn parameter_type_consts(tokens: TokenStream) -> Vec<(String, usize, bool)> {
    let trees = tokens.into_iter().collect::<Vec<_>>();
    let mut consts = Vec::new();
    let mut index = 0;
    let mut masked = false;
    while index < trees.len() {
        match &trees[index] {
            TokenTree::Punct(punct) if punct.as_char() == '#' => {
                if let Some(TokenTree::Group(group)) = trees.get(index + 1) {
                    let attr = format!("#[{}]", group.stream());
                    if let Ok(attrs) =
                        syn::parse_str::<syn::DeriveInput>(&format!("{attr} struct S;"))
                    {
                        masked |= is_non_production(&attrs.attrs);
                    }
                    index += 2;
                    continue;
                }
            }
            TokenTree::Ident(ident) if ident == "const" => {
                if let Some(TokenTree::Ident(name)) = trees.get(index + 1) {
                    let line = name.span().start().line;
                    let mut cursor = index + 2;
                    while cursor < trees.len()
                        && !matches!(&trees[cursor], TokenTree::Punct(punct) if punct.as_char() == '=')
                    {
                        cursor += 1;
                    }
                    let start = cursor + 1;
                    let mut end = start;
                    while end < trees.len()
                        && !matches!(&trees[end], TokenTree::Punct(punct) if punct.as_char() == ';')
                    {
                        end += 1;
                    }
                    if !masked {
                        let value = trees.get(start..end).unwrap_or_default();
                        consts.push((name.to_string(), line, expr_tokens_are_zero(value)));
                    }
                    masked = false;
                    index = end + 1;
                    continue;
                }
            }
            TokenTree::Punct(punct) if punct.as_char() == ';' => masked = false,
            _ => {}
        }
        index += 1;
    }
    consts
}

pub struct ZeroDepositConfig;

impl LintRule for ZeroDepositConfig {
    fn id(&self) -> &str {
        "RTC004"
    }
    fn name(&self) -> &str {
        "zero-deposit-config"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        runtime_scope(ctx, &config.runtime_config)?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Warning);

        // Origin-gated consumers of named constants in this file: if every Config
        // impl that binds `type X = <Const>;` gates the deposit behind a
        // non-signed origin, a zero value is not a spam vector.
        let mut gated_consts: HashMap<String, bool> = HashMap::new();
        for_each_runtime_impl(&file, |item_impl, _| {
            for item_type in impl_types(item_impl) {
                let Some(name) = type_path(&item_type.ty).and_then(last_ident) else {
                    continue;
                };
                let gated = deposit_is_origin_gated(item_impl, &item_type.ident.to_string());
                gated_consts
                    .entry(name)
                    .and_modify(|all| *all &= gated)
                    .or_insert(gated);
            }
        });

        let mut zero_items: Vec<(String, usize)> = Vec::new();

        struct Collector<'a> {
            zero_items: &'a mut Vec<(String, usize)>,
            config_impl_hits: Vec<(String, usize)>,
        }
        impl<'ast> Visit<'ast> for Collector<'_> {
            fn visit_item_use(&mut self, _: &'ast syn::ItemUse) {}

            fn visit_item_const(&mut self, item: &'ast ItemConst) {
                if expr_is_zero(&item.expr) {
                    self.zero_items
                        .push((item.ident.to_string(), line_of(&item.ident)));
                }
                visit::visit_item_const(self, item);
            }

            fn visit_macro(&mut self, mac: &'ast Macro) {
                if last_ident(&mac.path).as_deref() == Some("parameter_types") {
                    for (name, line, zero) in parameter_type_consts(mac.tokens.clone()) {
                        if zero {
                            self.zero_items.push((name, line));
                        }
                    }
                }
                visit::visit_macro(self, mac);
            }

            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                if runtime_config_impl(item_impl).is_some() {
                    for item_type in impl_types(item_impl) {
                        let name = item_type.ident.to_string();
                        if const_type_is_zero(&item_type.ty)
                            && (is_existential_deposit_name(&name) || is_deposit_name(&name))
                            && (is_existential_deposit_name(&name)
                                || !deposit_is_origin_gated(item_impl, &name))
                        {
                            self.config_impl_hits.push((name, line_of(&item_type.ty)));
                        }
                    }
                }
                visit::visit_item_impl(self, item_impl);
            }
        }

        let mut collector = Collector {
            zero_items: &mut zero_items,
            config_impl_hits: Vec::new(),
        };
        collector.visit_file(&file);
        let config_impl_hits = collector.config_impl_hits;

        let mut diagnostics = Vec::new();
        for (name, line) in zero_items {
            let existential = is_existential_deposit_name(&name);
            if !existential && !is_deposit_name(&name) {
                continue;
            }
            if !existential && gated_consts.get(&name).copied().unwrap_or(false) {
                continue;
            }
            diagnostics.push(self.report(ctx, severity, line, &name, existential));
        }
        for (name, line) in config_impl_hits {
            let existential = is_existential_deposit_name(&name);
            diagnostics.push(self.report(ctx, severity, line, &name, existential));
        }
        diagnostics.sort_by_key(|diagnostic| diagnostic.line);
        diagnostics.dedup_by_key(|diagnostic| diagnostic.line);
        non_empty(diagnostics)
    }
}

impl ZeroDepositConfig {
    fn report(
        &self,
        ctx: &FileContext,
        severity: Severity,
        line: usize,
        name: &str,
        existential: bool,
    ) -> Diagnostic {
        if existential {
            diagnostic(
                self,
                ctx,
                severity,
                line,
                format!("`{name}` is configured as zero"),
                "With a zero existential deposit, accounts are never reaped and any account can be \
                 created for the cost of a transfer fee, enabling cheap state bloat.",
                "Set the existential deposit to a non-zero value derived from the storage cost of \
                 an account, and document why it must stay above zero.",
            )
        } else {
            diagnostic(
                self,
                ctx,
                severity,
                line,
                format!("deposit `{name}` is configured as zero"),
                "Storage deposits make permissionless callers pay for the state they create. A \
                 zero deposit lets any signed account write storage (approvals, metadata, \
                 accounts, ...) for the cost of a transaction fee.",
                "Charge a non-zero deposit (for example a base plus per-byte amount derived from \
                 the existential deposit), or restrict the call to a privileged origin.",
            )
        }
    }
}

// ---------------------------------------------------------------------------
// RTC005: insecure collective-flip randomness in the runtime
// ---------------------------------------------------------------------------

const INSECURE_RANDOMNESS: &[&str] = &[
    "pallet_insecure_randomness_collective_flip",
    "RandomnessCollectiveFlip",
];

pub struct InsecureRandomnessInRuntime;

impl LintRule for InsecureRandomnessInRuntime {
    fn id(&self) -> &str {
        "RTC005"
    }
    fn name(&self) -> &str {
        "insecure-randomness-in-runtime"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        runtime_scope(ctx, &config.runtime_config)?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Warning);

        struct Visitor {
            lines: Vec<usize>,
        }
        impl Visitor {
            fn scan_macro_tokens(&mut self, tokens: TokenStream) {
                for tree in tokens {
                    match tree {
                        TokenTree::Ident(ident)
                            if ident == "pallet_insecure_randomness_collective_flip" =>
                        {
                            self.lines.push(ident.span().start().line);
                        }
                        TokenTree::Group(group) => self.scan_macro_tokens(group.stream()),
                        _ => {}
                    }
                }
            }
        }
        impl<'ast> Visit<'ast> for Visitor {
            fn visit_item_use(&mut self, _: &'ast syn::ItemUse) {}

            fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
                // `impl pallet_insecure_randomness_collective_flip::Config for Runtime {}`
                // is reported through its runtime declaration; only inspect its items.
                for item in &item_impl.items {
                    self.visit_impl_item(item);
                }
            }

            fn visit_macro(&mut self, mac: &'ast Macro) {
                if last_ident(&mac.path).as_deref() == Some("construct_runtime") {
                    self.scan_macro_tokens(mac.tokens.clone());
                }
            }

            fn visit_impl_item_type(&mut self, item: &'ast ImplItemType) {
                if tokens_contain_ident(item.ty.to_token_stream(), INSECURE_RANDOMNESS) {
                    self.lines.push(line_of(&item.ty));
                }
            }

            fn visit_item_type(&mut self, item: &'ast ItemType) {
                if tokens_contain_ident(item.ty.to_token_stream(), INSECURE_RANDOMNESS) {
                    self.lines.push(line_of(&item.ty));
                }
            }
        }

        let mut visitor = Visitor { lines: Vec::new() };
        visitor.visit_file(&file);
        visitor.lines.sort_unstable();
        visitor.lines.dedup();

        non_empty(
            visitor
                .lines
                .into_iter()
                .map(|line| {
                    diagnostic(
                        self,
                        ctx,
                        severity,
                        line,
                        "runtime uses `pallet_insecure_randomness_collective_flip`".to_string(),
                        "Collective-flip randomness is derived from the previous 81 block hashes. \
                         Block authors can bias it and anyone can predict it, so any consumer \
                         (contracts, lotteries, committee shuffling) is exploitable.",
                        "Remove the pallet and source randomness from BABE/VRF output \
                         (`pallet_babe::RandomnessFromOneEpochAgo`) or the relay chain \
                         (`cumulus_pallet_parachain_system::RelaychainDataProvider` + VRF).",
                    )
                })
                .collect(),
        )
    }
}
