use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use cargo_metadata::{Dependency, Metadata, MetadataCommand, Package, TargetKind};
use syn::{
    punctuated::Punctuated,
    visit_mut::{self, VisitMut},
    AttrStyle, Attribute, Expr, Lit, Meta, Token,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SourceTargetKind {
    Lib,
    Bin,
    ProcMacro,
    Test,
    Bench,
    Example,
    Other(String),
}

#[derive(Debug, Clone, Default)]
pub struct ProjectModel {
    file_target_kinds: HashMap<PathBuf, Vec<SourceTargetKind>>,
    proc_macro_source_roots: Vec<PathBuf>,
    rustc_cfgs: BTreeSet<String>,
    file_cfgs: HashMap<PathBuf, ActiveCfg>,
}

impl ProjectModel {
    pub fn discover(scan_root: &Path) -> Option<Self> {
        Self::discover_with_features(scan_root, false, &[])
    }

    pub fn discover_with_features(
        scan_root: &Path,
        no_default_features: bool,
        features: &[String],
    ) -> Option<Self> {
        let manifest_dir = find_manifest_dir(scan_root)?;

        let metadata = match workspace_metadata(&manifest_dir) {
            Ok(metadata) => metadata,
            Err(error) => {
                eprintln!(
                    "polkadot-linter: cargo metadata failed for {} ({error}); source analysis continues without Cargo target and feature data",
                    manifest_dir.display()
                );
                return None;
            }
        };

        let rustc_cfgs = rustc_cfgs();
        let resolved_features = workspace_features(
            &metadata.packages,
            &patched_members(&metadata),
            no_default_features,
            features,
        );

        let mut file_target_kinds = HashMap::<PathBuf, Vec<SourceTargetKind>>::new();
        let mut proc_macro_source_roots = Vec::<PathBuf>::new();
        let mut file_cfgs = HashMap::<PathBuf, ActiveCfg>::new();
        for (package, features) in metadata.packages.into_iter().zip(resolved_features) {
            let active_cfg = ActiveCfg::new(&rustc_cfgs, features);
            for target in package.targets {
                let Ok(src_path) = target.src_path.into_std_path_buf().canonicalize() else {
                    continue;
                };
                if target.kind.contains(&TargetKind::ProcMacro) {
                    if let Some(source_root) = src_path.parent() {
                        proc_macro_source_roots.push(source_root.to_path_buf());
                    }
                }
                let kinds = target
                    .kind
                    .into_iter()
                    .map(|kind| match kind {
                        TargetKind::Lib | TargetKind::RLib => SourceTargetKind::Lib,
                        TargetKind::ProcMacro => SourceTargetKind::ProcMacro,
                        TargetKind::Bin => SourceTargetKind::Bin,
                        TargetKind::Test => SourceTargetKind::Test,
                        TargetKind::Bench => SourceTargetKind::Bench,
                        TargetKind::Example => SourceTargetKind::Example,
                        TargetKind::Unknown(other) => SourceTargetKind::Other(other),
                        other => SourceTargetKind::Other(other.to_string()),
                    })
                    .collect::<Vec<_>>();
                file_target_kinds
                    .entry(src_path.clone())
                    .or_default()
                    .extend(kinds);
                file_cfgs
                    .entry(src_path)
                    .or_insert_with(|| active_cfg.clone());
            }
        }

        Some(Self {
            file_target_kinds,
            proc_macro_source_roots,
            rustc_cfgs,
            file_cfgs,
        })
    }

    pub fn target_kinds_for_file(&self, path: &Path) -> Vec<SourceTargetKind> {
        let Ok(path) = path.canonicalize() else {
            return Vec::new();
        };
        let mut kinds = self
            .file_target_kinds
            .get(&path)
            .cloned()
            .unwrap_or_default();
        if self
            .proc_macro_source_roots
            .iter()
            .any(|root| path.starts_with(root))
            && !kinds.contains(&SourceTargetKind::ProcMacro)
        {
            kinds.push(SourceTargetKind::ProcMacro);
        }
        kinds
    }

    pub fn rustc_cfgs(&self) -> &BTreeSet<String> {
        &self.rustc_cfgs
    }

    pub fn active_cfg_for_file(&self, path: &Path) -> Option<&ActiveCfg> {
        let path = path.canonicalize().ok()?;
        self.file_cfgs.get(&path)
    }
}

/// Cargo and rustc cfg values active for one source package.
#[derive(Debug, Clone, Default)]
pub struct ActiveCfg {
    flags: BTreeSet<String>,
    values: BTreeMap<String, BTreeSet<String>>,
}

impl ActiveCfg {
    fn new(rustc_cfgs: &BTreeSet<String>, features: impl IntoIterator<Item = String>) -> Self {
        let mut active = Self::default();
        for cfg in rustc_cfgs {
            if let Some((key, value)) = cfg.split_once('=') {
                active
                    .values
                    .entry(key.to_string())
                    .or_default()
                    .insert(value.trim_matches('"').to_string());
            } else {
                active.flags.insert(cfg.clone());
            }
        }
        active
            .values
            .entry("feature".to_string())
            .or_default()
            .extend(features);
        active
    }

    fn predicate_truth(&self, meta: &Meta) -> Option<bool> {
        match meta {
            Meta::Path(path) => {
                let ident = path.get_ident()?.to_string();
                self.flags.contains(&ident).then_some(true)
            }
            Meta::NameValue(name_value) => {
                let key = name_value.path.get_ident()?.to_string();
                let Expr::Lit(expr_lit) = &name_value.value else {
                    return None;
                };
                let Lit::Str(value) = &expr_lit.lit else {
                    return None;
                };
                self.values
                    .get(&key)
                    .map(|values| values.contains(&value.value()))
            }
            Meta::List(list) => {
                let operator = list.path.get_ident()?.to_string();
                let predicates = list
                    .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                    .ok()?;
                match operator.as_str() {
                    "all" => {
                        if predicates.is_empty() {
                            return Some(true);
                        }
                        let mut saw_unknown = false;
                        for predicate in &predicates {
                            match self.predicate_truth(predicate) {
                                Some(false) => return Some(false),
                                Some(true) => {}
                                None => saw_unknown = true,
                            }
                        }
                        (!saw_unknown).then_some(true)
                    }
                    "any" => {
                        if predicates.is_empty() {
                            return Some(false);
                        }
                        let mut saw_unknown = false;
                        for predicate in &predicates {
                            match self.predicate_truth(predicate) {
                                Some(true) => return Some(true),
                                Some(false) => {}
                                None => saw_unknown = true,
                            }
                        }
                        (!saw_unknown).then_some(false)
                    }
                    "not" if predicates.len() == 1 => {
                        self.predicate_truth(&predicates[0]).map(|value| !value)
                    }
                    _ => None,
                }
            }
        }
    }
}

/// Applies Cargo-selected `cfg_attr` attributes to the authored AST while
/// retaining attributes whose predicates cannot be resolved from metadata.
pub fn expand_active_cfg_attrs(ast: &mut syn::File, active_cfg: &ActiveCfg) {
    struct CfgAttrExpander<'a> {
        active_cfg: &'a ActiveCfg,
    }

    impl CfgAttrExpander<'_> {
        fn expand_attrs(&self, attrs: &mut Vec<Attribute>) {
            let mut expanded = Vec::with_capacity(attrs.len());
            for attr in std::mem::take(attrs) {
                self.expand_attr(attr, &mut expanded);
            }
            *attrs = expanded;
        }

        fn expand_attr(&self, attr: Attribute, expanded: &mut Vec<Attribute>) {
            if !attr.path().is_ident("cfg_attr") {
                expanded.push(attr);
                return;
            }
            let Ok(arguments) =
                attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            else {
                expanded.push(attr);
                return;
            };
            let mut arguments = arguments.into_iter();
            let Some(predicate) = arguments.next() else {
                expanded.push(attr);
                return;
            };
            match self.active_cfg.predicate_truth(&predicate) {
                Some(true) => {
                    for meta in arguments {
                        self.expand_attr(
                            Attribute {
                                pound_token: Default::default(),
                                style: AttrStyle::Outer,
                                bracket_token: Default::default(),
                                meta,
                            },
                            expanded,
                        );
                    }
                }
                Some(false) => {}
                None => expanded.push(attr),
            }
        }
    }

    impl VisitMut for CfgAttrExpander<'_> {
        fn visit_file_mut(&mut self, node: &mut syn::File) {
            self.expand_attrs(&mut node.attrs);
            visit_mut::visit_file_mut(self, node);
        }

        fn visit_item_mut(&mut self, node: &mut syn::Item) {
            let attrs = match node {
                syn::Item::Const(item) => &mut item.attrs,
                syn::Item::Enum(item) => &mut item.attrs,
                syn::Item::ExternCrate(item) => &mut item.attrs,
                syn::Item::Fn(item) => &mut item.attrs,
                syn::Item::ForeignMod(item) => &mut item.attrs,
                syn::Item::Impl(item) => &mut item.attrs,
                syn::Item::Macro(item) => &mut item.attrs,
                syn::Item::Mod(item) => &mut item.attrs,
                syn::Item::Static(item) => &mut item.attrs,
                syn::Item::Struct(item) => &mut item.attrs,
                syn::Item::Trait(item) => &mut item.attrs,
                syn::Item::TraitAlias(item) => &mut item.attrs,
                syn::Item::Type(item) => &mut item.attrs,
                syn::Item::Union(item) => &mut item.attrs,
                syn::Item::Use(item) => &mut item.attrs,
                _ => return visit_mut::visit_item_mut(self, node),
            };
            self.expand_attrs(attrs);
            visit_mut::visit_item_mut(self, node);
        }

        fn visit_impl_item_mut(&mut self, node: &mut syn::ImplItem) {
            let attrs = match node {
                syn::ImplItem::Const(item) => &mut item.attrs,
                syn::ImplItem::Fn(item) => &mut item.attrs,
                syn::ImplItem::Type(item) => &mut item.attrs,
                syn::ImplItem::Macro(item) => &mut item.attrs,
                _ => return visit_mut::visit_impl_item_mut(self, node),
            };
            self.expand_attrs(attrs);
            visit_mut::visit_impl_item_mut(self, node);
        }

        fn visit_trait_item_mut(&mut self, node: &mut syn::TraitItem) {
            let attrs = match node {
                syn::TraitItem::Const(item) => &mut item.attrs,
                syn::TraitItem::Fn(item) => &mut item.attrs,
                syn::TraitItem::Type(item) => &mut item.attrs,
                syn::TraitItem::Macro(item) => &mut item.attrs,
                _ => return visit_mut::visit_trait_item_mut(self, node),
            };
            self.expand_attrs(attrs);
            visit_mut::visit_trait_item_mut(self, node);
        }
    }

    CfgAttrExpander { active_cfg }.visit_file_mut(ast);
}

fn find_manifest_dir(scan_root: &Path) -> Option<PathBuf> {
    let start = scan_root.canonicalize().ok()?;
    let anchor = if start.is_dir() {
        start
    } else {
        start.parent()?.to_path_buf()
    };

    anchor
        .ancestors()
        .find(|dir| dir.join("Cargo.toml").exists())
        .map(Path::to_path_buf)
}

const METADATA_TIMEOUT: Duration = Duration::from_secs(60);

/// Loads workspace-local Cargo metadata without resolving dependencies.
///
/// `--no-deps --offline` never touches the network or the package cache lock.
/// Passing `--manifest-path` instead of changing into the project keeps rustup
/// resolving the toolchain from the caller's directory, so scanning a project
/// does not select (and auto-install) its `rust-toolchain.toml`; the output
/// format is toolchain-independent.
fn workspace_metadata(manifest_dir: &Path) -> Result<Metadata, String> {
    let mut command = MetadataCommand::new();
    command
        .manifest_path(manifest_dir.join("Cargo.toml"))
        .no_deps()
        .other_options(vec!["--offline".to_string()]);
    let mut child = command
        .cargo_command()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = read_in_background(child.stdout.take().expect("stdout is piped"));
    let stderr = read_in_background(child.stderr.take().expect("stderr is piped"));

    // A descendant of cargo can keep the pipes open after cargo exits, so
    // the deadline bounds reading the output as well as waiting for exit. On
    // timeout the reader threads are leaked.
    let deadline = Instant::now() + METADATA_TIMEOUT;
    let timed_out = |mut child: std::process::Child| {
        let _ = child.kill();
        let _ = child.wait();
        format!("timed out after {}s", METADATA_TIMEOUT.as_secs())
    };
    let output = match stdout.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(output) => output.map_err(|error| error.to_string())?,
        Err(_) => return Err(timed_out(child)),
    };
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            return Err(timed_out(child));
        }
        thread::sleep(Duration::from_millis(20));
    };
    if !status.success() {
        let stderr = stderr
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        return Err(
            match stderr.lines().rev().find(|line| !line.trim().is_empty()) {
                Some(line) => format!("{status}: {}", line.trim()),
                None => status.to_string(),
            },
        );
    }
    MetadataCommand::parse(output).map_err(|error| error.to_string())
}

fn read_in_background(
    mut pipe: impl Read + Send + 'static,
) -> mpsc::Receiver<std::io::Result<String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut output = String::new();
        let _ = sender.send(pipe.read_to_string(&mut output).map(|_| output));
    });
    receiver
}

/// Resolves the Cargo features of each workspace member (indexed like
/// `packages`) the way `cargo metadata` unifies them across the workspace,
/// following feature tables and dependency edges between workspace members.
/// Features of external dependencies are never needed for source analysis.
///
/// Known limitation: a path package outside the workspace that depends on a
/// member and enables its features is not followed, because `--no-deps`
/// metadata does not describe non-members.
fn workspace_features(
    packages: &[Package],
    patched: &HashMap<String, usize>,
    no_default_features: bool,
    requested: &[String],
) -> Vec<BTreeSet<String>> {
    let member_by_dir = packages
        .iter()
        .enumerate()
        .filter_map(|(index, package)| Some((package.manifest_path.parent()?, index)))
        .collect::<HashMap<_, _>>();
    let member = |dependency: &Dependency| match dependency.path.as_deref() {
        Some(path) => member_by_dir.get(path).copied(),
        None => patched.get(&dependency.name).copied(),
    };
    let alias = |dependency: &Dependency| {
        dependency
            .rename
            .clone()
            .unwrap_or_else(|| dependency.name.clone())
    };

    let mut queue = VecDeque::<(usize, String)>::new();
    let enable_dependency = |queue: &mut VecDeque<(usize, String)>, dependency: &Dependency| {
        if let Some(target) = member(dependency) {
            if dependency.uses_default_features {
                queue.push_back((target, "default".to_string()));
            }
            queue.extend(
                dependency
                    .features
                    .iter()
                    .map(|feature| (target, feature.clone())),
            );
        }
    };

    for (index, package) in packages.iter().enumerate() {
        if !no_default_features {
            queue.push_back((index, "default".to_string()));
        }
        for feature in requested.iter().flat_map(|features| {
            features
                .split([',', ' '])
                .filter(|feature| !feature.is_empty())
        }) {
            let feature = match feature.split_once('/') {
                Some((name, feature)) if name == package.name.as_str() => feature,
                _ => feature,
            };
            queue.push_back((index, feature.to_string()));
        }
        for dependency in package.dependencies.iter().filter(|dep| !dep.optional) {
            enable_dependency(&mut queue, dependency);
        }
    }

    let mut enabled = vec![BTreeSet::<String>::new(); packages.len()];
    let mut activated_dependencies = HashSet::<(usize, String)>::new();
    let mut deferred_weak = HashMap::<(usize, String), Vec<String>>::new();
    while let Some((index, item)) = queue.pop_front() {
        let package = &packages[index];
        let (dependency_name, dependency_feature, weak) = match item.split_once('/') {
            Some((name, feature)) => match name.strip_suffix('?') {
                Some(name) => (name, Some(feature), true),
                None => (name, Some(feature), false),
            },
            None => match item.strip_prefix("dep:") {
                Some(name) => (name, None, false),
                None => match package.features.get(&item) {
                    Some(items) => {
                        if enabled[index].insert(item.clone()) {
                            queue.extend(items.iter().map(|item| (index, item.clone())));
                        }
                        continue;
                    }
                    // A bare optional dependency name without an implicit feature.
                    None => (item.as_str(), None, false),
                },
            },
        };

        // `dep:name` only activates the dependency, never a feature of that name.
        if dependency_feature.is_some() && !weak && package.features.contains_key(dependency_name) {
            queue.push_back((index, dependency_name.to_string()));
        }
        let dependencies = package
            .dependencies
            .iter()
            .filter(|dependency| alias(dependency) == dependency_name);
        let key = (index, dependency_name.to_string());
        if !weak && activated_dependencies.insert(key.clone()) {
            for dependency in dependencies
                .clone()
                .filter(|dependency| dependency.optional)
            {
                enable_dependency(&mut queue, dependency);
            }
            let deferred = deferred_weak.remove(&key).unwrap_or_default();
            queue.extend(deferred.into_iter().map(|item| (index, item)));
        }
        let activated = activated_dependencies.contains(&key);
        for dependency in dependencies {
            if !activated && dependency.optional {
                deferred_weak
                    .entry(key.clone())
                    .or_default()
                    .push(item.clone());
            } else if let (Some(feature), Some(target)) = (dependency_feature, member(dependency)) {
                queue.push_back((target, feature.to_string()));
            }
        }
    }
    enabled
}

/// Maps package names that the workspace root `[patch.*]` tables redirect to a
/// workspace member onto that member's index in `metadata.packages`.
fn patched_members(metadata: &Metadata) -> HashMap<String, usize> {
    let root = metadata.workspace_root.as_std_path();
    let Some(manifest) = fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|manifest| manifest.parse::<toml::Table>().ok())
    else {
        return HashMap::new();
    };
    let member_dirs = metadata
        .packages
        .iter()
        .map(|package| {
            package
                .manifest_path
                .parent()
                .and_then(|dir| dir.canonicalize().ok())
        })
        .collect::<Vec<_>>();
    manifest
        .get("patch")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|sources| sources.values().filter_map(toml::Value::as_table))
        .flatten()
        .filter_map(|(name, patch)| {
            let dir = root
                .join(patch.get("path")?.as_str()?)
                .canonicalize()
                .ok()?;
            let index = member_dirs
                .iter()
                .position(|member| member.as_ref() == Some(&dir))?;
            let name = patch
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(name);
            Some((name.to_string(), index))
        })
        .collect()
}

fn rustc_cfgs() -> BTreeSet<String> {
    let Ok(output) = Command::new("rustc").args(["--print", "cfg"]).output() else {
        return BTreeSet::new();
    };
    if !output.status.success() {
        return BTreeSet::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use cargo_metadata::CargoOpt;

    use super::*;

    /// Oracle for `workspace_features`: compares it with cargo's full feature
    /// resolution for every polkadot-sdk workspace member. Needs the submodule
    /// and a populated cargo cache:
    ///
    /// ```sh
    /// git submodule update --init --depth 1 .repos/polkadot-sdk
    /// cargo fetch --manifest-path .repos/polkadot-sdk/Cargo.toml
    /// cargo test --lib -- --ignored workspace_features_match_cargo
    /// ```
    #[test]
    #[ignore = "needs .repos/polkadot-sdk and its dependencies in the cargo cache"]
    fn workspace_features_match_cargo_for_polkadot_sdk() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join(".repos/polkadot-sdk/Cargo.toml");
        assert!(manifest.exists(), "{} is missing", manifest.display());
        let workspace = workspace_metadata(manifest.parent().unwrap()).unwrap();
        let patched = patched_members(&workspace);
        for (no_default_features, features) in [
            (false, vec![]),
            (true, vec![]),
            (false, vec!["runtime-benchmarks".to_string()]),
        ] {
            let mut command = MetadataCommand::new();
            command
                .manifest_path(&manifest)
                .other_options(vec!["--offline".to_string()]);
            if no_default_features {
                command.features(CargoOpt::NoDefaultFeatures);
            }
            if !features.is_empty() {
                command.features(CargoOpt::SomeFeatures(features.clone()));
            }
            let expected = command
                .exec()
                .expect("full offline cargo metadata should succeed")
                .resolve
                .expect("full metadata has a resolve graph")
                .nodes
                .into_iter()
                .map(|node| {
                    let features = node.features.iter().map(ToString::to_string).collect();
                    (node.id, features)
                })
                .collect::<HashMap<_, BTreeSet<String>>>();
            let actual = workspace_features(
                &workspace.packages,
                &patched,
                no_default_features,
                &features,
            );
            let mismatches = workspace
                .packages
                .iter()
                .zip(actual)
                .filter(|(package, features)| expected.get(&package.id) != Some(features))
                .map(|(package, _)| package.name.to_string())
                .collect::<Vec<_>>();
            assert!(
                mismatches.is_empty(),
                "{} of {} members differ from cargo (no_default_features={no_default_features}, features={features:?}): {mismatches:?}",
                mismatches.len(),
                workspace.packages.len()
            );
        }
    }
}
