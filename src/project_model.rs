use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    process::Command,
};

use cargo_metadata::{MetadataCommand, TargetKind};

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
}

impl ProjectModel {
    pub fn discover(scan_root: &Path) -> Option<Self> {
        let manifest_dir = find_manifest_dir(scan_root)?;

        let metadata = MetadataCommand::new()
            .current_dir(&manifest_dir)
            .no_deps()
            .exec()
            .ok()?;

        let mut file_target_kinds = HashMap::<PathBuf, Vec<SourceTargetKind>>::new();
        let mut proc_macro_source_roots = Vec::<PathBuf>::new();
        for package in metadata.packages {
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
                file_target_kinds.entry(src_path).or_default().extend(kinds);
            }
        }

        Some(Self {
            file_target_kinds,
            proc_macro_source_roots,
            rustc_cfgs: rustc_cfgs(),
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
