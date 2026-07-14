use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    process::Command,
};

use cargo_metadata::{CargoOpt, MetadataCommand, TargetKind};
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

        let mut metadata_command = MetadataCommand::new();
        metadata_command.current_dir(&manifest_dir);
        if no_default_features {
            metadata_command.features(CargoOpt::NoDefaultFeatures);
        }
        if !features.is_empty() {
            metadata_command.features(CargoOpt::SomeFeatures(features.to_vec()));
        }
        let metadata = metadata_command.exec().ok()?;

        let rustc_cfgs = rustc_cfgs();
        let resolved_features = metadata
            .resolve
            .as_ref()
            .map(|resolve| {
                resolve
                    .nodes
                    .iter()
                    .map(|node| {
                        (
                            node.id.to_string(),
                            node.features
                                .iter()
                                .map(ToString::to_string)
                                .collect::<BTreeSet<_>>(),
                        )
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        let mut file_target_kinds = HashMap::<PathBuf, Vec<SourceTargetKind>>::new();
        let mut proc_macro_source_roots = Vec::<PathBuf>::new();
        let mut file_cfgs = HashMap::<PathBuf, ActiveCfg>::new();
        for package in metadata.packages {
            let active_cfg = ActiveCfg::new(
                &rustc_cfgs,
                resolved_features
                    .get(&package.id.to_string())
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
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
