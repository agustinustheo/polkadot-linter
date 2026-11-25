//! Runtime configuration rules (RTC001-RTC011).
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
//!   exactly one ungated runtime (or, with none ungated, only runtimes
//!   switched by `feature` cfgs) with `construct_runtime!`, `#[frame_support::runtime]`
//!   (or a bare `#[runtime]` with `#[runtime::*]` items) or
//!   `#[frame_construct_runtime]`. Files that are not under any Cargo
//!   manifest fall back to inspecting the file itself;
//! - test code is always skipped: `#[cfg(test)]` items, files reached only
//!   through `#[cfg(test)]` (or benchmark-gated) `mod` declarations,
//!   `mock`/`tests`/`benches` path tokens, and test runtime crates (a package
//!   or directory name token listed in `test_runtime_tokens`);
//! - dev runtimes listed in `[runtime_config].dev_runtime_packages`, or whose
//!   package or directory name has a `dev_runtime_tokens` token, are
//!   skipped unless `include_dev_runtimes = true`.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use syn::{
    parse::Parser,
    spanned::Spanned,
    visit::{self, Visit},
    Attribute, Expr, File as SynFile, ImplItem, ImplItemType, Item, ItemConst, ItemImpl, ItemMacro,
    ItemMod, ItemType, Lit, Macro, Meta, Type, TypePath,
};
use walkdir::WalkDir;

use crate::{
    config::{Config, RuntimeConfigConfig},
    diagnostics::{default_confidence, Confidence, Diagnostic, RuleCategory, Severity},
    engine::{crate_lib, scan_cached, FileContext},
    rules::LintRule,
    syn_util::{
        compact_tokens, is_construct_runtime, macro_name, path_attr, path_last_ident,
        strip_expr_wrappers,
    },
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
    /// The module tree was parsed. A crate that [`may_declare_runtime`]
    /// skipped is not a runtime here, but it is not a proven library either.
    sources_read: bool,
    /// Gate of every source file reachable from the library target through
    /// `mod` declarations, keyed by canonical path.
    modules: HashMap<PathBuf, Gate>,
    /// Production runtime pallets from `VALUE_MOVING_PALLETS`: `RuntimeCall`
    /// variant and pallet crate.
    pallets: Vec<(String, String)>,
    /// Every declared production `RuntimeCall` variant name and the pallet
    /// crate of each declaration (`None` when not visible). `#[cfg]`
    /// alternatives of a runtime declaration can give a variant several crates.
    pallet_crates: HashMap<String, Vec<Option<String>>>,
    /// The version Cargo writes for the package in `Cargo.lock`. `None` when
    /// the manifest inherits a version that the workspace does not state.
    version: Option<String>,
    /// Canonical root of the workspace the package belongs to, if any.
    workspace: Option<PathBuf>,
    /// Every production runtime pallet name.
    pallet_names: HashSet<String>,
    /// `&'static str` constants and `parameter_types!`: name to string value.
    str_params: HashMap<String, String>,
    /// Every production `AssetTransactor` is `()`, and the executors that
    /// `pallet_xcm` and every other `XcmExecutor<..>` name are visible
    /// `xcm_executor::Config` impls. See [`Declarations::unit_asset_transactor_only`].
    unit_asset_transactor_only: bool,
    /// True if `pallet_xcm::Config` exists in this runtime crate, all
    /// permissionless local `pallet_xcm` outbound paths are provably
    /// privileged-only or `Nothing`, and no other pallet declares an XCM sender.
    local_outbound_xcm_privileged_only: bool,
}

/// Nearest ancestor of `path` with a `Cargo.toml`, spelt as in `path`.
///
/// The result is not canonical, so it strips as a prefix of `path` under a
/// relative or symlinked scan root.
fn manifest_dir(path: &Path) -> Option<&Path> {
    // The file may not exist yet (unsaved editor buffers); an empty root is the
    // working directory, outside the scanned package.
    path.ancestors()
        .skip(1)
        .filter(|dir| !dir.as_os_str().is_empty())
        .find(|dir| dir.join("Cargo.toml").is_file())
}

/// Locate the Cargo package owning `path`, or fall back to the file itself
/// (its AST from `ast`) when no manifest exists (single-file fixtures).
fn runtime_crate_at(path: &Path, ast: impl FnOnce() -> Option<SynFile>) -> Arc<RuntimeCrate> {
    // The canonical directory is the cache key, so relative and absolute scans
    // share one parsed crate.
    let manifest_dir =
        manifest_dir(path).map(|dir| fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()));

    // Fixtures and single files scanned outside any Cargo package.
    let Some(manifest_dir) = manifest_dir else {
        let mut declarations = Declarations::default();
        if let Some(file) = ast() {
            declarations.count(&production_ast(&file), false, Some(&[]));
        }
        let local_outbound_xcm_privileged_only = declarations.local_outbound_xcm_privileged_only();
        let unit_asset_transactor_only = declarations.unit_asset_transactor_only();
        return Arc::new(RuntimeCrate {
            is_runtime: declarations.is_runtime(),
            sources_read: true,
            pallets: declarations.pallets,
            pallet_crates: declarations.pallet_crates,
            pallet_names: declarations.pallet_names,
            str_params: declarations.str_params,
            unit_asset_transactor_only,
            local_outbound_xcm_privileged_only,
            ..RuntimeCrate::default()
        });
    };

    scan_cached(manifest_dir, |dir| Arc::new(load_runtime_crate(dir)))
}

fn load_runtime_crate(root: &Path) -> RuntimeCrate {
    load_crate(root, CrateLoad::Sources)
}

/// What [`load_crate`] reads beyond the manifest.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CrateLoad {
    /// Parse the module tree and its declarations.
    Sources,
    /// Read only the manifest: the crate is not a runtime.
    ManifestOnly,
}

fn load_crate(root: &Path, load: CrateLoad) -> RuntimeCrate {
    let manifest = fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|manifest| manifest.parse::<toml::Value>().ok());
    let package_field = |key: &str| {
        manifest
            .as_ref()
            .and_then(|manifest| manifest.get("package")?.get(key)?.as_str())
            .map(ToOwned::to_owned)
    };
    let package = package_field("name");

    // A manifest without a package (a virtual workspace) does not own the file
    // as a crate source; the file is an orphan and is not a runtime.
    if package.is_none() {
        return RuntimeCrate {
            root: Some(root.to_path_buf()),
            ..RuntimeCrate::default()
        };
    }
    let workspace = workspace_manifest(root, manifest.as_ref());
    let version = lock_version(manifest.as_ref(), workspace.as_ref());
    if load == CrateLoad::ManifestOnly {
        return RuntimeCrate {
            package,
            version,
            workspace: workspace.map(|(dir, _)| dir),
            root: Some(root.to_path_buf()),
            ..RuntimeCrate::default()
        };
    }

    let mut modules = ModuleTree::default();
    modules.walk(
        &crate_lib(root, manifest.as_ref()),
        true,
        Gate::Production,
        false,
        Some(&[]),
    );

    let mut declarations = Declarations::default();
    for module in modules.files.values() {
        if module.gate == Gate::Production {
            declarations.count(
                &production_ast(&module.file),
                module.featured,
                module.path.as_deref(),
            );
        }
    }
    let is_runtime = declarations.is_runtime();
    let sources_read = !modules.files.is_empty();
    let local_outbound_xcm_privileged_only = declarations.local_outbound_xcm_privileged_only();
    let unit_asset_transactor_only = declarations.unit_asset_transactor_only();

    RuntimeCrate {
        package,
        version,
        workspace: workspace.map(|(dir, _)| dir),
        root: Some(root.to_path_buf()),
        is_runtime,
        sources_read,
        pallets: declarations.pallets,
        pallet_crates: declarations.pallet_crates,
        pallet_names: declarations.pallet_names,
        str_params: declarations.str_params,
        unit_asset_transactor_only,
        local_outbound_xcm_privileged_only,
        modules: modules
            .files
            .into_iter()
            .map(|(path, module)| (path, module.gate))
            .collect(),
    }
}

/// The canonical directory and parsed manifest of the workspace that owns the
/// package at `root`: the `package.workspace` path, or else the nearest
/// directory from `root` upwards whose `Cargo.toml` has a `[workspace]` table.
///
/// The search stops at a directory with a `Cargo.lock` and no `[workspace]`:
/// Cargo writes the lockfile at the workspace root, so the package there is
/// its own workspace even when a parent workspace excludes it.
fn workspace_manifest(
    root: &Path,
    manifest: Option<&toml::Value>,
) -> Option<(PathBuf, Arc<toml::Value>)> {
    // Every crate of a workspace reads the same ancestor manifests, so each
    // directory is read once per scan.
    let read = |dir: &Path| {
        scan_cached(WorkspaceManifestKey(dir.to_path_buf()), |key| {
            let dir = &key.0;
            let manifest = fs::read_to_string(dir.join("Cargo.toml"))
                .ok()?
                .parse::<toml::Value>()
                .ok()?;
            manifest.get("workspace")?;
            let dir = fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
            Some((dir, Arc::new(manifest)))
        })
    };
    let explicit = manifest
        .and_then(|manifest| manifest.get("package")?.get("workspace")?.as_str())
        .map(|path| root.join(path));
    match explicit {
        Some(dir) => read(&dir),
        None => {
            for dir in root.ancestors() {
                if let Some(workspace) = read(dir) {
                    return Some(workspace);
                }
                if dir.join("Cargo.lock").is_file() {
                    return None;
                }
            }
            None
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
struct WorkspaceManifestKey(PathBuf);

/// The version Cargo writes in `Cargo.lock` for the package: the manifest
/// string, the `[workspace.package]` version for `version.workspace = true`,
/// or `0.0.0` when the manifest has no version. `None` when an inherited
/// version is not stated.
fn lock_version(
    manifest: Option<&toml::Value>,
    workspace: Option<&(PathBuf, Arc<toml::Value>)>,
) -> Option<String> {
    match manifest?.get("package")?.get("version") {
        None => Some("0.0.0".to_string()),
        Some(toml::Value::String(version)) => Some(version.clone()),
        Some(version) if version.get("workspace")?.as_bool()? => workspace?
            .1
            .get("workspace")?
            .get("package")?
            .get("version")?
            .as_str()
            .map(ToOwned::to_owned),
        Some(_) => None,
    }
}

/// The crate's module tree: every file reachable through `mod name;`, with the
/// least restrictive `cfg` gate on any path that reaches it.
#[derive(Default)]
struct ModuleTree {
    files: HashMap<PathBuf, Module>,
}

struct Module {
    gate: Gate,
    /// Every path reaching the file passes a `cfg` mentioning a `feature`.
    featured: bool,
    /// Module path from the crate root. `None` when several module paths
    /// reach the file.
    path: Option<Vec<String>>,
    file: SynFile,
}

impl ModuleTree {
    /// `owns_dir`: the file is a crate root or `mod.rs`, so its child modules
    /// live next to it rather than in a directory named after it.
    fn walk(
        &mut self,
        path: &Path,
        owns_dir: bool,
        gate: Gate,
        featured: bool,
        module_path: Option<&[String]>,
    ) {
        let Ok(canonical) = fs::canonicalize(path) else {
            return;
        };
        if let Some(known) = self.files.get_mut(&canonical) {
            let same_path = known.path.as_deref() == module_path;
            if same_path && known.gate <= gate && (!known.featured || featured) {
                return;
            }
            if !same_path {
                known.path = None;
            }
            known.gate = known.gate.min(gate);
            known.featured &= featured;
        } else {
            let Some(file) = fs::read_to_string(&canonical)
                .ok()
                .and_then(|content| syn::parse_file(&content).ok())
            else {
                return;
            };
            self.files.insert(
                canonical.clone(),
                Module {
                    gate,
                    featured,
                    path: module_path.map(<[String]>::to_vec),
                    file,
                },
            );
        }

        let parent = canonical.parent().unwrap_or(Path::new("")).to_path_buf();
        let child_dir = if owns_dir {
            parent.clone()
        } else {
            parent.join(canonical.file_stem().unwrap_or_default())
        };
        let module = &self.files[&canonical];
        let (gate, featured) = (module.gate, module.featured);
        let gate = gate.max(attrs_gate(&module.file.attrs));
        let featured = featured || feature_gated(&module.file.attrs);
        let items = module.file.items.clone();
        let module_path = module.path.clone();
        self.walk_items(
            &items,
            &parent,
            &child_dir,
            gate,
            featured,
            module_path.as_deref(),
        );
    }

    /// `path_base`: the directory a `#[path]` on a file module of `items` is
    /// relative to. It is the file's directory, or `child_dir` for the items
    /// of an inline module.
    fn walk_items(
        &mut self,
        items: &[Item],
        path_base: &Path,
        child_dir: &Path,
        gate: Gate,
        featured: bool,
        parent_module: Option<&[String]>,
    ) {
        for item in items {
            let Item::Mod(module) = item else {
                continue;
            };
            let gate = gate.max(attrs_gate(&module.attrs));
            let featured = featured || feature_gated(&module.attrs);
            let explicit = path_attr(&module.attrs);
            let name = module.ident.to_string();
            let path = parent_module.map(|parent| [parent, &[ident_text(&module.ident)]].concat());
            match &module.content {
                Some((_, items)) => {
                    let dir = explicit
                        .map(|path| child_dir.join(path))
                        .unwrap_or_else(|| child_dir.join(&name));
                    self.walk_items(items, &dir, &dir, gate, featured, path.as_deref());
                }
                None => {
                    if let Some(explicit) = explicit {
                        let file = path_base.join(explicit);
                        self.walk(&file, true, gate, featured, path.as_deref());
                        continue;
                    }
                    let flat = child_dir.join(format!("{name}.rs"));
                    if flat.is_file() {
                        self.walk(&flat, false, gate, featured, path.as_deref());
                    } else {
                        let file = child_dir.join(&name).join("mod.rs");
                        self.walk(&file, true, gate, featured, path.as_deref());
                    }
                }
            }
        }
    }
}

/// Additional pallet crates that can send outbound XCM outside `pallet_xcm`.
const OTHER_XCM_SENDER_PALLETS: &[&str] = &["orml_xtokens", "orml_xcm", "pallet_xcm_transactor"];

/// `construct_runtime!`, `#[frame_support::runtime]` (or a bare `#[runtime]`
/// whose module uses `#[runtime::*]` attributes) and `#[frame_construct_runtime]`
/// declarations, split by whether a `feature` cfg (on the item, an enclosing
/// module, or the file) gates them.
#[derive(Default)]
struct Declarations {
    ungated: usize,
    featured: usize,
    pallets: Vec<(String, String)>,
    pallet_crates: HashMap<String, Vec<Option<String>>>,
    pallet_names: HashSet<String>,
    str_params: HashMap<String, String>,
    /// Crate paths of the self types of the impls whose trait path resolves
    /// to `xcm_executor::Config`.
    executors: HashSet<Vec<String>>,
    /// Some impl has an `AssetTransactor` other than `()`, or an executor impl
    /// has none.
    active_asset_transactor: bool,
    /// Crate path of `X` in each `pallet_xcm::Config` `type XcmExecutor =
    /// XcmExecutor<X>`. `None` when the impl has no such item, the type has
    /// another shape or `X` does not resolve.
    pallet_xcm_executors: Vec<Option<Vec<String>>>,
    /// Crate path of `X` in every `XcmExecutor<X>`, in types and expressions.
    /// `None` when `X` does not resolve.
    executor_refs: Vec<Option<Vec<String>>>,
    has_pallet_xcm_config: bool,
    any_pallet_xcm_config_open: bool,
    has_other_xcm_sender: bool,
}

impl Declarations {
    /// A runtime crate builds exactly one runtime, or switches between
    /// runtimes by feature. Crates that declare several ungated runtimes (XCM
    /// simulators, emulated networks, documentation cookbooks) are test or
    /// example harnesses, not runtimes.
    fn is_runtime(&self) -> bool {
        self.ungated == 1 || (self.ungated == 0 && self.featured >= 1)
    }

    fn local_outbound_xcm_privileged_only(&self) -> bool {
        let has_other_xcm_pallet = self
            .pallet_crates
            .values()
            .flatten()
            .flatten()
            .any(|krate| OTHER_XCM_SENDER_PALLETS.contains(&krate.as_str()));
        self.has_pallet_xcm_config
            && !self.any_pallet_xcm_config_open
            && !self.has_other_xcm_sender
            && !has_other_xcm_pallet
    }

    /// Every `AssetTransactor` in the crate is `()`, and `pallet_xcm` and every
    /// other `XcmExecutor<X>` name an `X` that resolves to the path of a
    /// visible `xcm_executor::Config` impl. An executor that `include!` or a
    /// macro expands is not visible, so a `pallet_xcm` that names it keeps the
    /// finding.
    fn unit_asset_transactor_only(&self) -> bool {
        let visible = |executor: &Option<Vec<String>>| {
            executor
                .as_ref()
                .is_some_and(|executor| self.executors.contains(executor))
        };
        !self.active_asset_transactor
            && !self.executors.is_empty()
            && !self.pallet_xcm_executors.is_empty()
            && self.pallet_xcm_executors.iter().all(visible)
            && self.executor_refs.iter().all(visible)
    }

    /// `module`: the file's module path from the crate root, or `None` when
    /// it is not known.
    fn count(&mut self, file: &SynFile, featured: bool, module: Option<&[String]>) {
        let featured = featured || feature_gated(&file.attrs);
        let mut finder = Finder {
            counts: self,
            featured,
            aliases: module_aliases(&file.items),
            uses: module_uses(&file.items),
            module: module.map(<[String]>::to_vec),
        };
        for item in &file.items {
            finder.visit_item(item);
        }
    }
}

struct Finder<'a> {
    counts: &'a mut Declarations,
    featured: bool,
    /// Aliases declared at the module level of the item being visited. An
    /// alias from another module resolves to nothing, so an impl's
    /// `AssetTransactor` or XCM sender is not `()` unless its own module says so.
    aliases: Aliases,
    /// `use` imports of the module of the item being visited.
    uses: Uses,
    /// Path from the crate root of the module of the item being visited.
    /// `None` inside a block or a file that several module paths reach.
    module: Option<Vec<String>>,
}

/// First crate identifier after `:` in a `construct_runtime!` entry
/// (`Name: pallet_foo = 1` or `Name: pallet_foo::{Pallet, Call} = 1`).
fn construct_runtime_pallet_crate(entry: &[TokenTree]) -> Option<String> {
    let colon = entry
        .iter()
        .position(|tree| matches!(tree, TokenTree::Punct(p) if p.as_char() == ':'))?;
    entry[colon + 1..].iter().find_map(|tree| match tree {
        TokenTree::Ident(ident) => Some(ident.to_string()),
        _ => None,
    })
}

/// First path segment of a `#[frame_support::runtime]` pallet type: `pallet_foo`
/// in `pallet_foo`, `pallet_foo::Pallet<Runtime>` or `pallet_foo + Call`.
fn runtime_type_pallet_crate(ty: &Type) -> Option<String> {
    let path = match ty {
        Type::TraitObject(object) => object.bounds.iter().find_map(|bound| match bound {
            syn::TypeParamBound::Trait(bound) => Some(&bound.path),
            _ => None,
        })?,
        ty => type_path(ty)?,
    };
    path.segments
        .first()
        .map(|segment| segment.ident.to_string())
}

/// The path from the crate root that `path` names when written in `module`
/// under the `use` imports `uses`. A first segment that no `use` imports is an
/// item of `module`, so an extern crate path does not resolve to a local item.
/// `None` for a `::` path, a name with several imports or a `super` above the
/// crate root.
fn resolve_path(path: &syn::Path, module: &[String], uses: &Uses) -> Option<Vec<String>> {
    if path.leading_colon.is_some() {
        return None;
    }
    let idents = path
        .segments
        .iter()
        .map(|segment| ident_text(&segment.ident))
        .collect::<Vec<_>>();
    let (first, rest) = idents.split_first()?;
    let written = match uses.get(first).map(Vec::as_slice) {
        Some([import]) => import.iter().chain(rest).cloned().collect::<Vec<_>>(),
        Some(_) => return None,
        None => idents.clone(),
    };
    let mut segments = written.into_iter().peekable();
    let mut resolved = if segments.next_if(|segment| segment == "crate").is_some() {
        Vec::new()
    } else {
        module.to_vec()
    };
    loop {
        if segments.next_if(|segment| segment == "self").is_some() {
            continue;
        }
        if segments.next_if(|segment| segment == "super").is_some() {
            resolved.pop()?;
            continue;
        }
        break;
    }
    resolved.extend(segments);
    Some(resolved)
}

impl Finder<'_> {
    /// The crate path that `path` names in the module being visited.
    fn resolve(&self, path: &syn::Path) -> Option<Vec<String>> {
        resolve_path(path, self.module.as_deref()?, &self.uses)
    }

    /// Segment `index` of `path` names `XcmExecutor`, directly or as the
    /// first segment through a `use` rename.
    fn names_xcm_executor(&self, path: &syn::Path, index: usize) -> bool {
        let name = ident_text(&path.segments[index].ident);
        name == "XcmExecutor"
            || (index == 0
                && path.leading_colon.is_none()
                && self.uses.get(&name).is_some_and(|imports| {
                    imports
                        .iter()
                        .any(|import| import.last().is_some_and(|last| last == "XcmExecutor"))
                }))
    }

    /// Crate path of `X` in an `XcmExecutor<X>` segment.
    fn executor_arg(&self, segment: &syn::PathSegment) -> Option<Vec<String>> {
        match generic_type_args(segment).as_slice() {
            [arg] => type_path(arg).and_then(|path| self.resolve(path)),
            _ => None,
        }
    }

    /// Crate path of `X` when `ty` is `XcmExecutor<X>`.
    fn xcm_executor_arg(&self, ty: &Type) -> Option<Vec<String>> {
        let path = type_path(ty)?;
        let last = path.segments.len().checked_sub(1)?;
        if !self.names_xcm_executor(path, last) {
            return None;
        }
        self.executor_arg(&path.segments[last])
    }

    fn found(&mut self, attrs: &[Attribute]) {
        if self.featured || feature_gated(attrs) {
            self.counts.featured += 1;
        } else {
            self.counts.ungated += 1;
        }
    }

    /// Record a declared pallet, its crate, and value-moving crates. Every
    /// declaration of a variant is kept, so `#[cfg]` alternatives add up. A
    /// known XCM sender crate anywhere in the path (`my_pallets::pallet_xcm_transactor`)
    /// counts as an XCM sender.
    fn pallet(&mut self, name: Option<String>, crate_name: Option<String>, tokens: TokenStream) {
        let Some(name) = name else { return };
        if find_ident(tokens.clone(), OTHER_XCM_SENDER_PALLETS).is_some() {
            self.counts.has_other_xcm_sender = true;
        }
        let value_moving = find_ident(tokens, VALUE_MOVING_PALLETS).map(|(krate, _)| krate);
        if let Some(krate) = &value_moving {
            let pallet = (name.clone(), krate.clone());
            if !self.counts.pallets.contains(&pallet) {
                self.counts.pallets.push(pallet);
            }
        }
        let krate = value_moving.or(crate_name);
        let crates = self.counts.pallet_crates.entry(name.clone()).or_default();
        if !crates.contains(&krate) {
            crates.push(krate);
        }
        self.counts.pallet_names.insert(name);
    }

    fn str_param(&mut self, name: String, value: &[TokenTree]) {
        if let [TokenTree::Literal(literal)] = value {
            if let Lit::Str(value) = Lit::new(literal.clone()) {
                self.counts.str_params.insert(name, value.value());
            }
        }
    }
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_item_macro(&mut self, item: &'ast ItemMacro) {
        if path_last_ident(&item.mac.path).is_some_and(|name| is_construct_runtime(&name)) {
            self.found(&item.attrs);
            for entry in construct_runtime_entries(item.mac.tokens.clone()) {
                // Attributes are `#[..]` groups; the first ident is the name.
                let name = entry.iter().find_map(|tree| match tree {
                    TokenTree::Ident(ident) => Some(ident.to_string()),
                    _ => None,
                });
                let crate_name = construct_runtime_pallet_crate(&entry);
                self.pallet(name, crate_name, entry.into_iter().collect());
            }
        } else if path_last_ident(&item.mac.path).as_deref() == Some("parameter_types") {
            for (name, _, value) in parameter_type_consts(item.mac.tokens.clone()) {
                self.str_param(name, &value);
            }
        }
    }
    fn visit_item_const(&mut self, item: &'ast ItemConst) {
        let value = item.expr.to_token_stream().into_iter().collect::<Vec<_>>();
        self.str_param(item.ident.to_string(), &value);
    }
    fn visit_item_impl(&mut self, item_impl: &'ast ItemImpl) {
        let broad_executor = is_xcm_executor_impl(item_impl, &self.uses);
        let executor = is_executor_config_impl(item_impl, &self.uses);
        let is_pallet_xcm = is_pallet_xcm_impl(item_impl);
        let other_config =
            !broad_executor && !is_pallet_xcm && runtime_config_impl(item_impl).is_some();
        let transactors = impl_types_named(item_impl, "AssetTransactor");
        if broad_executor || is_pallet_xcm || other_config || !transactors.is_empty() {
            // Type parameters shadow module aliases inside the impl.
            let aliases = if item_impl.generics.type_params().next().is_some() {
                std::borrow::Cow::Owned(without_generic_params(&self.aliases, &item_impl.generics))
            } else {
                std::borrow::Cow::Borrowed(&self.aliases)
            };
            if executor {
                if let Some(key) = type_path(&item_impl.self_ty).and_then(|path| self.resolve(path))
                {
                    self.counts.executors.insert(key);
                }
            }
            self.counts.active_asset_transactor |= (broad_executor && transactors.is_empty())
                || transactors
                    .iter()
                    .any(|item| !is_unit_or_alias_unit(&item.ty, &aliases, 0));
            if is_pallet_xcm {
                self.counts.has_pallet_xcm_config = true;
                if !pallet_xcm_outbound_is_privileged(item_impl, &aliases) {
                    self.counts.any_pallet_xcm_config_open = true;
                }
                let executors = impl_types_named(item_impl, "XcmExecutor");
                if executors.is_empty() {
                    self.counts.pallet_xcm_executors.push(None);
                }
                for item in executors {
                    let executor = self.xcm_executor_arg(&item.ty);
                    self.counts.pallet_xcm_executors.push(executor);
                }
            } else if other_config {
                // Any other pallet Config impl declaring an XcmSender / XcmRouter /
                // SendXcm associated type can emit outbound XCM independently of
                // `pallet_xcm`.
                let sends_xcm = impl_types(item_impl).any(|item| {
                    matches!(
                        item.ident.to_string().as_str(),
                        "XcmSender" | "XcmRouter" | "SendXcm"
                    ) && !is_unit_or_alias_unit(&item.ty, &aliases, 0)
                });
                self.counts.has_other_xcm_sender |= sends_xcm;
            }
        }
        visit::visit_item_impl(self, item_impl);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        // Every segment, so a turbofish `XcmExecutor::<X>::execute(..)` counts.
        for (index, segment) in path.segments.iter().enumerate() {
            if self.names_xcm_executor(path, index) {
                let executor = self.executor_arg(segment);
                self.counts.executor_refs.push(executor);
            }
        }
        visit::visit_path(self, path);
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        // Items in a block see the block's own items first, which the module
        // aliases and imports do not describe.
        let outer = std::mem::take(&mut self.aliases);
        let outer_uses = std::mem::take(&mut self.uses);
        let outer_module = self.module.take();
        visit::visit_block(self, block);
        self.aliases = outer;
        self.uses = outer_uses;
        self.module = outer_module;
    }
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if is_runtime_module(item) {
            self.found(&item.attrs);
            for item in item.content.iter().flat_map(|(_, items)| items) {
                if let Item::Type(item_type) = item {
                    self.pallet(
                        Some(item_type.ident.to_string()),
                        runtime_type_pallet_crate(&item_type.ty),
                        item_type.ty.to_token_stream(),
                    );
                }
            }
        }
        let outer = self.featured;
        self.featured |= feature_gated(&item.attrs);
        let outer_scope = item.content.as_ref().map(|(_, items)| {
            let module = self
                .module
                .as_ref()
                .map(|module| [module.as_slice(), &[ident_text(&item.ident)]].concat());
            (
                std::mem::replace(&mut self.aliases, module_aliases(items)),
                std::mem::replace(&mut self.uses, module_uses(items)),
                std::mem::replace(&mut self.module, module),
            )
        });
        visit::visit_item_mod(self, item);
        if let Some((aliases, uses, module)) = outer_scope {
            self.aliases = aliases;
            self.uses = uses;
            self.module = module;
        }
        self.featured = outer;
    }
}

fn is_runtime_module(item: &ItemMod) -> bool {
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
    let items = item
        .content
        .as_ref()
        .map(|(_, items)| items.as_slice())
        .unwrap_or_default();
    item.attrs.iter().any(|attr| {
        let segments = attr_segments(attr);
        match segments.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            [.., "frame_construct_runtime"] => true,
            [_, .., "runtime"] => true,
            ["runtime"] => has_runtime_item_attr(items),
            _ => false,
        }
    })
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

/// One of `tokens` is a whole name token of the package or directory `name`,
/// e.g. `test` in `polkadot-test-runtime` but not in `mychain-testnet-runtime`.
fn has_name_token(name: &str, tokens: &[String]) -> bool {
    name_tokens(name).any(|token| tokens.iter().any(|t| t.eq_ignore_ascii_case(&token)))
}

/// Returns `true` when `krate` is a runtime crate that passes the configured
/// test, dev, and testnet runtime filters.
fn is_included_runtime_crate(krate: &RuntimeCrate, config: &RuntimeConfigConfig) -> bool {
    if !krate.is_runtime {
        return false;
    }
    let Some(root) = &krate.root else {
        return true;
    };
    let dir_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let package = krate.package.as_deref().unwrap_or_default();
    let named =
        |tokens: &[String]| has_name_token(&dir_name, tokens) || has_name_token(package, tokens);
    if named(&config.test_runtime_tokens) {
        return false;
    }
    if !config.include_dev_runtimes
        && (named(&config.dev_runtime_tokens)
            || config.dev_runtime_packages.iter().any(|p| p == package))
    {
        return false;
    }
    if config.excludes_testnet_runtimes() && named(&config.testnet_runtime_tokens) {
        return false;
    }
    true
}

/// The runtime crates that a scan lints, gathered before the scan so that
/// `exclude_testnet_runtimes` is decided once for every scan root.
pub(crate) struct ScannedRuntimes {
    /// The scan's runtime config with testnet runtimes linted.
    config: RuntimeConfigConfig,
    crates: Vec<Arc<RuntimeCrate>>,
}

impl ScannedRuntimes {
    pub(crate) fn new(config: &RuntimeConfigConfig) -> Self {
        let mut config = config.clone();
        config.exclude_testnet_runtimes = Some(false);
        Self {
            config,
            crates: Vec::new(),
        }
    }

    fn add(&mut self, krate: Arc<RuntimeCrate>) {
        let known = self.crates.iter().any(|known| {
            Arc::ptr_eq(known, &krate) || (known.root.is_some() && known.root == krate.root)
        });
        if !known {
            self.crates.push(krate);
        }
    }

    /// Adds the runtime crate that the source rules lint `path` as part of,
    /// with testnet runtimes included. The caller passes only a Rust file
    /// that the engine does not classify as test or benchmark code.
    pub(crate) fn add_source(&mut self, path: &Path, rel_path: &Path) {
        let krate = runtime_crate_at(path, || {
            fs::read_to_string(path)
                .ok()
                .and_then(|content| syn::parse_file(&content).ok())
        });
        if let Some(krate) = source_scope(krate, path, rel_path, &self.config) {
            self.add(krate);
        }
    }

    /// Adds the local runtimes that RTC009 roots in the `Cargo.lock` at
    /// `path`, when it has an affected package.
    pub(crate) fn add_lock(&mut self, path: &Path) {
        if path.file_name().is_none_or(|name| name != "Cargo.lock")
            || !lock_has_affected_package(path)
        {
            return;
        }
        let lock_dir = match path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir,
            _ => Path::new("."),
        };
        let lock_root = fs::canonicalize(lock_dir).unwrap_or_else(|_| lock_dir.to_path_buf());
        for krate in &workspace_runtimes(lock_dir) {
            if !is_other_workspace(krate, &lock_root)
                && is_included_runtime_crate(krate, &self.config)
            {
                self.add(Arc::clone(krate));
            }
        }
    }

    /// Names of the runtimes that `exclude_testnet_runtimes` skips, and
    /// whether they are every runtime the scan lints. A name is the package
    /// name, or the directory name for a package without one.
    pub(crate) fn testnets(&self, config: &RuntimeConfigConfig) -> (Vec<String>, bool) {
        let (kept, skipped): (Vec<_>, Vec<_>) = self
            .crates
            .iter()
            .partition(|krate| is_included_runtime_crate(krate, config));
        let mut names = skipped
            .iter()
            .map(|krate| {
                krate.package.clone().unwrap_or_else(|| {
                    krate
                        .root
                        .as_deref()
                        .and_then(Path::file_name)
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
            })
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        let all = kept.is_empty() && !names.is_empty();
        (names, all)
    }
}

/// Shared gate: the file is non-test source of a production (or opted-in dev)
/// runtime crate.
fn runtime_scope(ctx: &FileContext, config: &RuntimeConfigConfig) -> Option<Arc<RuntimeCrate>> {
    if !ctx.is_rust || ctx.is_test_file || ctx.is_benchmark_file || ctx.ast.is_none() {
        return None;
    }
    let krate = runtime_crate_at(&ctx.path, || ctx.ast.clone());
    source_scope(krate, &ctx.path, &ctx.rel_path, config)
}

/// `krate` when the file at `path` is production source of it and the runtime
/// filters in `config` keep it.
fn source_scope(
    krate: Arc<RuntimeCrate>,
    path: &Path,
    rel_path: &Path,
    config: &RuntimeConfigConfig,
) -> Option<Arc<RuntimeCrate>> {
    if !is_included_runtime_crate(&krate, config) {
        return None;
    }
    if krate.root.is_none() {
        // Single-file fallback: only the scan-relative path is known.
        return (!is_test_path(rel_path)).then_some(krate);
    }

    // `krate.root` is canonical and `path` may not be, so strip the
    // manifest directory as spelt in `path`.
    let in_crate = manifest_dir(path)
        .and_then(|dir| path.strip_prefix(dir).ok())
        .unwrap_or(path);
    if is_test_path(in_crate) {
        return None;
    }
    // Files reached only through `#[cfg(test)]` or benchmark-gated `mod`
    // declarations are not production code. Files the module walk could not
    // reach (macro-generated modules, `include!`) are kept.
    let gate = fs::canonicalize(path)
        .ok()
        .and_then(|path| krate.modules.get(&path).copied());
    if gate.is_some_and(|gate| gate != Gate::Production) {
        return None;
    }
    Some(krate)
}

// ---------------------------------------------------------------------------
// Non-production cfg filtering
// ---------------------------------------------------------------------------

const NON_PRODUCTION_FEATURES: &[&str] = &["runtime-benchmarks", "try-runtime", "test-helpers"];

/// `leaf` holds for the cfg predicate: for `all(..)` some operand matches, for
/// `any(..)` every operand does.
pub(crate) fn cfg_matches(meta: &Meta, leaf: &dyn Fn(&Meta) -> bool) -> bool {
    let Meta::List(list) = meta else {
        return leaf(meta);
    };
    let Ok(nested) =
        list.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
    else {
        return false;
    };
    if list.path.is_ident("all") {
        nested.iter().any(|meta| cfg_matches(meta, leaf))
    } else if list.path.is_ident("any") {
        !nested.is_empty() && nested.iter().all(|meta| cfg_matches(meta, leaf))
    } else {
        false
    }
}

fn is_test_cfg(meta: &Meta) -> bool {
    matches!(meta, Meta::Path(path) if path.is_ident("test"))
}

fn is_non_production_cfg(meta: &Meta) -> bool {
    is_test_cfg(meta)
        || matches!(meta, Meta::NameValue(name_value) if name_value.path.is_ident("feature")
        && matches!(
            &name_value.value,
            Expr::Lit(expr) if matches!(
                &expr.lit,
                Lit::Str(feature) if NON_PRODUCTION_FEATURES.contains(&feature.value().as_str())
            )
        ))
}

/// A `#[cfg(..)]` whose predicate mentions `feature`, at any depth.
fn feature_gated(attrs: &[Attribute]) -> bool {
    fn mentions(tokens: TokenStream) -> bool {
        tokens.into_iter().any(|token| match token {
            TokenTree::Ident(ident) => ident == "feature",
            TokenTree::Group(group) => mentions(group.stream()),
            _ => false,
        })
    }
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .meta
                .require_list()
                .is_ok_and(|list| mentions(list.tokens.clone()))
    })
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
                Ok(meta) if cfg_matches(&meta, &is_test_cfg) => Gate::Test,
                Ok(meta) if cfg_matches(&meta, &is_non_production_cfg) => Gate::NonProduction,
                _ => Gate::Production,
            }
        })
        .max()
        .unwrap_or(Gate::Production)
}

fn is_non_production(attrs: &[Attribute]) -> bool {
    attrs_gate(attrs) != Gate::Production
}

/// `#` followed by a bracketed group, as found in macro input, is a
/// non-production attribute.
fn attr_tokens_non_production(tokens: &[TokenTree]) -> bool {
    Attribute::parse_outer
        .parse2(tokens.iter().cloned().collect())
        .is_ok_and(|attrs| is_non_production(&attrs))
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
    if path_last_ident(trait_path).as_deref() != Some("Config") || trait_path.segments.len() < 2 {
        return None;
    }
    let self_ident = type_path(&item_impl.self_ty).and_then(path_last_ident)?;
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

fn impl_types_named<'a>(item_impl: &'a ItemImpl, name: &str) -> Vec<&'a ImplItemType> {
    impl_types(item_impl)
        .filter(|item_type| item_type.ident == name)
        .collect()
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
        confidence: default_confidence(rule.id()),
        file: ctx.path.clone(),
        line,
        column: None,
        end_line: None,
        message,
        explanation: explanation.to_string(),
        suggestion: Some(suggestion.to_string()),
        also_in: Vec::new(),
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
// RTC001: test WeightInfo in a runtime Config impl
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
        runtime_scope(ctx, &config.runtime_config)?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Warning);

        let mut diagnostics = Vec::new();
        for_each_runtime_impl(&file, |item_impl, trait_path| {
            let pallet = trait_path[..trait_path.len() - 1].join("::");
            for item_type in impl_types(item_impl) {
                if !is_weight_info_ident(&item_type.ident.to_string()) {
                    continue;
                }
                let Some(path) = type_path(&item_type.ty) else {
                    continue;
                };
                if !path_idents(path)
                    .iter()
                    .any(|segment| segment.starts_with("TestWeightInfo"))
                {
                    continue;
                }
                diagnostics.push(diagnostic(
                    self,
                    ctx,
                    severity,
                    line_of(&item_type.ty),
                    format!(
                        "`{pallet}::Config for Runtime` uses test weights `{}`",
                        path.to_token_stream().to_string().replace(' ', "")
                    ),
                    "`TestWeightInfo` types are placeholders for mocks and unit tests; they are \
                     not benchmarked and return zero or flat, arbitrary weights.",
                    "Benchmark the pallet in this runtime via `define_benchmarks!` and bind the \
                     generated `weights::<pallet>::WeightInfo<Runtime>`.",
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
            .and_then(path_last_ident)
            .is_some_and(|ident| ident == "NoPriceForMessageDelivery")
}

/// The impl may be an XCM executor config: its trait is `xcm_executor::Config`
/// (see [`is_executor_config_impl`]), or a `Config` that a module path names
/// or whose self type is `XcmConfig`. RTC003 and RTC006 check every such impl.
fn is_xcm_executor_impl(item_impl: &ItemImpl, uses: &Uses) -> bool {
    let Some((_, trait_path, _)) = &item_impl.trait_ else {
        return false;
    };
    if is_executor_config_impl(item_impl, uses) {
        return true;
    }
    if path_last_ident(trait_path).as_deref() != Some("Config") {
        return false;
    }
    path_idents(trait_path)
        .iter()
        .any(|segment| segment == "xcm_executor" || segment == "staging_xcm_executor")
        || type_path(&item_impl.self_ty)
            .and_then(path_last_ident)
            .is_some_and(|ident| ident == "XcmConfig")
}

/// The impl's trait path resolves to `xcm_executor::Config` or
/// `staging_xcm_executor::Config`, through the module's `use` imports and
/// renames. A name with several imports resolves only when each one does.
fn is_executor_config_impl(item_impl: &ItemImpl, uses: &Uses) -> bool {
    let Some((_, trait_path, _)) = &item_impl.trait_ else {
        return false;
    };
    let idents = trait_path
        .segments
        .iter()
        .map(|segment| ident_text(&segment.ident))
        .collect::<Vec<_>>();
    let Some((first, rest)) = idents.split_first() else {
        return false;
    };
    let is_executor_config = |path: &[String]| {
        path.last().is_some_and(|last| last == "Config")
            && path
                .iter()
                .any(|segment| segment == "xcm_executor" || segment == "staging_xcm_executor")
    };
    match uses.get(first) {
        Some(imports) => imports.iter().all(|import| {
            is_executor_config(&import.iter().chain(rest).cloned().collect::<Vec<_>>())
        }),
        None => is_executor_config(&idents),
    }
}

/// Names that a module's `use` items bring into scope, each with the full
/// path of every import of it. Glob imports are left out.
type Uses = HashMap<String, Vec<Vec<String>>>;

fn module_uses(items: &[Item]) -> Uses {
    fn walk(tree: &syn::UseTree, prefix: &mut Vec<String>, uses: &mut Uses) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(ident_text(&path.ident));
                walk(&path.tree, prefix, uses);
                prefix.pop();
            }
            syn::UseTree::Name(name) if name.ident == "self" => {
                if let Some(last) = prefix.last() {
                    uses.entry(last.clone()).or_default().push(prefix.clone());
                }
            }
            syn::UseTree::Name(name) => {
                let mut path = prefix.clone();
                path.push(ident_text(&name.ident));
                uses.entry(ident_text(&name.ident)).or_default().push(path);
            }
            syn::UseTree::Rename(rename) => {
                let mut path = prefix.clone();
                if rename.ident != "self" {
                    path.push(ident_text(&rename.ident));
                }
                uses.entry(ident_text(&rename.rename))
                    .or_default()
                    .push(path);
            }
            syn::UseTree::Glob(_) => {}
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    walk(tree, prefix, uses);
                }
            }
        }
    }
    let mut uses = Uses::new();
    for item in items {
        if let Item::Use(item) = item {
            walk(&item.tree, &mut Vec::new(), &mut uses);
        }
    }
    uses
}

/// An identifier without its `r#` raw prefix, so `r#Transactor` and
/// `Transactor` are the same name.
fn ident_text(ident: &syn::Ident) -> String {
    use syn::ext::IdentExt;
    ident.unraw().to_string()
}

/// A name without its `r#` raw prefix.
fn unraw_name(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

/// True when `ty` is `()` or resolves to `()` through unambiguous aliases in
/// `aliases`. An ambiguous or unknown alias returns `false` (fail closed), so
/// the caller sets the alias scope.
fn is_unit_or_alias_unit(ty: &Type, aliases: &Aliases, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    if is_unit_type(ty) {
        return true;
    }
    if let Type::Paren(inner) = ty {
        return is_unit_or_alias_unit(&inner.elem, aliases, depth + 1);
    }
    let Some(path) = type_path(ty) else {
        return false;
    };
    let Some(segment) = path.segments.last() else {
        return false;
    };
    if path.segments.len() == 1 && segment.arguments.is_empty() {
        return unique_alias(aliases, &segment.ident.to_string())
            .is_some_and(|alias| is_unit_or_alias_unit(alias, aliases, depth + 1));
    }
    false
}

/// Same-file `type Name = ...;` aliases: every distinct right-hand side of a
/// name. More than one (in different modules or under different cfgs) is
/// ambiguous.
type Aliases = HashMap<String, Vec<Type>>;

fn type_aliases(file: &SynFile) -> Aliases {
    struct Collector(Aliases);
    impl<'ast> Visit<'ast> for Collector {
        fn visit_item_type(&mut self, item: &'ast ItemType) {
            if item.generics.params.is_empty() {
                let text = |ty: &Type| ty.to_token_stream().to_string();
                let known = self.0.entry(ident_text(&item.ident)).or_default();
                if known.iter().all(|ty| text(ty) != text(&item.ty)) {
                    known.push((*item.ty).clone());
                }
            }
        }
    }
    let mut aliases = Collector(HashMap::new());
    aliases.visit_file(file);
    aliases.0
}

/// Adds every name that `tree` brings into scope to `names`. Sets `glob` for a
/// `*` import, which can bring any name into scope.
fn use_tree_names(
    tree: &syn::UseTree,
    parent: Option<&syn::Ident>,
    names: &mut Vec<String>,
    glob: &mut bool,
) {
    match tree {
        syn::UseTree::Path(path) => use_tree_names(&path.tree, Some(&path.ident), names, glob),
        syn::UseTree::Name(name) if name.ident == "self" => {
            names.extend(parent.map(ident_text));
        }
        syn::UseTree::Name(name) => names.push(ident_text(&name.ident)),
        syn::UseTree::Rename(rename) => names.push(ident_text(&rename.rename)),
        syn::UseTree::Glob(_) => *glob = true,
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                use_tree_names(tree, parent, names, glob);
            }
        }
    }
}

/// `type Name = ...;` aliases declared directly in `items`, not in nested
/// modules, that resolve without doubt.
///
/// An alias resolves only when its `type` item is the single item of the
/// module that defines the name and has no `#[cfg]` or `#[cfg_attr]`. A `use`,
/// `struct`, `enum`, gated item or second `type` of the same name makes the
/// name unknown, and a glob import makes every name unknown. An unknown name
/// is never `()`, so a check that needs `()` fails closed.
fn module_aliases(items: &[Item]) -> Aliases {
    let mut definitions = HashMap::<String, usize>::new();
    let mut candidates = Aliases::new();
    let mut glob = false;
    for item in items {
        let mut names = Vec::new();
        match item {
            Item::Type(item) => {
                names.push(ident_text(&item.ident));
                if item.generics.params.is_empty() && !has_cfg(&item.attrs) {
                    candidates.insert(ident_text(&item.ident), vec![(*item.ty).clone()]);
                }
            }
            Item::Struct(item) => names.push(ident_text(&item.ident)),
            Item::Enum(item) => names.push(ident_text(&item.ident)),
            Item::Union(item) => names.push(ident_text(&item.ident)),
            Item::Trait(item) => names.push(ident_text(&item.ident)),
            Item::TraitAlias(item) => names.push(ident_text(&item.ident)),
            Item::Mod(item) => names.push(ident_text(&item.ident)),
            Item::ExternCrate(item) => names.push(ident_text(
                item.rename
                    .as_ref()
                    .map_or(&item.ident, |(_, rename)| rename),
            )),
            Item::Use(item) => use_tree_names(&item.tree, None, &mut names, &mut glob),
            _ => {}
        }
        for name in names {
            *definitions.entry(name).or_default() += 1;
        }
    }
    if glob {
        return Aliases::new();
    }
    candidates.retain(|name, _| definitions.get(name) == Some(&1));
    candidates
}

/// A `#[cfg]` or `#[cfg_attr]`, which can remove the item or give it another
/// `cfg`.
fn has_cfg(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
}

/// `aliases` without the names that `generics` declares as type parameters,
/// which shadow a module alias inside the item.
fn without_generic_params(aliases: &Aliases, generics: &syn::Generics) -> Aliases {
    let mut aliases = aliases.clone();
    for param in generics.type_params() {
        aliases.remove(&ident_text(&param.ident));
    }
    aliases
}

/// The single same-file definition of alias `ident`; `None` if it is unknown
/// or ambiguous.
fn unique_alias<'a>(aliases: &'a Aliases, ident: &str) -> Option<&'a Type> {
    match aliases.get(unraw_name(ident))?.as_slice() {
        [ty] => Some(ty),
        _ => None,
    }
}

/// The type (after resolving same-file aliases) is a filter named `name`, or a
/// tuple containing one (tuples of `Contains` are a union). An ambiguous alias
/// may be the filter, unless `every_definition` asks that each of its
/// definitions contains it.
fn contains_filter(
    ty: &Type,
    name: &str,
    aliases: &Aliases,
    every_definition: bool,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    let recurse = |ty: &Type| contains_filter(ty, name, aliases, every_definition, depth + 1);
    match ty {
        Type::Paren(inner) => recurse(&inner.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(recurse),
        _ => {
            let Some(ident) = type_path(ty).and_then(path_last_ident) else {
                return false;
            };
            ident == name
                || (type_path(ty).is_some_and(|path| path.segments.len() == 1)
                    && aliases.get(unraw_name(&ident)).is_some_and(|definitions| {
                        if every_definition {
                            definitions.iter().all(recurse)
                        } else {
                            definitions.len() > 1 || definitions.iter().any(recurse)
                        }
                    }))
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
    /// RTC003: executor `FeeManager` waives fees for every origin.
    FeeManagerWaived,
    /// RTC003: router delivery price is free (`()` or `NoPriceForMessageDelivery`).
    RouterDeliveryFree,
    /// RTC006: fees are charged and then dropped.
    Burned,
}

fn type_text(ty: &Type) -> String {
    ty.to_token_stream().to_string().replace(' ', "")
}

/// Every XCM fee finding in the production AST: kind, line, message.
fn xcm_fee_findings(file: &SynFile) -> Vec<(XcmFeeFinding, usize, String)> {
    struct Visitor<'a> {
        aliases: &'a Aliases,
        uses: Uses,
        findings: Vec<(XcmFeeFinding, usize, String)>,
        lines: HashSet<usize>,
        /// Delivery prices in the file: (any free, any priced).
        prices: (bool, bool),
        /// Fee managers of executors that send but cannot export.
        export_free_fee_managers: Vec<Type>,
    }

    impl Visitor<'_> {
        fn push(&mut self, kind: XcmFeeFinding, line: usize, message: String) {
            if self.lines.insert(line) {
                self.findings.push((kind, line, message));
            }
        }

        /// Records whether the delivery price `ty` (or the same-file alias it
        /// names) is free, and returns whether `ty` itself is, for RTC003.
        fn note_price(&mut self, ty: &Type) -> bool {
            let free = is_free_delivery_price(ty)
                || type_path(ty)
                    .filter(|path| path.segments.len() == 1)
                    .and_then(path_last_ident)
                    .and_then(|ident| unique_alias(self.aliases, &ident))
                    .is_some_and(is_free_delivery_price);
            if free {
                self.prices.0 = true;
            } else {
                self.prices.1 = true;
            }
            is_free_delivery_price(ty)
        }

        fn check_price(&mut self, ident: &str, ty: &Type) {
            if DELIVERY_PRICE_TYPES.contains(&ident) && self.note_price(ty) {
                self.push(
                    XcmFeeFinding::RouterDeliveryFree,
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
                && generic_type_args(segment).first().is_some_and(|waived| {
                    contains_filter(waived, "Everything", self.aliases, false, 0)
                });
            if ident == "WaiveDeliveryFees" || waived_everywhere {
                self.push(
                    XcmFeeFinding::FeeManagerWaived,
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
            if is_xcm_executor_impl(item_impl, &self.uses) {
                // An executor that can neither send nor export never pays a
                // delivery or export fee, so its fee manager is moot.
                let sends = impl_type_named(item_impl, "XcmSender")
                    .is_none_or(|item| !is_unit_type(&item.ty));
                let exports = impl_type_named(item_impl, "MessageExporter")
                    .is_none_or(|item| !is_unit_type(&item.ty));
                if let Some(fee_manager) = impl_type_named(item_impl, "FeeManager") {
                    if exports {
                        self.check_fee_manager(&fee_manager.ty);
                    } else if sends {
                        // Checked once every delivery price in the file is known.
                        self.export_free_fee_managers.push(fee_manager.ty.clone());
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
                        if self.note_price(price) {
                            self.push(
                                XcmFeeFinding::RouterDeliveryFree,
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
        uses: module_uses(&file.items),
        findings: Vec::new(),
        lines: HashSet::new(),
        prices: (false, false),
        export_free_fee_managers: Vec::new(),
    };
    visitor.visit_file(file);
    // Without an exporter, fees are only delivery prices: when every one in
    // the file is free, a `()` fee manager has nothing to burn.
    let all_prices_free = visitor.prices == (true, false);
    for fee_manager in std::mem::take(&mut visitor.export_free_fee_managers) {
        if !(all_prices_free && is_unit_type(&fee_manager)) {
            visitor.check_fee_manager(&fee_manager);
        }
    }
    visitor.findings
}

fn is_pallet_xcm_impl(item_impl: &ItemImpl) -> bool {
    runtime_config_impl(item_impl)
        .is_some_and(|segments| segments.iter().any(|segment| segment == "pallet_xcm"))
}

fn is_nothing_filter(ty: &Type, aliases: &Aliases, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match ty {
        Type::Paren(inner) => is_nothing_filter(&inner.elem, aliases, depth + 1),
        Type::Tuple(tuple) => {
            tuple.elems.is_empty()
                || tuple
                    .elems
                    .iter()
                    .all(|elem| is_nothing_filter(elem, aliases, depth + 1))
        }
        _ => {
            let Some(path) = type_path(ty) else {
                return false;
            };
            let Some(ident) = path_last_ident(path) else {
                return false;
            };
            ident == "Nothing"
                || (path.segments.len() == 1
                    && unique_alias(aliases, &ident)
                        .is_some_and(|alias| is_nothing_filter(alias, aliases, depth + 1)))
        }
    }
}

/// Origin is provably privileged or rejects every origin (including
/// `EnsureXcmOrigin<_, ()>` and `NeverEnsureOrigin`).
fn xcm_origin_is_privileged(ty: &Type, aliases: &Aliases, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    if origin_is_privileged(ty, aliases, depth) {
        return true;
    }
    if let Type::Paren(inner) = ty {
        return xcm_origin_is_privileged(&inner.elem, aliases, depth + 1);
    }
    let Some(path) = type_path(ty) else {
        return false;
    };
    let Some(segment) = path.segments.last() else {
        return false;
    };
    let ident = segment.ident.to_string();
    if ident == "NeverEnsureOrigin" {
        return true;
    }
    if ident == "EnsureXcmOrigin" {
        return generic_type_args(segment)
            .get(1)
            .is_some_and(|converter| is_unit_or_alias_unit(converter, aliases, depth + 1));
    }
    if path.segments.len() == 1 && segment.arguments.is_empty() {
        return unique_alias(aliases, &ident)
            .is_some_and(|alias| xcm_origin_is_privileged(alias, aliases, depth + 1));
    }
    false
}

/// True when `impl pallet_xcm::Config for Runtime` closes every permissionless
/// local outbound XCM path (`send`, `execute`, `teleport_assets`,
/// `reserve_transfer_assets`), across all `#[cfg]` alternatives in the impl.
fn pallet_xcm_outbound_is_privileged(item_impl: &ItemImpl, aliases: &Aliases) -> bool {
    let all_items = |name: &str, pred: &dyn Fn(&Type) -> bool| {
        let items = impl_types_named(item_impl, name);
        !items.is_empty() && items.iter().all(|item| pred(&item.ty))
    };
    let send_privileged = all_items("SendXcmOrigin", &|ty| {
        xcm_origin_is_privileged(ty, aliases, 0)
    });
    let execute_privileged = all_items("XcmExecuteFilter", &|ty| is_nothing_filter(ty, aliases, 0))
        || all_items("ExecuteXcmOrigin", &|ty| {
            xcm_origin_is_privileged(ty, aliases, 0)
        });
    let teleport_blocked = all_items("XcmTeleportFilter", &|ty| is_nothing_filter(ty, aliases, 0));
    let reserve_blocked = all_items("XcmReserveTransferFilter", &|ty| {
        is_nothing_filter(ty, aliases, 0)
    });
    send_privileged && execute_privileged && teleport_blocked && reserve_blocked
}

fn xcm_fee_diagnostics(
    rule: &dyn LintRule,
    ctx: &FileContext,
    config: &Config,
    is_waived_rule: bool,
    default: Severity,
    explanation: &str,
    suggestion: &str,
) -> Option<Vec<Diagnostic>> {
    let krate = runtime_scope(ctx, &config.runtime_config)?;
    let file = production_ast(ctx.ast.as_ref()?);
    let severity = config.rule_severity(rule.id(), default);
    non_empty(
        xcm_fee_findings(&file)
            .into_iter()
            .filter(|(found, _, _)| match found {
                XcmFeeFinding::FeeManagerWaived | XcmFeeFinding::RouterDeliveryFree => {
                    is_waived_rule
                }
                XcmFeeFinding::Burned => !is_waived_rule,
            })
            .map(|(found, line, mut message)| {
                let low_confidence = found == XcmFeeFinding::FeeManagerWaived
                    && krate.local_outbound_xcm_privileged_only;
                if low_confidence {
                    message.push_str(
                        " (local `pallet_xcm` outbound paths appear privileged-only; verify \
                         whether inbound XCM can trigger onward sends)",
                    );
                }
                let mut diag =
                    diagnostic(rule, ctx, severity, line, message, explanation, suggestion);
                if low_confidence {
                    diag.confidence = Confidence::Low;
                }
                diag
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
            true,
            Severity::Warning,
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
            false,
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

/// `type KeyDeposit = ();` is zero (`()` is `Get<T>` returning `T::default()`).
/// Bare `Deposit` (pallet-revive's backend) is not an amount; `InvulnerableDeposit`
/// is only charged from governance-set invulnerables.
fn is_amount_deposit_name(name: &str) -> bool {
    (name.ends_with("Deposit") && name != "Deposit" && name != "InvulnerableDeposit")
        || ["DepositBase", "DepositPer", "DepositFactor"]
            .iter()
            .any(|part| name.contains(part))
}

/// `KeyDeposit` only guards `pallet_session::set_keys`; a production match arm
/// `Session(.. set_keys ..) => false` (a call filter) makes it unreachable.
fn set_keys_filtered(file: &SynFile) -> bool {
    struct Finder(bool);
    impl<'ast> Visit<'ast> for Finder {
        fn visit_arm(&mut self, arm: &'ast syn::Arm) {
            self.0 |= find_ident(arm.pat.to_token_stream(), &["set_keys"]).is_some()
                && matches!(strip_expr_wrappers(&arm.body), Expr::Lit(lit) if compact_tokens(lit) == "false");
            visit::visit_arm(self, arm);
        }
    }
    let mut finder = Finder(false);
    finder.visit_file(file);
    finder.0
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
/// recursively and same-file `type` aliases are resolved; any unknown,
/// ambiguous or cross-file origin type is treated as permissionless.
fn origin_is_privileged(ty: &Type, aliases: &Aliases, depth: usize) -> bool {
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
        unique_alias(aliases, &ident)
            .is_some_and(|alias| origin_is_privileged(alias, aliases, depth + 1))
    } else {
        false
    }
}

fn deposit_is_origin_gated(item_impl: &ItemImpl, deposit_type: &str, aliases: &Aliases) -> bool {
    let Some(origin) = gating_origin(deposit_type) else {
        return false;
    };
    impl_type_named(item_impl, origin)
        .is_some_and(|item| origin_is_privileged(&item.ty, aliases, 0))
}

/// Declarations inside a `parameter_types!` body: name, line, value. Covers the
/// `pub const X: T = v;`, `pub X: T = v;` and `pub storage X: T = v;` forms
/// (frame/support/src/macros.rs), skipping test/benchmark-gated declarations.
fn parameter_type_consts(tokens: TokenStream) -> Vec<(String, usize, Vec<TokenTree>)> {
    let trees = tokens.into_iter().collect::<Vec<_>>();
    trees
        .split(|tree| matches!(tree, TokenTree::Punct(punct) if punct.as_char() == ';'))
        .filter_map(|declaration| {
            let mut index = 0;
            let mut masked = false;
            while let (Some(TokenTree::Punct(punct)), Some(TokenTree::Group(_))) =
                (declaration.get(index), declaration.get(index + 1))
            {
                if punct.as_char() != '#' {
                    break;
                }
                masked |= attr_tokens_non_production(&declaration[index..index + 2]);
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
                    rest[equals + 1..].to_vec(),
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
                let Some(name) = type_path(&item_type.ty).and_then(path_last_ident) else {
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
            aliases: &'a Aliases,
            set_keys_filtered: bool,
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
                if path_last_ident(&mac.path).as_deref() == Some("parameter_types") {
                    for (name, line, value) in parameter_type_consts(mac.tokens.clone()) {
                        if expr_tokens_are_zero(&value) {
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
                        if (const_type_is_zero(&item_type.ty)
                            || (is_unit_type(&item_type.ty)
                                && is_amount_deposit_name(&name)
                                && !(name == "KeyDeposit" && self.set_keys_filtered)))
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
            set_keys_filtered: set_keys_filtered(&file),
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

/// Collective-flip crate names (current and pre-rename) and the conventional
/// runtime pallet name.
const INSECURE_RANDOMNESS: &[&str] = &[
    "pallet_insecure_randomness_collective_flip",
    "pallet_randomness_collective_flip",
    "RandomnessCollectiveFlip",
];

/// The first ident of `tokens` among `needles`, with its line.
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
pub(crate) fn construct_runtime_entries(tokens: TokenStream) -> Vec<Vec<TokenTree>> {
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
            while let (Some(TokenTree::Punct(punct)), Some(TokenTree::Group(_))) =
                (entry.get(index), entry.get(index + 1))
            {
                if punct.as_char() != '#' {
                    break;
                }
                if attr_tokens_non_production(&entry[index..index + 2]) {
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
                // Only consumer bindings are reported, not the pallet's own `Config`
                // impl; `pallet_contracts` only feeds the deprecated `seal_random`.
                if item_impl.trait_.as_ref().is_some_and(|(_, path, _)| {
                    path.segments
                        .first()
                        .is_some_and(|s| s.ident == "pallet_contracts")
                }) {
                    return;
                }
                for item in &item_impl.items {
                    self.visit_impl_item(item);
                }
            }

            fn visit_impl_item_type(&mut self, item: &'ast ImplItemType) {
                self.record(item.ty.to_token_stream(), INSECURE_RANDOMNESS);
            }

            fn visit_item_type(&mut self, item: &'ast ItemType) {
                // `#[runtime::pallet_index(..)] pub type X = ..;` declares the pallet.
                if !item
                    .attrs
                    .iter()
                    .any(|a| a.path().segments[0].ident == "runtime")
                {
                    self.record(item.ty.to_token_stream(), INSECURE_RANDOMNESS);
                }
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

// ---------------------------------------------------------------------------
// RTC007: XCM deny barrier not applied to nested programs
// ---------------------------------------------------------------------------

/// Library denies that inspect the message's instructions
/// (polkadot/xcm/xcm-builder/src/barriers.rs,
/// cumulus/parachains/runtimes/bridge-hubs/common/src/barriers.rs).
const INSTRUCTION_DENIES: &[&str] = &["DenyReserveTransferToRelayChain", "DenyExportMessageFrom"];

pub struct XcmDenyBarrierNotRecursive;

impl LintRule for XcmDenyBarrierNotRecursive {
    fn id(&self) -> &str {
        "RTC007"
    }
    fn name(&self) -> &str {
        "xcm-deny-barrier-not-recursive"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        runtime_scope(ctx, &config.runtime_config)?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Warning);

        struct Visitor<'a> {
            aliases: &'a Aliases,
            hits: Vec<(usize, String)>,
        }
        impl Visitor<'_> {
            /// Report every instruction-inspecting deny in `ty` (a tuple of
            /// denies, or an alias of one) not wrapped in `DenyRecursively`.
            fn check_deny(&mut self, ty: &Type, depth: usize) {
                if depth > 8 {
                    return;
                }
                if let Type::Tuple(tuple) = ty {
                    for elem in &tuple.elems {
                        self.check_deny(elem, depth + 1);
                    }
                    return;
                }
                let Some(path) = type_path(ty) else { return };
                let Some(ident) = path_last_ident(path) else {
                    return;
                };
                if INSTRUCTION_DENIES.contains(&ident.as_str()) {
                    self.hits.push((line_of(ty), type_text(ty)));
                } else if path.segments.len() == 1 {
                    if let Some(alias) = unique_alias(self.aliases, &ident) {
                        self.check_deny(alias, depth + 1);
                    }
                }
            }
        }
        impl<'ast> Visit<'ast> for Visitor<'_> {
            fn visit_item_use(&mut self, _: &'ast syn::ItemUse) {}

            fn visit_type_path(&mut self, type_path: &'ast TypePath) {
                if let Some(segment) = type_path.path.segments.last() {
                    if segment.ident == "DenyThenTry" {
                        if let Some(deny) = generic_type_args(segment).first() {
                            self.check_deny(deny, 0);
                        }
                    }
                }
                visit::visit_type_path(self, type_path);
            }
        }

        let aliases = type_aliases(&file);
        let mut visitor = Visitor {
            aliases: &aliases,
            hits: Vec::new(),
        };
        visitor.visit_file(&file);
        visitor.hits.sort();
        visitor.hits.dedup();
        non_empty(
            visitor
                .hits
                .into_iter()
                .map(|(line, deny)| {
                    diagnostic(
                        self,
                        ctx,
                        severity,
                        line,
                        format!(
                            "`DenyThenTry` deny `{deny}` is not wrapped in `DenyRecursively`, so \
                             instructions nested in `SetAppendix`, `SetErrorHandler` or \
                             `ExecuteWithOrigin` bypass it"
                        ),
                        "`DenyThenTry` checks only top-level instructions; nested programs run \
                         without the barrier.",
                        "Wrap the deny in `DenyRecursively<..>`.",
                    )
                })
                .collect(),
        )
    }
}

// ---------------------------------------------------------------------------
// RTC008: proxy filter fails open
// ---------------------------------------------------------------------------

/// Pallet crates whose calls move the caller's funds or assets.
const VALUE_MOVING_PALLETS: &[&str] = &[
    "pallet_balances",
    "pallet_assets",
    "pallet_xcm",
    "pallet_revive",
    "pallet_contracts",
    "pallet_asset_conversion",
    "pallet_psm",
    "pallet_uniques",
    "pallet_nfts",
    "pallet_nft_fractionalization",
    "pallet_vesting",
    "pallet_indices",
    "orml_tokens",
    "orml_xtokens",
    "orml_currencies",
];

/// A `RuntimeCall` pattern: pallet variant and call (`None`: every call).
type CallPat = (String, Option<String>);

/// What a proxy type's `filter` arm admits.
enum Calls {
    Allow(Vec<CallPat>),
    Deny(Vec<CallPat>),
    Unknown,
}

/// Appends the calls `pat` matches to `out`; `false` if `pat` has a shape
/// (`_`, a binding, a nested pattern) that is not modelled precisely.
fn call_pats(pat: &syn::Pat, out: &mut Vec<CallPat>) -> bool {
    let call = |inner: Option<&syn::Pat>| match inner {
        None | Some(syn::Pat::Rest(_) | syn::Pat::Wild(_)) => Some(None),
        Some(syn::Pat::Ident(i)) if i.subpat.is_none() => Some(None),
        Some(syn::Pat::Struct(s)) => Some(path_last_ident(&s.path)),
        Some(syn::Pat::TupleStruct(s)) => Some(path_last_ident(&s.path)),
        Some(syn::Pat::Path(p)) => Some(path_last_ident(&p.path)),
        Some(_) => None,
    };
    let (path, inner) = match pat {
        syn::Pat::Or(or) => return or.cases.iter().all(|case| call_pats(case, out)),
        syn::Pat::TupleStruct(s) if s.elems.len() == 1 => (&s.path, Some(&s.elems[0])),
        syn::Pat::TupleStruct(s) => (&s.path, None),
        syn::Pat::Struct(s) => (&s.path, None),
        syn::Pat::Path(p) => (&p.path, None),
        _ => return false,
    };
    let Some(pallet) = path_last_ident(path) else {
        return false;
    };
    let cases: Vec<Option<&syn::Pat>> = match inner {
        Some(syn::Pat::Or(or)) => or.cases.iter().map(Some).collect(),
        inner => vec![inner],
    };
    cases
        .into_iter()
        .all(|case| call(case).map(|c| out.push((pallet.clone(), c))).is_some())
}

fn filter_calls(body: &Expr) -> Calls {
    let matches_pats = |expr: &Expr| {
        let Expr::Macro(mac) = strip_expr_wrappers(expr) else {
            return None;
        };
        (macro_name(&mac.mac).as_deref() == Some("matches")).then_some(())?;
        let pat = mac
            .mac
            .parse_body_with(|input: syn::parse::ParseStream| {
                input.parse::<Expr>()?;
                input.parse::<syn::Token![,]>()?;
                syn::Pat::parse_multi_with_leading_vert(input)
            })
            .ok()?;
        let mut out = Vec::new();
        call_pats(&pat, &mut out).then_some(out)
    };
    match strip_expr_wrappers(body) {
        Expr::Block(block) => match block.block.stmts.as_slice() {
            [syn::Stmt::Expr(expr, None)] => filter_calls(expr),
            _ => Calls::Unknown,
        },
        Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Not(_)) => {
            matches_pats(&unary.expr).map_or(Calls::Unknown, Calls::Deny)
        }
        expr => matches_pats(expr).map_or(Calls::Unknown, Calls::Allow),
    }
}

/// `a` matches every call `b` matches.
fn covers(a: &CallPat, b: &CallPat) -> bool {
    a.0 == b.0 && (a.1.is_none() || a.1 == b.1)
}

/// Known operational-only pallet crates whose admission via `is_superset` is
/// treated as a non-value policy mismatch rather than a high-impact escalation.
/// Any unknown, custom, or renamed pallet fails closed as high impact.
/// `pallet_collator_selection` is absent because its calls reserve candidacy
/// bonds (fellows runtimes #1341). `pallet_session` is absent because
/// `set_keys` holds a `KeyDeposit` and replaces collator or validator keys.
const LOW_IMPACT_PROXY_CRATES: &[&str] = &[
    "pallet_timestamp",
    "pallet_aura",
    "pallet_authorship",
    "cumulus_pallet_parachain_system",
    "pallet_message_queue",
    "frame_system",
];

struct SubsetWitness {
    display: String,
    high_impact: bool,
}

/// Every declaration of `pallet` uses a low-impact crate. A variant with no
/// declaration, or a declaration without a visible crate, is high impact.
fn is_low_impact_pallet(
    pallet: &str,
    pallet_crates: &HashMap<String, Vec<Option<String>>>,
) -> bool {
    pallet_crates.get(pallet).is_some_and(|crates| {
        !crates.is_empty()
            && crates.iter().all(|krate| {
                krate
                    .as_deref()
                    .is_some_and(|krate| LOW_IMPACT_PROXY_CRATES.contains(&krate))
            })
    })
}

/// Calls `sub` admits that `sup` does not, if evident: "`Pallet` calls", "`Pallet::call`".
/// Inspects every uncovered pattern and prefers a high-impact witness whenever
/// one exists, so low-impact patterns never mask a high-impact escalation.
fn subset_witness(
    sub: &Calls,
    sup: &Calls,
    pallet_crates: &HashMap<String, Vec<Option<String>>>,
) -> Option<SubsetWitness> {
    let show = |(pallet, call): &CallPat| SubsetWitness {
        display: call
            .as_ref()
            .map_or(format!("`{pallet}` calls"), |c| format!("`{pallet}::{c}`")),
        high_impact: !is_low_impact_pallet(pallet, pallet_crates),
    };
    let pick = |uncovered: Vec<&CallPat>| -> Option<SubsetWitness> {
        let mut first_low: Option<SubsetWitness> = None;
        for pat in uncovered {
            let witness = show(pat);
            if witness.high_impact {
                return Some(witness);
            }
            if first_low.is_none() {
                first_low = Some(witness);
            }
        }
        first_low
    };
    match (sub, sup) {
        (Calls::Deny(_), Calls::Allow(_)) => Some(SubsetWitness {
            display: "calls".into(),
            high_impact: true,
        }),
        (Calls::Unknown | Calls::Deny(_), _) | (_, Calls::Unknown) => None,
        (Calls::Allow(sub), Calls::Allow(sup)) => pick(
            sub.iter()
                .filter(|s| !sup.iter().any(|a| covers(a, s)))
                .collect(),
        ),
        (Calls::Allow(sub), Calls::Deny(sup)) => pick(
            sub.iter()
                .filter(|s| sup.iter().any(|d| covers(d, s) || covers(s, d)))
                .collect(),
        ),
    }
}

/// Last idents of the (or-)path patterns in `pat`, e.g. `ProxyType::A | ProxyType::B`;
/// `["_"]` for any other pattern (`_`, a binding), which matches every variant.
fn variant_names(pat: &syn::Pat) -> Vec<String> {
    let mut out = Vec::new();
    if !call_pats(pat, &mut out) {
        return vec!["_".into()];
    }
    out.into_iter().map(|(name, _)| name).collect()
}

/// `calls` evidently admits `Proxy::add_proxy`.
fn admits_add_proxy(calls: &Calls) -> bool {
    let add_proxy = ("Proxy".to_string(), Some("add_proxy".to_string()));
    match calls {
        Calls::Allow(allowed) => allowed.iter().any(|a| covers(a, &add_proxy)),
        Calls::Deny(denied) => !denied.iter().any(|d| d.0 == add_proxy.0),
        Calls::Unknown => false,
    }
}

pub struct ProxyFilterFailsOpen;

impl LintRule for ProxyFilterFailsOpen {
    fn id(&self) -> &str {
        "RTC008"
    }
    fn name(&self) -> &str {
        "proxy-filter-fails-open"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        let krate = runtime_scope(ctx, &config.runtime_config)?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Warning);
        let mut diagnostics = Vec::new();

        // Proxy filters are top-level impls in every runtime surveyed.
        for item in &file.items {
            let Item::Impl(item_impl) = item else {
                continue;
            };
            let trait_name = item_impl
                .trait_
                .as_ref()
                .and_then(|(_, p, _)| path_last_ident(p));
            if trait_name.as_deref() != Some("InstanceFilter") {
                continue;
            }
            // Arms of the `match` statement in method `name`.
            let fn_arms = |name: &str| {
                let stmts = item_impl.items.iter().find_map(|item| match item {
                    ImplItem::Fn(f) if f.sig.ident == name => Some(&f.block.stmts),
                    _ => None,
                });
                stmts.into_iter().flatten().find_map(|stmt| match stmt {
                    syn::Stmt::Expr(Expr::Match(m), _) => Some(m.arms.as_slice()),
                    _ => None,
                })
            };
            // (a) deny-list arms that already deny a value-moving pallet (so they are
            // meant to stop transfers) but leave another one reachable.
            let mut proxies: Vec<(String, Calls)> = Vec::new();
            for arm in fn_arms("filter").unwrap_or_default() {
                let calls = filter_calls(&arm.body);
                let denies = |name: &String| match &calls {
                    Calls::Deny(denied) => denied.iter().any(|(pallet, _)| pallet == name),
                    _ => false,
                };
                if krate.pallets.iter().any(|(name, _)| denies(name)) {
                    let missing: Vec<String> = krate
                        .pallets
                        .iter()
                        .filter(|(name, _)| !denies(name))
                        .map(|(name, krate)| format!("{name} ({krate})"))
                        .collect();
                    if !missing.is_empty() {
                        diagnostics.push(diagnostic(
                            self,
                            ctx,
                            severity,
                            line_of(&arm.pat),
                            format!(
                                "proxy filter `{}` is a deny-list that does not deny the \
                                 value-moving pallets {}",
                                compact_tokens(&arm.pat),
                                missing.join(", ")
                            ),
                            "A `!matches!` filter allows every pallet it does not name.",
                            "Deny the listed pallets, or make the filter an allow-list.",
                        ));
                    }
                }
                for name in variant_names(&arm.pat) {
                    if name != "_" {
                        proxies.push((name, filter_calls(&arm.body)));
                    }
                }
            }
            // (b) `(X, _) => true` where X may add a proxy type Y that admits a call X does
            // not. Pairs decided by an earlier unguarded arm (`_` matching any side) are skipped.
            let mut decided: HashSet<(String, String)> = HashSet::new();
            for arm in fn_arms("is_superset").unwrap_or_default() {
                let syn::Pat::Tuple(tuple) = &arm.pat else {
                    continue;
                };
                let (Some(sup), Some(sub), None) =
                    (tuple.elems.first(), tuple.elems.get(1), &arm.guard)
                else {
                    continue;
                };
                let sups = variant_names(sup);
                let returns_true = matches!(strip_expr_wrappers(&arm.body), Expr::Lit(lit) if compact_tokens(lit) == "true");
                let wildcard = match (sups.as_slice(), sub) {
                    ([x], syn::Pat::Wild(_)) if x != "_" && returns_true => Some(x),
                    _ => None,
                };
                let Some(x) = wildcard else {
                    for x in &sups {
                        decided.extend(variant_names(sub).into_iter().map(|y| (x.clone(), y)));
                    }
                    continue;
                };
                let is_decided = |y: &String| {
                    [x, "_"].iter().any(|a| {
                        [y.as_str(), "_"]
                            .iter()
                            .any(|b| decided.contains(&(a.to_string(), b.to_string())))
                    })
                };
                let Some((_, sup_calls)) = proxies.iter().find(|(name, _)| name == x) else {
                    continue;
                };
                if !admits_add_proxy(sup_calls) {
                    continue;
                }
                let escapes: Vec<(String, bool)> = proxies
                    .iter()
                    .filter(|(y, _)| y != x && !is_decided(y))
                    .filter_map(|(y, calls)| {
                        let witness = subset_witness(calls, sup_calls, &krate.pallet_crates)?;
                        if config
                            .runtime_config
                            .rtc008_reports_value_escalations_only()
                            && !witness.high_impact
                        {
                            return None;
                        }
                        Some((
                            format!(
                                "`{x}` can add `{y}` proxies, which admit {} `{x}` does not",
                                witness.display
                            ),
                            witness.high_impact,
                        ))
                    })
                    .collect();
                if !escapes.is_empty() {
                    let any_high_impact = escapes.iter().any(|(_, high)| *high);
                    let descriptions: Vec<&str> =
                        escapes.iter().map(|(text, _)| text.as_str()).collect();
                    let mut diag = diagnostic(
                        self,
                        ctx,
                        severity,
                        line_of(&arm.pat),
                        format!(
                            "`is_superset` arm `{} => true` lets `{x}` escalate: {}",
                            compact_tokens(&arm.pat),
                            descriptions.join("; ")
                        ),
                        "A proxy can add or remove any proxy type it is a superset of.",
                        "List the subsumed proxy types instead of `_`.",
                    );
                    diag.confidence = if any_high_impact {
                        Confidence::High
                    } else {
                        Confidence::Medium
                    };
                    diagnostics.push(diag);
                }
            }
        }
        non_empty(diagnostics)
    }
}

// ---------------------------------------------------------------------------
// RTC009: known-vulnerable Polkadot crate version in Cargo.lock
// ---------------------------------------------------------------------------

/// `(crate, first vulnerable, first fixed on that major line, advisory, url, precondition)`.
type VulnerableRelease = (
    &'static str,
    Version,
    Option<Version>,
    &'static str,
    &'static str,
    Option<&'static str>,
);
type Version = (u64, u64, u64);

fn parse_version(version: &str) -> Option<Version> {
    let mut parts = version.split('.').map(|part| part.parse().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

fn vulnerable_releases() -> Vec<VulnerableRelease> {
    include_str!("vulnerable_crates.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let row = line.split('\t').collect::<Vec<_>>();
            let version = |field: &str| parse_version(field).expect("valid version in TSV");
            let fixed = (row[2] != "-").then(|| version(row[2]));
            let precondition = row.get(5).copied().filter(|value| *value != "-");
            (row[0], version(row[1]), fixed, row[3], row[4], precondition)
        })
        .collect()
}

const CRATES_IO_SOURCES: [&str; 2] = [
    "registry+https://github.com/rust-lang/crates.io-index",
    "sparse+https://index.crates.io/",
];

/// Line of the `name = "<name>"` key of the `[[package]]` block that also has
/// `version = "<version>"`, in any key order; 1 if not found.
fn package_line(lock: &str, name: &str, version: &str) -> usize {
    let (name, version) = (
        format!("name = \"{name}\""),
        format!("version = \"{version}\""),
    );
    let (mut name_line, mut version_seen) = (None, false);
    for (index, line) in lock.lines().map(str::trim).enumerate() {
        if line == "[[package]]" {
            (name_line, version_seen) = (None, false);
        }
        if line == name {
            name_line = Some(index + 1);
        }
        version_seen |= line == version;
        if let (Some(line), true) = (name_line, version_seen) {
            return line;
        }
    }
    1
}

/// A crates.io `Cargo.lock` package with at least one advisory: index, name,
/// version, parsed version and the releases that affect it.
type AffectedPackage<'a> = (usize, &'a str, &'a str, Version, Vec<&'a VulnerableRelease>);

/// The crates.io packages of `packages` that an advisory affects. Path and
/// git packages are the workspace's own code, and a private registry may
/// serve a patched fork under the same name and version.
fn affected_packages<'a>(
    packages: &'a [toml::Value],
    advisories: &'a [VulnerableRelease],
) -> Vec<AffectedPackage<'a>> {
    packages
        .iter()
        .enumerate()
        .filter_map(|(idx, package)| {
            let field = |key| package.get(key).and_then(toml::Value::as_str);
            let (name, version, source) = (field("name")?, field("version")?, field("source")?);
            let installed = parse_version(version)?;
            if !CRATES_IO_SOURCES.contains(&source) {
                return None;
            }
            let releases = advisories
                .iter()
                .filter(|(krate, first_vulnerable, fixed, ..)| {
                    *krate == name
                        && installed.0 == first_vulnerable.0
                        && installed >= *first_vulnerable
                        && fixed.is_none_or(|fixed| installed < fixed)
                })
                .collect::<Vec<_>>();
            (!releases.is_empty()).then_some((idx, name, version, installed, releases))
        })
        .collect()
}

/// The `Cargo.lock` at `path` has a crates.io package that an advisory affects.
fn lock_has_affected_package(path: &Path) -> bool {
    let Some(lock) = fs::read_to_string(path)
        .ok()
        .and_then(|content| content.parse::<toml::Table>().ok())
    else {
        return false;
    };
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    !affected_packages(packages, &vulnerable_releases()).is_empty()
}

fn version_string((major, minor, patch): Version) -> String {
    format!("{major}.{minor}.{patch}")
}

#[derive(PartialEq, Eq, Hash)]
struct WorkspaceRuntimesKey(PathBuf);

/// Every runtime crate under a workspace directory. See [`workspace_crates`].
fn workspace_runtimes(lock_dir: &Path) -> Vec<Arc<RuntimeCrate>> {
    workspace_crates(lock_dir)
        .iter()
        .filter(|krate| krate.is_runtime)
        .cloned()
        .collect()
}

/// Discovers every Cargo package under a workspace directory (skipping hidden
/// directories, `target`, and `node_modules`), cached per scan.
fn workspace_crates(lock_dir: &Path) -> Arc<Vec<Arc<RuntimeCrate>>> {
    let key =
        WorkspaceRuntimesKey(fs::canonicalize(lock_dir).unwrap_or_else(|_| lock_dir.to_path_buf()));
    scan_cached(key, |WorkspaceRuntimesKey(root)| {
        let mut runtimes = Vec::new();
        let walker = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                if entry.file_type().is_dir() {
                    let name = entry.file_name().to_string_lossy();
                    return !name.starts_with('.') && name != "target" && name != "node_modules";
                }
                true
            });
        for entry in walker.filter_map(Result::ok) {
            if entry.file_type().is_file() && entry.file_name() == "Cargo.toml" {
                if let Some(pkg_dir) = entry.path().parent() {
                    let canonical =
                        fs::canonicalize(pkg_dir).unwrap_or_else(|_| pkg_dir.to_path_buf());
                    // The source rules never see a manifest-only crate, so the
                    // text check cannot hide a runtime from them.
                    let krate = if may_declare_runtime(&canonical) {
                        scan_cached(canonical, |dir| Arc::new(load_runtime_crate(dir)))
                    } else {
                        scan_cached(ManifestOnlyKey(canonical), |ManifestOnlyKey(dir)| {
                            Arc::new(load_crate(dir, CrateLoad::ManifestOnly))
                        })
                    };
                    if krate.package.is_some() {
                        runtimes.push(krate);
                    }
                }
            }
        }
        Arc::new(runtimes)
    })
}

#[derive(PartialEq, Eq, Hash)]
struct ManifestOnlyKey(PathBuf);

/// Some Rust file under the crate directory `root`, outside nested crates and
/// test paths (`mock.rs`, `tests/`), has text that a runtime declaration
/// needs: `construct_runtime`, or `runtime` then optional whitespace and `]`
/// (`#[frame_support::runtime]`, `#[runtime]`) or `(`
/// (`#[frame_support::runtime(..)]`). Reading text is much cheaper than
/// parsing the module tree, which RTC009 needs only for a possible runtime. A
/// pallet's mock runtime is in a test path, so the pallet is not parsed.
fn may_declare_runtime(root: &Path) -> bool {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 || !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !name.starts_with('.')
                && name != "target"
                && name != "node_modules"
                && !entry.path().join("Cargo.toml").is_file()
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
        .filter(|entry| !is_test_path(entry.path().strip_prefix(root).unwrap_or(entry.path())))
        .any(|entry| fs::read_to_string(entry.path()).is_ok_and(|text| has_runtime_text(&text)))
}

/// See [`may_declare_runtime`].
fn has_runtime_text(text: &str) -> bool {
    text.contains("construct_runtime")
        || text.match_indices("runtime").any(|(start, needle)| {
            text[start + needle.len()..]
                .trim_start()
                .starts_with([']', '('])
        })
}

/// The runtime belongs to a workspace other than the one rooted at
/// `lock_root`, so another lockfile describes it.
fn is_other_workspace(krate: &RuntimeCrate, lock_root: &Path) -> bool {
    krate
        .workspace
        .as_ref()
        .is_some_and(|workspace| workspace != lock_root)
}

/// A runtime root in `Cargo.lock`: a local production runtime crate parsed
/// from source, or a package whose source was not inspected.
#[derive(Clone)]
enum ReachRoot {
    Local(Arc<RuntimeCrate>),
    External,
}

/// The package builds a runtime WASM blob: it depends on this crate.
///
/// Runtime libraries such as `sp-runtime` or `polkadot-runtime-common` do not
/// depend on it, so a `runtime` name token alone does not identify a runtime.
///
/// `Cargo.lock` resolves every feature of a workspace member, so it keeps a
/// member's optional `substrate-wasm-builder` build dependency. For a package
/// outside the workspace (git, registry or an outside path) it keeps an
/// optional dependency only when some feature enables it, so a remote
/// runtime can lack the edge (checked with `cargo generate-lockfile`, Cargo
/// 1.93).
const WASM_BUILDER_CRATE: &str = "substrate-wasm-builder";

/// Every FRAME runtime depends on this crate directly.
const FRAME_EXECUTIVE_CRATE: &str = "frame-executive";

/// Name prefixes of library families with a `runtime` name token that are not
/// runtimes (`sp-runtime`, `sp-runtime-interface`, `bp-runtime`,
/// `frame-try-runtime`).
const RUNTIME_NAMED_LIBRARY_PREFIXES: &[&str] = &["sp-", "bp-", "frame-try-"];

fn is_runtime_named_library(name: &str) -> bool {
    RUNTIME_NAMED_LIBRARY_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// The package name ends in a `runtime` token, as most runtime crates do
/// (`asset-hub-polkadot-runtime`), and is not a known library.
fn ends_in_runtime_token(name: &str) -> bool {
    name_tokens(name).last().as_deref() == Some("runtime") && !is_runtime_named_library(name)
}

/// The package name has a `runtime` token anywhere (`runtime-eden`,
/// `interlay-runtime-parachain`) and is not a known library.
fn has_runtime_token(name: &str) -> bool {
    name_tokens(name).any(|token| token == "runtime") && !is_runtime_named_library(name)
}

/// True when a `Cargo.lock` package that no parsed runtime matches passes the
/// test, dev and testnet runtime name filters.
fn is_included_external_runtime(name: &str, config: &RuntimeConfigConfig) -> bool {
    if has_name_token(name, &config.test_runtime_tokens) {
        return false;
    }
    if !config.include_dev_runtimes
        && (has_name_token(name, &config.dev_runtime_tokens)
            || config.dev_runtime_packages.iter().any(|p| p == name))
    {
        return false;
    }
    if config.excludes_testnet_runtimes() && has_name_token(name, &config.testnet_runtime_tokens) {
        return false;
    }
    true
}

/// Parse a `Cargo.lock` `dependencies` entry (`name`, `name version`, or
/// `name version (source)`).
///
/// Cargo writes a git source without the `#<commit>` suffix that the
/// package's own `source` has, so compare sources with [`source_base`].
fn parse_lock_dep_spec(spec: &str) -> Option<(&str, Option<&str>, Option<&str>)> {
    let mut parts = spec.split_ascii_whitespace();
    let dep_name = parts.next()?;
    let mut dep_version = None;
    let mut dep_source = None;
    for part in parts {
        if let Some(src) = part.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            dep_source = Some(src);
        } else if dep_version.is_none() {
            dep_version = Some(part);
        }
    }
    Some((dep_name, dep_version, dep_source))
}

/// A `Cargo.lock` source without its `#<commit>` suffix.
fn source_base(source: &str) -> &str {
    source.split_once('#').map_or(source, |(base, _)| base)
}

/// Reachability of `Cargo.lock` packages from included production runtime crates.
struct LockReachability {
    /// For each package index in `Cargo.lock`, the production runtime roots
    /// that transitively depend on it.
    reaching_roots: Vec<Vec<ReachRoot>>,
}

impl LockReachability {
    /// Returns `Some(reachability)` when `lock_dir` is a Cargo workspace with
    /// at least one included production runtime rooted in `packages` and
    /// dependency edges. Returns `None` for a standalone lockfile, a
    /// non-runtime workspace, a workspace whose runtimes are all excluded, or
    /// a lockfile that an included local runtime does not match, so every
    /// finding keeps its normal severity.
    fn build(
        lock_dir: Option<&Path>,
        packages: &[toml::Value],
        config: &RuntimeConfigConfig,
    ) -> Option<Self> {
        let lock_dir = match lock_dir {
            Some(dir) if !dir.as_os_str().is_empty() => dir,
            _ => Path::new("."),
        };
        if !lock_dir.join("Cargo.toml").is_file() {
            return None;
        }
        let has_dep_edges = packages.iter().any(|pkg| {
            pkg.get("dependencies")
                .and_then(toml::Value::as_array)
                .is_some_and(|deps| !deps.is_empty())
        });
        if !has_dep_edges {
            return None;
        }

        let all_crates = workspace_crates(lock_dir);
        let all_runtimes = all_crates
            .iter()
            .filter(|krate| krate.is_runtime)
            .collect::<Vec<_>>();
        if all_runtimes.is_empty() {
            return None;
        }

        struct LockPkg<'a> {
            idx: usize,
            version: &'a str,
            source: Option<&'a str>,
        }

        // Index Cargo.lock packages by name.
        let mut by_name: HashMap<&str, Vec<LockPkg<'_>>> = HashMap::new();
        let mut names = vec![None; packages.len()];
        for (idx, pkg) in packages.iter().enumerate() {
            if let (Some(name), Some(version)) = (
                pkg.get("name").and_then(toml::Value::as_str),
                pkg.get("version").and_then(toml::Value::as_str),
            ) {
                let source = pkg.get("source").and_then(toml::Value::as_str);
                names[idx] = Some(name);
                by_name.entry(name).or_default().push(LockPkg {
                    idx,
                    version,
                    source,
                });
            }
        }

        // Build adjacency list for Cargo.lock `dependencies` entries, honoring
        // version and `(source)` disambiguation when present. A source that
        // matches no package keeps the edge to every package of that name and
        // version (fail closed).
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); packages.len()];
        for (idx, pkg) in packages.iter().enumerate() {
            let Some(deps) = pkg.get("dependencies").and_then(toml::Value::as_array) else {
                continue;
            };
            for dep in deps.iter().filter_map(toml::Value::as_str) {
                let Some((dep_name, dep_version, dep_source)) = parse_lock_dep_spec(dep) else {
                    continue;
                };
                let Some(candidates) = by_name.get(dep_name) else {
                    continue;
                };
                let versioned = candidates
                    .iter()
                    .filter(|target| dep_version.is_none_or(|ver| ver == target.version))
                    .collect::<Vec<_>>();
                let sourced = versioned
                    .iter()
                    .filter(|target| {
                        dep_source.is_none_or(|src| {
                            target.source.map(source_base) == Some(source_base(src))
                        })
                    })
                    .map(|target| target.idx)
                    .collect::<Vec<_>>();
                if sourced.is_empty() {
                    adj[idx].extend(versioned.iter().map(|target| target.idx));
                } else {
                    adj[idx].extend(sourced);
                }
            }
        }

        // A package is identified by (name, version, source). A parsed local
        // runtime matches only a `source`-less entry of its name and version,
        // so its source never describes a git or registry package of the same
        // name. Runtimes of another workspace have their own lockfile.
        let lock_root = fs::canonicalize(lock_dir).unwrap_or_else(|_| lock_dir.to_path_buf());
        let mut matched: Vec<Vec<&Arc<RuntimeCrate>>> = vec![Vec::new(); packages.len()];
        for &rt in &all_runtimes {
            if is_other_workspace(rt, &lock_root) {
                continue;
            }
            let local = rt
                .package
                .as_deref()
                .and_then(|name| by_name.get(name))
                .into_iter()
                .flatten()
                .filter(|pkg| pkg.source.is_none())
                .collect::<Vec<_>>();
            let hits = match rt.version.as_deref() {
                Some(version) => local
                    .iter()
                    .filter(|pkg| pkg.version == version)
                    .map(|pkg| pkg.idx)
                    .collect::<Vec<_>>(),
                None if local.len() == 1 => vec![local[0].idx],
                None => Vec::new(),
            };
            // A stale lockfile, or a version that cannot be resolved: the
            // lockfile does not show what this runtime reaches.
            if hits.is_empty() && is_included_runtime_crate(rt, config) {
                return None;
            }
            for idx in hits {
                matched[idx].push(rt);
            }
        }

        // A matched entry is a local root unless the filters exclude every
        // runtime that matches it. Only an entry matched by exactly one parsed
        // crate can use that crate's `AssetTransactor` mitigation.
        let mut local_roots = Vec::new();
        let mut external_roots = Vec::new();
        for (idx, runtimes) in matched.iter().enumerate() {
            match runtimes.as_slice() {
                [] => {}
                [rt] if is_included_runtime_crate(rt, config) => {
                    local_roots.push((idx, Arc::clone(rt)));
                }
                runtimes
                    if runtimes
                        .iter()
                        .any(|rt| is_included_runtime_crate(rt, config)) =>
                {
                    external_roots.push(idx);
                }
                _ => {}
            }
        }

        // An unmatched package that depends on `substrate-wasm-builder`,
        // directly or through other unmatched packages (the `polkadot-sdk`
        // umbrella, a helper build crate), may build a runtime the parser did
        // not see, so it is an external root. The search passes through a
        // package that the name filters exclude, because a package that
        // depends on it can still build a runtime.
        let mut dependents = vec![Vec::new(); packages.len()];
        for (idx, deps) in adj.iter().enumerate() {
            for &dep in deps {
                dependents[dep].push(idx);
            }
        }
        let mut builds_wasm = vec![false; packages.len()];
        let mut queue = (0..packages.len())
            .filter(|&idx| names[idx] == Some(WASM_BUILDER_CRATE))
            .collect::<VecDeque<_>>();
        while let Some(node) = queue.pop_front() {
            for &dependent in &dependents[node] {
                if builds_wasm[dependent] || !matched[dependent].is_empty() {
                    continue;
                }
                builds_wasm[dependent] = true;
                queue.push_back(dependent);
            }
        }

        // Cargo omits an optional dependency of a package outside the
        // workspace unless a feature enables it, so a git or registry runtime
        // may show no `substrate-wasm-builder` edge. Every FRAME runtime
        // depends on `frame-executive`, and most have a `runtime` name token,
        // so an unmatched package with either is an external root too. A
        // workspace member keeps its builder edge, so the `frame-executive`
        // edge and an inner `runtime` token skip a source-less entry of a
        // local package that the parser found is not a runtime (a pallet with
        // `frame-executive` as a dev-dependency). A source-less entry that no
        // parsed crate matches (a member in a hidden directory, a path
        // package outside the workspace) is code the parser did not read, so
        // it is a root.
        let executive = (0..packages.len())
            .filter(|&idx| names[idx] == Some(FRAME_EXECUTIVE_CRATE))
            .collect::<HashSet<_>>();
        let parsed = all_crates
            .iter()
            .filter(|krate| !is_other_workspace(krate, &lock_root))
            .filter_map(|krate| {
                let key = (krate.package.as_deref()?, krate.version.as_deref()?);
                Some((key, krate.sources_read && !krate.is_runtime))
            })
            .collect::<Vec<_>>();
        let local_libraries = parsed
            .iter()
            .filter(|(_, library)| *library)
            .map(|(key, _)| *key)
            .collect::<HashSet<_>>();
        let local_crates = parsed.iter().map(|(key, _)| *key).collect::<HashSet<_>>();
        for (idx, name) in names.iter().enumerate() {
            let Some(name) = name else { continue };
            let local = packages[idx]
                .get("source")
                .is_none()
                .then(|| packages[idx].get("version").and_then(toml::Value::as_str))
                .map(|version| version.map(|version| (*name, version)));
            let is_local_library = local
                .flatten()
                .is_some_and(|key| local_libraries.contains(&key));
            let is_unparsed_local =
                local.is_some_and(|key| key.is_none_or(|key| !local_crates.contains(&key)));
            let root_like = builds_wasm[idx]
                || is_unparsed_local
                || ends_in_runtime_token(name)
                || (!is_local_library
                    && (adj[idx].iter().any(|dep| executive.contains(dep))
                        || has_runtime_token(name)));
            if matched[idx].is_empty() && root_like && is_included_external_runtime(name, config) {
                external_roots.push(idx);
            }
        }

        // If no included production runtime is rooted in this lockfile, do not
        // apply reachability downgrades (fail closed).
        if local_roots.is_empty() && external_roots.is_empty() {
            return None;
        }

        let mut reaching_roots: Vec<Vec<ReachRoot>> = vec![Vec::new(); packages.len()];
        let mut walk = |roots: &[usize], root_kind: ReachRoot| {
            let mut visited = vec![false; packages.len()];
            let mut queue = VecDeque::new();
            for &root in roots {
                if !visited[root] {
                    visited[root] = true;
                    queue.push_back(root);
                }
            }
            while let Some(node) = queue.pop_front() {
                reaching_roots[node].push(root_kind.clone());
                for &next in &adj[node] {
                    if !visited[next] {
                        visited[next] = true;
                        queue.push_back(next);
                    }
                }
            }
        };
        for (idx, rt) in local_roots {
            walk(&[idx], ReachRoot::Local(rt));
        }
        walk(&external_roots, ReachRoot::External);

        Some(Self { reaching_roots })
    }
}

pub struct KnownVulnerableCrateVersion;

impl LintRule for KnownVulnerableCrateVersion {
    fn id(&self) -> &str {
        "RTC009"
    }
    fn name(&self) -> &str {
        "known-vulnerable-crate-version"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        if ctx.path.file_name()? != "Cargo.lock" {
            return None;
        }
        let lock = ctx.content.parse::<toml::Table>().ok()?;
        let packages = lock.get("package")?.as_array()?;
        let advisories = vulnerable_releases();
        let affected = affected_packages(packages, &advisories);
        if affected.is_empty() {
            return None;
        }
        // Reachability parses every crate of the workspace, so it is built
        // only for a lockfile with an affected package.
        let reachability =
            LockReachability::build(ctx.path.parent(), packages, &config.runtime_config);
        let mut diagnostics = Vec::new();
        for (pkg_idx, name, version, installed, releases) in affected {
            let reaching = reachability
                .as_ref()
                .map(|r| r.reaching_roots[pkg_idx].as_slice());
            let unreachable_from_prod_runtime = reaching.is_some_and(|roots| roots.is_empty());
            for (_, first_vulnerable, fixed, advisory, url, precondition) in releases {
                let line = package_line(ctx.content, name, version);
                let fix = match fixed {
                    Some(fixed) => format!("upgrade to {} or later", version_string(*fixed)),
                    None => {
                        // The nearest later line that got a fix for the same advisory.
                        let nearest = advisories
                            .iter()
                            .filter(|row| row.0 == name && row.3 == *advisory)
                            .filter_map(|row| row.2)
                            .filter(|fixed| fixed.0 > installed.0)
                            .min()
                            .map_or(String::new(), |fixed| {
                                format!("; the nearest fixed release is {}", version_string(fixed))
                            });
                        format!(
                            "the {}.x line has no fixed release{nearest}",
                            first_vulnerable.0
                        )
                    }
                };
                // For advisories with `precondition = asset_transactor`,
                // downgrade to advisory/low confidence when every reaching
                // production runtime is a local runtime setting
                // `AssetTransactor = ()` (e.g. Hyperbridge Nexus).
                let mitigated_by_unit_transactor = *precondition == Some("asset_transactor")
                    && reaching.is_some_and(|roots| {
                        !roots.is_empty()
                            && roots.iter().all(|root| match root {
                                ReachRoot::Local(rt) => rt.unit_asset_transactor_only,
                                ReachRoot::External => false,
                            })
                    });
                let downgraded = unreachable_from_prod_runtime || mitigated_by_unit_transactor;
                let default_sev = if downgraded {
                    Severity::Advisory
                } else {
                    Severity::Error
                };
                let severity = config.rule_severity(self.id(), default_sev);
                let mut message =
                    format!("{name} {version} is affected by \"{advisory}\" ({url}): {fix}");
                if unreachable_from_prod_runtime {
                    message.push_str(
                        " (not reachable from any included production runtime crate in this \
                         workspace)",
                    );
                } else if mitigated_by_unit_transactor {
                    message.push_str(
                        " (every reaching production runtime sets `AssetTransactor = ()`, \
                         so withdrawal debits are not reachable)",
                    );
                }
                let mut diag = diagnostic(
                    self,
                    ctx,
                    severity,
                    line,
                    message,
                    "This Polkadot crate release has a known security bug whose fix shipped as a \
                     patch release without a RustSec/GHSA advisory, so cargo-audit may not \
                     report it. A runtime built from this lockfile still contains the bug.",
                    "Bump the crate to the first fixed release on its line (for example with \
                     `cargo update -p <crate> --precise <version>` or psvm), rebuild the runtime \
                     and ship it in a runtime upgrade.",
                );
                if downgraded {
                    diag.confidence = Confidence::Low;
                }
                diagnostics.push(diag);
            }
        }
        non_empty(diagnostics)
    }
}

// ---------------------------------------------------------------------------
// RTC010: storage-reading aliaser on the unpaid-execution barrier
// ---------------------------------------------------------------------------

pub struct StorageReadingUnpaidAliaser;

impl LintRule for StorageReadingUnpaidAliaser {
    fn id(&self) -> &str {
        "RTC010"
    }
    fn name(&self) -> &str {
        "storage-reading-unpaid-aliaser"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        struct Barriers<'a> {
            aliases: &'a Aliases,
            lines: Vec<usize>,
        }
        impl<'ast> Visit<'ast> for Barriers<'_> {
            fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
                if segment.ident == "AllowExplicitUnpaidExecutionFrom" {
                    if let Some(aliasers) = generic_type_args(segment).get(1) {
                        if contains_filter(aliasers, "AuthorizedAliasers", self.aliases, true, 0) {
                            self.lines.push(line_of(segment));
                        }
                    }
                }
                visit::visit_path_segment(self, segment);
            }
        }

        runtime_scope(ctx, &config.runtime_config)?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Warning);
        let aliases = type_aliases(&file);
        let mut barriers = Barriers {
            aliases: &aliases,
            lines: Vec::new(),
        };
        barriers.visit_file(&file);
        non_empty(
            barriers
                .lines
                .into_iter()
                .map(|line| {
                    diagnostic(
                        self,
                        ctx,
                        severity,
                        line,
                        "`AllowExplicitUnpaidExecutionFrom` is given `pallet_xcm::AuthorizedAliasers` \
                         as its aliaser filter"
                            .to_string(),
                        "Barriers run before any fee is taken. `AllowExplicitUnpaidExecutionFrom<T, \
                         Aliasers>` calls `Aliasers::contains` for every `AliasOrigin` ahead of \
                         `UnpaidExecution` (up to 5 per message), and `AuthorizedAliasers` reads \
                         storage on keys the message chooses. Any incoming message can trigger \
                         those reads for free and then be rejected (polkadot-sdk #12831, \
                         polkadot-fellows/runtimes #1237).",
                        "Pass only computation-only aliasers to the barrier, e.g. \
                         `type CheapTrustedAliasers = (AliasChildLocation,);`, and keep \
                         `(CheapTrustedAliasers, AuthorizedAliasers<Runtime>)` for \
                         `xcm_executor::Config::Aliasers`.",
                    )
                })
                .collect(),
        )
    }
}

// ---------------------------------------------------------------------------
// RTC011: `RemovePallet` that names a pallet still in the runtime
// ---------------------------------------------------------------------------

pub struct RemovePalletOfLivePallet;

impl LintRule for RemovePalletOfLivePallet {
    fn id(&self) -> &str {
        "RTC011"
    }
    fn name(&self) -> &str {
        "remove-pallet-of-live-pallet"
    }
    fn family(&self) -> &str {
        "runtime-config"
    }

    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>> {
        struct Removals<'a> {
            krate: &'a RuntimeCrate,
            found: Vec<(usize, String)>,
        }
        impl<'ast> Visit<'ast> for Removals<'_> {
            fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
                if segment.ident == "RemovePallet" {
                    let name = generic_type_args(segment)
                        .first()
                        .and_then(|ty| type_path(ty))
                        .and_then(path_last_ident)
                        .and_then(|param| self.krate.str_params.get(&param));
                    if let Some(name) = name.filter(|name| self.krate.pallet_names.contains(*name))
                    {
                        self.found.push((line_of(segment), name.clone()));
                    }
                }
                visit::visit_path_segment(self, segment);
            }
        }

        let krate = runtime_scope(ctx, &config.runtime_config)?;
        let file = production_ast(ctx.ast.as_ref()?);
        let severity = config.rule_severity(self.id(), Severity::Error);
        let mut removals = Removals {
            krate: &krate,
            found: Vec::new(),
        };
        removals.visit_file(&file);
        non_empty(
            removals
                .found
                .into_iter()
                .map(|(line, name)| {
                    diagnostic(
                        self,
                        ctx,
                        severity,
                        line,
                        format!("`RemovePallet` deletes the storage of `{name}`, which the runtime still declares"),
                        "`RemovePallet` deletes every key under the pallet prefix on each runtime \
                         upgrade. If the pallet is still in the runtime, each later upgrade wipes \
                         its live state (polkadot-sdk #13137: Asset Hub Westend kept \
                         `RemovePallet<PsmName, _>` with `Psm` still in the runtime).",
                        "Remove the `RemovePallet` entry from the migrations after it has run once, \
                         or remove the pallet from the runtime.",
                    )
                })
                .collect(),
        )
    }
}
