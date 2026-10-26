//! Runtime configuration rules (RTC001-RTC006).
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
//!   nearest Cargo package whose library target (`[lib] path`, default
//!   `src/lib.rs`) and the production modules reachable from it declare
//!   exactly one runtime with `construct_runtime!`, `#[frame_support::runtime]`
//!   (or a bare `#[runtime]` with `#[runtime::*]` items) or
//!   `#[frame_construct_runtime]`. Files that are not under any Cargo
//!   manifest fall back to inspecting the file itself;
//! - test code is always skipped: `#[cfg(test)]` items, files reached only
//!   through `#[cfg(test)]` (or benchmark-gated) `mod` declarations,
//!   `mock`/`tests`/`benches` path tokens, and test runtime crates (a package
//!   or directory name token listed in `test_runtime_tokens`);
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

use crate::{
    config::{Config, RuntimeConfigConfig},
    diagnostics::{Diagnostic, RuleCategory, Severity},
    engine::FileContext,
    rules::LintRule,
};

// ---------------------------------------------------------------------------
// Runtime crate discovery and scope
// ---------------------------------------------------------------------------

/// How a source file or item is gated by `cfg` attributes, from least to most
/// restrictive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Gate {
    /// Compiled into the production runtime.
    Production,
    /// Only compiled for `runtime-benchmarks`, `try-runtime` or `test-helpers`.
    NonProduction,
    /// Only compiled for `cfg(test)`.
    Test,
}

/// What the rules know about the Cargo package that owns a source file.
#[derive(Debug, Default)]
struct RuntimeCrate {
    /// Cargo package name, when the file belongs to a Cargo package.
    package: Option<String>,
    /// Package root directory (the directory holding `Cargo.toml`).
    root: Option<PathBuf>,
    /// The library target and the production modules reachable from it declare
    /// exactly one FRAME runtime.
    is_runtime: bool,
    /// Benchmark entry paths (segments joined with `_`) collected from every
    /// non-test `define_benchmarks!` reachable from the library target. `None`
    /// when the runtime has no `define_benchmarks!` at all.
    benchmarks: Option<HashSet<String>>,
    /// Gate of every source file reachable from the library target through
    /// `mod` declarations, keyed by canonical path.
    modules: HashMap<PathBuf, Gate>,
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
        let is_runtime = ast.is_some_and(|file| runtime_declarations(&production_ast(file)) == 1);
        let benchmarks = ast.and_then(|file| {
            let mut keys = HashSet::new();
            collect_define_benchmarks(&strip_ast(file, Gate::Test), &mut keys).then_some(keys)
        });
        return Arc::new(RuntimeCrate {
            is_runtime,
            benchmarks,
            ..RuntimeCrate::default()
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
    let manifest = fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|manifest| manifest.parse::<toml::Table>().ok());
    let package = manifest
        .as_ref()
        .and_then(|manifest| manifest.get("package")?.get("name")?.as_str())
        .map(ToOwned::to_owned);

    // A manifest without a package (a virtual workspace) does not own the file
    // as a crate source; the file is an orphan and is not a runtime.
    if package.is_none() {
        return RuntimeCrate {
            root: Some(root.to_path_buf()),
            ..RuntimeCrate::default()
        };
    }

    let lib_path = manifest
        .as_ref()
        .and_then(|manifest| manifest.get("lib")?.get("path")?.as_str())
        .unwrap_or("src/lib.rs");

    let mut modules = ModuleTree::default();
    modules.walk(&root.join(lib_path), true, Gate::Production);

    // A runtime crate builds exactly one runtime. Crates that declare several
    // (XCM simulators, emulated networks, documentation cookbooks) are test or
    // example harnesses, not runtimes.
    let declarations: usize = modules
        .files
        .values()
        .filter(|(gate, _)| *gate == Gate::Production)
        .map(|(_, file)| runtime_declarations(&production_ast(file)))
        .sum();
    let is_runtime = declarations == 1;

    let benchmarks = if is_runtime {
        let mut keys = HashSet::new();
        let mut found = false;
        for (gate, file) in modules.files.values() {
            if *gate != Gate::Test {
                found |= collect_define_benchmarks(&strip_ast(file, Gate::Test), &mut keys);
            }
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
        modules: modules
            .files
            .into_iter()
            .map(|(path, (gate, _))| (path, gate))
            .collect(),
    }
}

/// The crate's module tree: every file reachable through `mod name;`, with the
/// least restrictive `cfg` gate on any path that reaches it.
#[derive(Default)]
struct ModuleTree {
    files: HashMap<PathBuf, (Gate, SynFile)>,
}

impl ModuleTree {
    /// `owns_dir`: the file is a crate root or `mod.rs`, so its child modules
    /// live next to it rather than in a directory named after it.
    fn walk(&mut self, path: &Path, owns_dir: bool, gate: Gate) {
        let Ok(canonical) = fs::canonicalize(path) else {
            return;
        };
        if let Some((known, _)) = self.files.get_mut(&canonical) {
            if *known <= gate {
                return;
            }
            *known = gate;
        } else {
            let Some(file) = fs::read_to_string(&canonical)
                .ok()
                .and_then(|content| syn::parse_file(&content).ok())
            else {
                return;
            };
            self.files.insert(canonical.clone(), (gate, file));
        }

        let parent = canonical.parent().unwrap_or(Path::new("")).to_path_buf();
        let child_dir = if owns_dir {
            parent.clone()
        } else {
            parent.join(canonical.file_stem().unwrap_or_default())
        };
        let gate = gate.max(attrs_gate(&self.files[&canonical].1.attrs));
        let items = self.files[&canonical].1.items.clone();
        self.walk_items(&items, &parent, &child_dir, false, gate);
    }

    fn walk_items(
        &mut self,
        items: &[Item],
        file_dir: &Path,
        child_dir: &Path,
        inline: bool,
        gate: Gate,
    ) {
        for item in items {
            let Item::Mod(module) = item else {
                continue;
            };
            let gate = gate.max(attrs_gate(&module.attrs));
            let explicit = path_attr(&module.attrs);
            let name = module.ident.to_string();
            match &module.content {
                Some((_, items)) => {
                    let dir = explicit
                        .map(|path| child_dir.join(path))
                        .unwrap_or_else(|| child_dir.join(&name));
                    self.walk_items(items, file_dir, &dir, true, gate);
                }
                None => {
                    if let Some(path) = explicit {
                        let base = if inline { child_dir } else { file_dir };
                        self.walk(&base.join(path), true, gate);
                        continue;
                    }
                    let flat = child_dir.join(format!("{name}.rs"));
                    if flat.is_file() {
                        self.walk(&flat, false, gate);
                    } else {
                        self.walk(&child_dir.join(&name).join("mod.rs"), true, gate);
                    }
                }
            }
        }
    }
}

/// `#[path = "..."]` on a module declaration.
fn path_attr(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        Meta::NameValue(name_value) if name_value.path.is_ident("path") => {
            match &name_value.value {
                Expr::Lit(expr) => match &expr.lit {
                    Lit::Str(path) => Some(path.value()),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    })
}

/// Number of `construct_runtime!`, `#[frame_support::runtime]` (or a bare
/// `#[runtime]` whose module uses `#[runtime::*]` attributes) and
/// `#[frame_construct_runtime]` declarations at any module level of the file.
fn runtime_declarations(file: &SynFile) -> usize {
    fn attr_segments(attr: &Attribute) -> Vec<String> {
        attr.path()
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect()
    }
    /// `#[runtime::runtime]`, `#[runtime::pallet_index(..)]`, `#[runtime::derive(..)]`.
    fn has_runtime_item_attr(items: &[Item]) -> bool {
        items.iter().any(|item| {
            item_attrs(item).iter().any(|attr| {
                matches!(
                    attr_segments(attr)
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()[..],
                    ["runtime", "runtime" | "pallet_index" | "derive"]
                )
            })
        })
    }
    struct Finder(usize);
    impl<'ast> Visit<'ast> for Finder {
        fn visit_item_macro(&mut self, item: &'ast ItemMacro) {
            if last_ident(&item.mac.path).as_deref() == Some("construct_runtime") {
                self.0 += 1;
            }
        }
        fn visit_item_mod(&mut self, item: &'ast ItemMod) {
            let items = item
                .content
                .as_ref()
                .map(|(_, items)| items.as_slice())
                .unwrap_or_default();
            if item.attrs.iter().any(|attr| {
                let segments = attr_segments(attr);
                match segments.iter().map(String::as_str).collect::<Vec<_>>()[..] {
                    [.., "frame_construct_runtime"] => true,
                    [_, .., "runtime"] => true,
                    ["runtime"] => has_runtime_item_attr(items),
                    _ => false,
                }
            }) {
                self.0 += 1;
            }
            visit::visit_item_mod(self, item);
        }
    }
    let mut finder = Finder(0);
    finder.visit_file(file);
    finder.0
}

/// Lower-cased name tokens, split on `-`, `_` and `.`.
fn name_tokens(name: &str) -> impl Iterator<Item = String> + '_ {
    name.split(['-', '_', '.'])
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_lowercase)
}

/// Path components that mark test, mock, or benchmark-only sources: a whole
/// `test`/`tests`/`mock`/`mocks`/`benches` token, so `testnet` or `contest`
/// do not match.
fn is_test_component(component: &str) -> bool {
    let stem = component.strip_suffix(".rs").unwrap_or(component);
    name_tokens(stem).any(|token| {
        matches!(
            token.as_str(),
            "test" | "tests" | "mock" | "mocks" | "benches"
        )
    })
}

fn is_test_path(path: &Path) -> bool {
    path.components()
        .any(|component| is_test_component(&component.as_os_str().to_string_lossy()))
}

/// Package or directory names that identify a test-only runtime crate: one of
/// the configured `test_runtime_tokens` is a whole name token.
fn is_test_package(name: &str, config: &RuntimeConfigConfig) -> bool {
    name_tokens(name).any(|token| {
        config
            .test_runtime_tokens
            .iter()
            .any(|test| test.eq_ignore_ascii_case(&token))
    })
}

/// Shared gate: the file is non-test source of a production (or opted-in dev)
/// runtime crate.
fn runtime_scope(ctx: &FileContext, config: &RuntimeConfigConfig) -> Option<Arc<RuntimeCrate>> {
    if !ctx.is_rust || ctx.is_test_file || ctx.is_benchmark_file || ctx.ast.is_none() {
        return None;
    }

    let krate = runtime_crate_for(ctx);
    if !krate.is_runtime {
        return None;
    }
    let Some(root) = &krate.root else {
        // Single-file fallback: only the scan-relative path is known.
        return (!is_test_path(&ctx.rel_path)).then_some(krate);
    };

    let in_crate = ctx.path.strip_prefix(root).unwrap_or(&ctx.path);
    if is_test_path(in_crate) {
        return None;
    }
    // Files reached only through `#[cfg(test)]` or benchmark-gated `mod`
    // declarations are not production code. Files the module walk could not
    // reach (macro-generated modules, `include!`) are kept.
    let gate = fs::canonicalize(&ctx.path)
        .ok()
        .and_then(|path| krate.modules.get(&path).copied());
    if gate.is_some_and(|gate| gate != Gate::Production) {
        return None;
    }
    let dir_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if is_test_package(&dir_name, config) {
        return None;
    }
    if let Some(package) = &krate.package {
        if is_test_package(package, config) {
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

fn cfg_is_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::NameValue(_) => false,
        Meta::List(list) => {
            let Ok(nested) = list.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return false;
            };
            if list.path.is_ident("all") {
                nested.iter().any(cfg_is_test)
            } else if list.path.is_ident("any") {
                !nested.is_empty() && nested.iter().all(cfg_is_test)
            } else {
                false
            }
        }
    }
}

/// The most restrictive gate implied by `#[test]` / `#[cfg(..)]` attributes.
fn attrs_gate(attrs: &[Attribute]) -> Gate {
    attrs
        .iter()
        .map(|attr| {
            if attr.path().is_ident("test") {
                return Gate::Test;
            }
            if !attr.path().is_ident("cfg") {
                return Gate::Production;
            }
            match attr.parse_args::<Meta>() {
                Ok(meta) if cfg_is_test(&meta) => Gate::Test,
                Ok(meta) if cfg_is_non_production(&meta) => Gate::NonProduction,
                _ => Gate::Production,
            }
        })
        .max()
        .unwrap_or(Gate::Production)
}

fn is_non_production(attrs: &[Attribute]) -> bool {
    attrs_gate(attrs) != Gate::Production
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

/// Clone the file without the items gated at `strip` or more restrictively.
fn strip_ast(file: &SynFile, strip: Gate) -> SynFile {
    fn strip_items(items: &mut Vec<Item>, strip: Gate) {
        items.retain(|item| attrs_gate(item_attrs(item)) < strip);
        for item in items {
            match item {
                Item::Mod(module) => {
                    if let Some((_, items)) = &mut module.content {
                        strip_items(items, strip);
                    }
                }
                Item::Impl(item_impl) => {
                    item_impl
                        .items
                        .retain(|item| attrs_gate(impl_item_attrs(item)) < strip);
                }
                _ => {}
            }
        }
    }
    let mut file = file.clone();
    if attrs_gate(&file.attrs) >= strip {
        file.items.clear();
    }
    strip_items(&mut file.items, strip);
    file
}

/// Clone the file without `#[cfg(test)]`, `#[test]` and benchmark/try-runtime
/// gated items, so rules only see production runtime configuration.
fn production_ast(file: &SynFile) -> SynFile {
    strip_ast(file, Gate::NonProduction)
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

fn expr_tokens_are_zero(tokens: &[TokenTree]) -> bool {
    syn::parse2::<Expr>(tokens.iter().cloned().collect()).is_ok_and(|expr| expr_is_zero(&expr))
}

/// A literal zero, possibly scaled: `0`, `0u128`, `(0)`, `0 as Balance`,
/// `0 * UNIT`, `UNIT * 0`, `{ 0 * CENTS }`, `0 * DOLLARS / 100`.
fn expr_is_zero(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(expr) => {
            matches!(&expr.lit, Lit::Int(int) if int.base10_digits().chars().all(|ch| ch == '0'))
        }
        Expr::Paren(expr) => expr_is_zero(&expr.expr),
        Expr::Group(expr) => expr_is_zero(&expr.expr),
        Expr::Cast(expr) => expr_is_zero(&expr.expr),
        Expr::Block(block) => match block.block.stmts.as_slice() {
            [syn::Stmt::Expr(expr, None)] => expr_is_zero(expr),
            _ => false,
        },
        Expr::Binary(binary) => match binary.op {
            syn::BinOp::Mul(_) | syn::BinOp::BitAnd(_) => {
                expr_is_zero(&binary.left) || expr_is_zero(&binary.right)
            }
            syn::BinOp::Div(_) | syn::BinOp::Rem(_) | syn::BinOp::Shl(_) | syn::BinOp::Shr(_) => {
                expr_is_zero(&binary.left)
            }
            syn::BinOp::Add(_) | syn::BinOp::BitOr(_) | syn::BinOp::BitXor(_) => {
                expr_is_zero(&binary.left) && expr_is_zero(&binary.right)
            }
            _ => false,
        },
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
                         backwards compatibility. For most FRAME pallets it carries the SDK's reference \
                         weights (benchmarked on reference hardware with `RocksDbWeight`), not weights \
                         benchmarked for this runtime's configuration and storage; a few pallets return \
                         zero or flat weights there. Extrinsics can therefore be mis-weighted and \
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
                         not benchmarked and return zero or flat, arbitrary weights.",
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

/// Record the entry path of every `define_benchmarks!` invocation in the file,
/// segments joined with `_` (`[polkadot_runtime_parachains::disputes, ..]` is
/// `polkadot_runtime_parachains_disputes`). Returns whether any invocation was
/// found.
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
                    if !segments.is_empty() {
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

/// Two `_`-joined module paths name the same module when they are equal or
/// one is a whole-segment suffix of the other (`runtime_parachains_paras` and
/// `polkadot_runtime_parachains_paras`, written with and without the crate
/// alias). `polkadot_runtime_parachains_disputes` does not match
/// `polkadot_runtime_parachains_disputes_slashing`.
fn joined_paths_match(a: &str, b: &str) -> bool {
    let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    long == short
        || long
            .strip_suffix(short)
            .is_some_and(|prefix| prefix.ends_with('_'))
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
                let pallet = pallet_segments.join("_");
                let covered = benchmarks.iter().any(|key| {
                    joined_paths_match(key, &pallet) || joined_paths_match(key, &module)
                });
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
// RTC003 / RTC006: XCM fees waived (RTC003) or burned (RTC006)
// ---------------------------------------------------------------------------

const DELIVERY_PRICE_TYPES: &[&str] = &[
    "PriceForSiblingDelivery",
    "PriceForParentDelivery",
    "PriceForChildParachainDelivery",
];

/// Routers whose third generic parameter is the delivery price.
const PRICED_ROUTERS: &[&str] = &["ParentAsUmp", "ChildParachainRouter"];

/// `()` and `NoPriceForMessageDelivery<_>` both return an empty price
/// (polkadot/runtime/common/src/xcm_sender.rs).
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

/// Same-file `type Name = ...;` aliases, used to resolve generic arguments
/// such as `XcmFeeManagerFromComponents<WaivedLocations, ..>`.
fn type_aliases(file: &SynFile) -> HashMap<String, Type> {
    struct Aliases(HashMap<String, Type>);
    impl<'ast> Visit<'ast> for Aliases {
        fn visit_item_type(&mut self, item: &'ast ItemType) {
            if item.generics.params.is_empty() {
                self.0.insert(item.ident.to_string(), (*item.ty).clone());
            }
        }
    }
    let mut aliases = Aliases(HashMap::new());
    aliases.visit_file(file);
    aliases.0
}

/// The type (after resolving same-file aliases) admits every location:
/// `Everything`, or a tuple containing it (tuples of `Contains` are a union).
fn contains_everything(ty: &Type, aliases: &HashMap<String, Type>, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match ty {
        Type::Paren(inner) => contains_everything(&inner.elem, aliases, depth + 1),
        Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .any(|elem| contains_everything(elem, aliases, depth + 1)),
        _ => {
            let Some(ident) = type_path(ty).and_then(last_ident) else {
                return false;
            };
            ident == "Everything"
                || (type_path(ty).is_some_and(|path| path.segments.len() == 1)
                    && aliases
                        .get(&ident)
                        .is_some_and(|alias| contains_everything(alias, aliases, depth + 1)))
        }
    }
}

fn generic_type_args(segment: &syn::PathSegment) -> Vec<&Type> {
    match &segment.arguments {
        syn::PathArguments::AngleBracketed(args) => args
            .args
            .iter()
            .filter_map(|arg| match arg {
                syn::GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum XcmFeeFinding {
    /// RTC003: fees are not charged at all.
    Waived,
    /// RTC006: fees are charged and then dropped.
    Burned,
}

fn type_text(ty: &Type) -> String {
    ty.to_token_stream().to_string().replace(' ', "")
}

/// Every XCM fee finding in the production AST: kind, line, message.
fn xcm_fee_findings(file: &SynFile) -> Vec<(XcmFeeFinding, usize, String)> {
    struct Visitor<'a> {
        aliases: &'a HashMap<String, Type>,
        findings: Vec<(XcmFeeFinding, usize, String)>,
        lines: HashSet<usize>,
    }

    impl Visitor<'_> {
        fn push(&mut self, kind: XcmFeeFinding, line: usize, message: String) {
            if self.lines.insert(line) {
                self.findings.push((kind, line, message));
            }
        }

        fn check_price(&mut self, ident: &str, ty: &Type) {
            if DELIVERY_PRICE_TYPES.contains(&ident) && is_free_delivery_price(ty) {
                self.push(
                    XcmFeeFinding::Waived,
                    line_of(ty),
                    format!(
                        "`{ident}` is `{}`, so XCM messages are delivered without a fee",
                        type_text(ty)
                    ),
                );
            }
        }

        fn check_fee_manager(&mut self, ty: &Type) {
            if is_unit_type(ty) {
                self.push(
                    XcmFeeFinding::Burned,
                    line_of(ty),
                    "XCM executor `FeeManager` is `()`: delivery and export fees are charged and \
                     then burned rather than routed (e.g. to the treasury); audits of older SDK \
                     versions reported this setting as waiving fees"
                        .to_string(),
                );
                return;
            }
            let Some(segment) = type_path(ty).and_then(|path| path.segments.last()) else {
                return;
            };
            let ident = segment.ident.to_string();
            let waived_everywhere = ident == "XcmFeeManagerFromComponents"
                && generic_type_args(segment)
                    .first()
                    .is_some_and(|waived| contains_everything(waived, self.aliases, 0));
            if ident == "WaiveDeliveryFees" || waived_everywhere {
                self.push(
                    XcmFeeFinding::Waived,
                    line_of(ty),
                    format!(
                        "XCM executor `FeeManager` is `{}`: delivery and export fees are waived \
                         for every origin",
                        type_text(ty)
                    ),
                );
            }
        }
    }

    impl<'ast> Visit<'ast> for Visitor<'_> {
        fn visit_item_use(&mut self, _: &'ast syn::ItemUse) {}

        fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
            if is_xcm_executor_impl(item_impl) {
                // An executor that can neither send nor export never pays a
                // delivery or export fee, so its fee manager is moot.
                let sends = impl_type_named(item_impl, "XcmSender")
                    .is_none_or(|item| !is_unit_type(&item.ty));
                let exports = impl_type_named(item_impl, "MessageExporter")
                    .is_none_or(|item| !is_unit_type(&item.ty));
                if let Some(fee_manager) = impl_type_named(item_impl, "FeeManager") {
                    if sends || exports {
                        self.check_fee_manager(&fee_manager.ty);
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
                    if let Some(price) = generic_type_args(segment).get(2) {
                        if is_free_delivery_price(price) {
                            self.push(
                                XcmFeeFinding::Waived,
                                line_of(*price),
                                format!(
                                    "`{ident}` router is configured with delivery price `{}`",
                                    type_text(price)
                                ),
                            );
                        }
                    }
                }
            }
            visit::visit_type_path(self, type_path);
        }
    }

    let aliases = type_aliases(file);
    let mut visitor = Visitor {
        aliases: &aliases,
        findings: Vec::new(),
        lines: HashSet::new(),
    };
    visitor.visit_file(file);
    visitor.findings
}

fn xcm_fee_diagnostics(
    rule: &dyn LintRule,
    ctx: &FileContext,
    config: &Config,
    kind: XcmFeeFinding,
    default: Severity,
    explanation: &str,
    suggestion: &str,
) -> Option<Vec<Diagnostic>> {
    runtime_scope(ctx, &config.runtime_config)?;
    let file = production_ast(ctx.ast.as_ref()?);
    let severity = config.rule_severity(rule.id(), default);
    non_empty(
        xcm_fee_findings(&file)
            .into_iter()
            .filter(|(found, _, _)| *found == kind)
            .map(|(_, line, message)| {
                diagnostic(rule, ctx, severity, line, message, explanation, suggestion)
            })
            .collect(),
    )
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
        xcm_fee_diagnostics(
            self,
            ctx,
            config,
            XcmFeeFinding::Waived,
            Severity::Error,
            "XCM delivery fees are the congestion control for cross-chain messaging. When they are \
             waived or priced at zero, any account can fill the outbound XCMP/UMP/DMP queues for \
             free, delaying or dropping other users' messages and exhausting storage.",
            "Use `XcmFeeManagerFromComponents<WaivedLocations, SendXcmFeeToAccount<..>>` with a \
             narrow `WaivedLocations` (e.g. the relay chain and system parachains) for \
             `FeeManager`, and an `ExponentialPrice<..>` for delivery prices, as the SDK system \
             parachains do.",
        )
    }
}

pub struct XcmFeesBurned;

impl LintRule for XcmFeesBurned {
    fn id(&self) -> &str {
        "RTC006"
    }
    fn name(&self) -> &str {
        "xcm-fees-burned"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        xcm_fee_diagnostics(
            self,
            ctx,
            config,
            XcmFeeFinding::Burned,
            Severity::Warning,
            "`impl FeeManager for ()` never waives a fee and its `handle_fee` does nothing, which the \
             trait documents as silently burning the fees (polkadot/xcm/xcm-executor/src/traits/\
             fee_manager.rs). Users still pay, but the revenue is destroyed instead of reaching the \
             treasury or collators. SR Labs audits of runtimes on older SDK versions reported the \
             same setting as waiving fees.",
            "Route fees with `XcmFeeManagerFromComponents<WaivedLocations, SendXcmFeeToAccount<\
             AssetTransactor, TreasuryAccount>>`, or document that burning XCM fees is intended.",
        )
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

/// Origin combinators that admit a caller when any of their origin arguments
/// does: every argument must be privileged.
const UNION_ORIGINS: &[&str] = &["EitherOf", "EitherOfDiverse"];

/// Origin adapters and the index of the type argument holding the origin they
/// check (frame/support/src/traits/dispatch.rs, frame/system/src/lib.rs).
const WRAPPING_ORIGINS: &[(&str, usize)] = &[
    ("AsEnsureOriginWithArg", 0),
    ("EnsureWithSuccess", 0),
    ("MapSuccess", 0),
    ("TryMapSuccess", 0),
    ("TryWithMorphedArg", 3),
];

/// The origin is provably privileged. Known combinators are parsed
/// recursively and same-file `type` aliases are resolved; any unknown or
/// cross-file origin type is treated as permissionless.
fn origin_is_privileged(ty: &Type, aliases: &HashMap<String, Type>, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    if let Type::Paren(inner) = ty {
        return origin_is_privileged(&inner.elem, aliases, depth + 1);
    }
    let Some(path) = type_path(ty) else {
        return false;
    };
    let Some(segment) = path.segments.last() else {
        return false;
    };
    let ident = segment.ident.to_string();
    let args = generic_type_args(segment);
    if PRIVILEGED_ORIGINS.contains(&ident.as_str()) {
        true
    } else if UNION_ORIGINS.contains(&ident.as_str()) {
        !args.is_empty()
            && args
                .iter()
                .all(|arg| origin_is_privileged(arg, aliases, depth + 1))
    } else if let Some((_, index)) = WRAPPING_ORIGINS.iter().find(|(name, _)| *name == ident) {
        args.get(*index)
            .is_some_and(|arg| origin_is_privileged(arg, aliases, depth + 1))
    } else if path.segments.len() == 1 && segment.arguments.is_empty() {
        aliases
            .get(&ident)
            .is_some_and(|alias| origin_is_privileged(alias, aliases, depth + 1))
    } else {
        false
    }
}

fn deposit_is_origin_gated(
    item_impl: &ItemImpl,
    deposit_type: &str,
    aliases: &HashMap<String, Type>,
) -> bool {
    let Some(origin) = gating_origin(deposit_type) else {
        return false;
    };
    impl_type_named(item_impl, origin)
        .is_some_and(|item| origin_is_privileged(&item.ty, aliases, 0))
}

/// Declarations inside a `parameter_types!` body: name, line, zero? Covers the
/// `pub const X: T = v;`, `pub X: T = v;` and `pub storage X: T = v;` forms
/// (frame/support/src/macros.rs), skipping test/benchmark-gated declarations.
fn parameter_type_consts(tokens: TokenStream) -> Vec<(String, usize, bool)> {
    let trees = tokens.into_iter().collect::<Vec<_>>();
    trees
        .split(|tree| matches!(tree, TokenTree::Punct(punct) if punct.as_char() == ';'))
        .filter_map(|declaration| {
            let mut index = 0;
            let mut masked = false;
            while let (Some(TokenTree::Punct(punct)), Some(TokenTree::Group(group))) =
                (declaration.get(index), declaration.get(index + 1))
            {
                if punct.as_char() != '#' {
                    break;
                }
                if let Ok(attrs) =
                    syn::parse_str::<syn::DeriveInput>(&format!("#[{}] struct S;", group.stream()))
                {
                    masked |= is_non_production(&attrs.attrs);
                }
                index += 2;
            }
            let rest = &declaration[index..];
            let colon = rest.iter().position(
                |tree| matches!(tree, TokenTree::Punct(punct) if punct.as_char() == ':'),
            )?;
            let TokenTree::Ident(name) = rest.get(colon.checked_sub(1)?)? else {
                return None;
            };
            let equals = rest.iter().position(|tree| {
                matches!(tree, TokenTree::Punct(punct)
                    if punct.as_char() == '=' && punct.spacing() == proc_macro2::Spacing::Alone)
            })?;
            (!masked).then(|| {
                (
                    name.to_string(),
                    name.span().start().line,
                    expr_tokens_are_zero(&rest[equals + 1..]),
                )
            })
        })
        .collect()
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
        let aliases = type_aliases(&file);
        let mut gated_consts: HashMap<String, bool> = HashMap::new();
        for_each_runtime_impl(&file, |item_impl, _| {
            for item_type in impl_types(item_impl) {
                let Some(name) = type_path(&item_type.ty).and_then(last_ident) else {
                    continue;
                };
                let gated =
                    deposit_is_origin_gated(item_impl, &item_type.ident.to_string(), &aliases);
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
            aliases: &'a HashMap<String, Type>,
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
                                || !deposit_is_origin_gated(item_impl, &name, self.aliases))
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
            aliases: &aliases,
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
                 accounts, ...) for the cost of a transaction fee. A zero deposit is only \
                 accepted when the consuming `Config` impl in this file gates the call with a \
                 known privileged origin (`EnsureRoot`, `EnsureSignedBy`, `EnsureMember`, ... \
                 or `EitherOf` of them); origins defined in other files are not resolved.",
                "Charge a non-zero deposit (for example a base plus per-byte amount derived from \
                 the existential deposit), or restrict the call to a privileged origin.",
            )
        }
    }
}

// ---------------------------------------------------------------------------
// RTC005: insecure collective-flip randomness in the runtime
// ---------------------------------------------------------------------------

/// Collective-flip crate names: the current one and the pre-rename
/// `pallet_randomness_collective_flip`.
const INSECURE_RANDOMNESS_PALLETS: &[&str] = &[
    "pallet_insecure_randomness_collective_flip",
    "pallet_randomness_collective_flip",
];

/// The crates above plus the conventional runtime pallet name.
const INSECURE_RANDOMNESS: &[&str] = &[
    "pallet_insecure_randomness_collective_flip",
    "pallet_randomness_collective_flip",
    "RandomnessCollectiveFlip",
];

/// The first insecure-randomness ident in `tokens`, with its line.
fn find_ident(tokens: TokenStream, needles: &[&str]) -> Option<(String, usize)> {
    tokens.into_iter().find_map(|tree| match tree {
        TokenTree::Ident(ident) if needles.iter().any(|needle| ident == needle) => {
            Some((ident.to_string(), ident.span().start().line))
        }
        TokenTree::Group(group) => find_ident(group.stream(), needles),
        _ => None,
    })
}

/// Pallet entries of a `construct_runtime!` body (`Name: path = index,`),
/// without the entries gated on `cfg(test)` or benchmark/try-runtime features.
fn construct_runtime_entries(tokens: TokenStream) -> Vec<Vec<TokenTree>> {
    // The entries are the last top-level brace group: `pub enum Runtime { .. }`
    // or the legacy `pub enum Runtime where .. { .. }`.
    let Some(body) = tokens
        .into_iter()
        .filter_map(|tree| match tree {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                Some(group.stream())
            }
            _ => None,
        })
        .last()
    else {
        return Vec::new();
    };
    let trees = body.into_iter().collect::<Vec<_>>();
    trees
        .split(|tree| matches!(tree, TokenTree::Punct(punct) if punct.as_char() == ','))
        .filter(|entry| {
            let mut index = 0;
            while let (Some(TokenTree::Punct(punct)), Some(TokenTree::Group(group))) =
                (entry.get(index), entry.get(index + 1))
            {
                if punct.as_char() != '#' {
                    break;
                }
                let gated =
                    syn::parse_str::<syn::DeriveInput>(&format!("#[{}] struct S;", group.stream()))
                        .is_ok_and(|attrs| is_non_production(&attrs.attrs));
                if gated {
                    return false;
                }
                index += 2;
            }
            true
        })
        .map(<[TokenTree]>::to_vec)
        .collect()
}

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
            hits: Vec<(usize, String)>,
        }
        impl Visitor {
            fn record(&mut self, tokens: TokenStream, needles: &[&str]) {
                if let Some((ident, line)) = find_ident(tokens, needles) {
                    self.hits.push((line, ident));
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
                    for entry in construct_runtime_entries(mac.tokens.clone()) {
                        self.record(entry.into_iter().collect(), INSECURE_RANDOMNESS_PALLETS);
                    }
                }
            }

            fn visit_impl_item_type(&mut self, item: &'ast ImplItemType) {
                self.record(item.ty.to_token_stream(), INSECURE_RANDOMNESS);
            }

            fn visit_item_type(&mut self, item: &'ast ItemType) {
                self.record(item.ty.to_token_stream(), INSECURE_RANDOMNESS);
            }
        }

        let mut visitor = Visitor { hits: Vec::new() };
        visitor.visit_file(&file);
        visitor.hits.sort();
        visitor.hits.dedup_by_key(|(line, _)| *line);

        non_empty(
            visitor
                .hits
                .into_iter()
                .map(|(line, ident)| {
                    diagnostic(
                        self,
                        ctx,
                        severity,
                        line,
                        format!("runtime uses collective-flip randomness `{ident}`"),
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
