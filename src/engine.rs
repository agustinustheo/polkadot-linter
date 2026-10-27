use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    hash::Hash,
    path::{Path, PathBuf},
};

use syn::File as SynFile;
use walkdir::WalkDir;

use crate::{
    config::Config,
    diagnostics::Diagnostic,
    project_model::{expand_active_cfg_attrs, ProjectModel, SourceTargetKind},
    rules::{self, LintRule},
};

pub struct LintEngine {
    config: Config,
    rules: Vec<Box<dyn LintRule>>,
    include_patterns: Vec<glob::Pattern>,
    exclude_patterns: Vec<glob::Pattern>,
}

impl LintEngine {
    pub fn new(config: Config) -> Self {
        Self::try_new(config).expect("lint configuration glob patterns must be valid")
    }

    pub fn try_new(config: Config) -> Result<Self, glob::PatternError> {
        let exclude_patterns = compile_patterns(&config.general.exclude)?;
        let include_patterns = compile_patterns(&config.general.include)?;

        let rules = rules::all_rules(&config);

        Ok(LintEngine {
            config,
            rules,
            include_patterns,
            exclude_patterns,
        })
    }

    pub fn filter_rules(&mut self, families: &[String]) {
        self.rules.retain(|r| {
            families
                .iter()
                .any(|f| r.family() == f || r.id().starts_with(f))
        });
    }

    pub fn set_include_patterns(&mut self, patterns: &[String]) -> Result<(), glob::PatternError> {
        self.include_patterns = compile_patterns(patterns)?;
        Ok(())
    }

    pub fn set_exclude_patterns(&mut self, patterns: &[String]) -> Result<(), glob::PatternError> {
        self.exclude_patterns = compile_patterns(patterns)?;
        Ok(())
    }

    pub fn scan(&self, root: &Path) -> Vec<Diagnostic> {
        self.scan_with_cargo_features(root, false, &[])
    }

    pub fn scan_with_cargo_features(
        &self,
        root: &Path,
        no_default_features: bool,
        features: &[String],
    ) -> Vec<Diagnostic> {
        let _scan_cache = ScanCache::begin();
        let mut diagnostics = Vec::new();
        let project_model =
            ProjectModel::discover_with_features(root, no_default_features, features);
        if let Some(model) = &project_model {
            log::debug!(
                "Loaded project model for {} with {} rustc cfgs",
                root.display(),
                model.rustc_cfgs().len()
            );
        }

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if !entry.file_type().is_file() {
                continue;
            }

            // Check file extension
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let is_rust = ext == "rs";
            let is_text = matches!(ext, "md" | "txt" | "toml" | "yaml" | "yml");

            if !is_rust && !is_text {
                continue;
            }

            let rel_path = path.strip_prefix(root).unwrap_or(path);
            let rel_str = rel_path.to_string_lossy();

            // Apply exclude patterns
            if self.exclude_patterns.iter().any(|p| p.matches(&rel_str)) {
                continue;
            }

            // Apply include patterns (if any specified)
            if !self.include_patterns.is_empty()
                && !self.include_patterns.iter().any(|p| p.matches(&rel_str))
            {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("Could not read {}: {e}", path.display());
                    continue;
                }
            };

            let source_target_kinds = project_model
                .as_ref()
                .map(|model| model.target_kinds_for_file(path))
                .unwrap_or_default();

            let mut ast = if is_rust {
                syn::parse_file(&content).ok()
            } else {
                None
            };
            if let (Some(model), Some(ast)) = (&project_model, &mut ast) {
                if let Some(active_cfg) = model.active_cfg_for_file(path) {
                    expand_active_cfg_attrs(ast, active_cfg);
                }
            }

            let class_path = classification_path(path, rel_path);
            let file_ctx = FileContext {
                path: path.to_path_buf(),
                rel_path: rel_path.to_path_buf(),
                content: &content,
                is_rust,
                is_text,
                is_test_file: source_target_kinds
                    .iter()
                    .any(|kind| matches!(kind, SourceTargetKind::Test))
                    || Self::is_test_file(&class_path, &content),
                is_benchmark_file: source_target_kinds
                    .iter()
                    .any(|kind| matches!(kind, SourceTargetKind::Bench))
                    || Self::is_benchmark_file(&class_path),
                source_target_kinds,
                ast,
                classification_path: class_path,
            };

            for rule in &self.rules {
                if !self.config.rule_enabled(rule.id()) {
                    continue;
                }

                if let Some(mut diags) = rule.check(&file_ctx, &self.config) {
                    diagnostics.append(&mut diags);
                }
            }
        }

        // Sort by file, then line
        diagnostics.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

        diagnostics
    }

    pub(crate) fn is_test_file(path: &Path, _content: &str) -> bool {
        let path_str = path.to_string_lossy().replace('\\', "/");
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        path_str.contains("/tests/")
            || path_str.contains("/test/")
            || path_str.contains("/test-utils/")
            || path_str.contains("integration_tests")
            || path_str.contains("integration-tests")
            || path_str.ends_with("_test.rs")
            || path_str.ends_with("_tests.rs")
            || file_name == "mock.rs"
            || file_name == "tests.rs"
            || file_name == "test.rs"
            || file_name == "testing_utils.rs"
    }

    pub(crate) fn is_benchmark_file(path: &Path) -> bool {
        let path_str = path.to_string_lossy().replace('\\', "/");
        path_str.contains("/benchmarking")
            || path_str.contains("/benchmarks")
            || path_str.ends_with("benchmarking.rs")
            || path_str.ends_with("benchmarks.rs")
    }
}

/// The path test/bench classifiers see, rooted at `/` so `/tests/`-style
/// patterns match a first component. It starts at the owning crate root
/// (nearest ancestor with a `Cargo.toml`) or the scan root, whichever is
/// higher: directories above both (e.g. a checkout under `~/tests/`) never
/// classify a file, while scanning a crate's `tests/` or `benches/` directory,
/// or a single helper file in it, still sees that directory.
pub fn classification_path(path: &Path, rel_path: &Path) -> PathBuf {
    let crate_rel = crate_root(path).and_then(|root| path.strip_prefix(root).ok());
    let rel = match (crate_rel, path.file_name()) {
        (Some(crate_rel), _) if crate_rel.components().count() > rel_path.components().count() => {
            crate_rel
        }
        (_, Some(name)) if rel_path.as_os_str().is_empty() => Path::new(name),
        _ => rel_path,
    };
    Path::new("/").join(rel)
}

/// The nearest ancestor directory of `path` containing a `Cargo.toml`.
pub(crate) fn crate_root(path: &Path) -> Option<&Path> {
    if !path.is_file() {
        return None;
    }
    path.parent()?
        .ancestors()
        .find(|dir| dir.join("Cargo.toml").is_file())
}

thread_local! {
    static SCAN_CACHES: RefCell<Option<HashMap<TypeId, Box<dyn Any>>>> = const { RefCell::new(None) };
}

// Cache only for an engine scan, so subsequent scans observe filesystem and
// configuration changes. proc_macro2 tokens remain on the scanning thread.
struct ScanCache;
impl ScanCache {
    fn begin() -> Self {
        SCAN_CACHES.with(|caches| *caches.borrow_mut() = Some(HashMap::new()));
        Self
    }
}
impl Drop for ScanCache {
    fn drop(&mut self) {
        SCAN_CACHES.with(|caches| *caches.borrow_mut() = None);
    }
}

/// Memoises `load(key)` for the current engine scan; outside a scan nothing
/// is cached. Each `(K, V)` pair is a separate cache, and `load` may itself
/// use other scan caches.
pub(crate) fn scan_cached<K, V>(key: K, load: impl FnOnce(&K) -> V) -> V
where
    K: Eq + Hash + 'static,
    V: Clone + 'static,
{
    let cached = SCAN_CACHES.with(|caches| {
        let caches = caches.borrow();
        let cache = caches.as_ref()?.get(&TypeId::of::<HashMap<K, V>>())?;
        cache.downcast_ref::<HashMap<K, V>>()?.get(&key).cloned()
    });
    if let Some(value) = cached {
        return value;
    }
    let value = load(&key);
    SCAN_CACHES.with(|caches| {
        if let Some(caches) = caches.borrow_mut().as_mut() {
            let cache = caches
                .entry(TypeId::of::<HashMap<K, V>>())
                .or_insert_with(|| Box::new(HashMap::<K, V>::new()));
            if let Some(cache) = cache.downcast_mut::<HashMap<K, V>>() {
                cache.insert(key, value.clone());
            }
        }
    });
    value
}

fn compile_patterns(patterns: &[String]) -> Result<Vec<glob::Pattern>, glob::PatternError> {
    patterns
        .iter()
        .map(|pattern| glob::Pattern::new(pattern))
        .collect()
}

/// Context passed to each rule when checking a file
pub struct FileContext<'a> {
    pub path: PathBuf,
    pub rel_path: PathBuf,
    pub content: &'a str,
    pub is_rust: bool,
    pub is_text: bool,
    pub is_test_file: bool,
    pub is_benchmark_file: bool,
    pub source_target_kinds: Vec<SourceTargetKind>,
    pub ast: Option<SynFile>,
    /// The path test/bench classifiers should match; see [`classification_path`].
    pub classification_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::config::Config;

    use super::LintEngine;

    #[test]
    fn target_classification_accepts_windows_path_separators() {
        assert!(LintEngine::is_test_file(
            Path::new(r"project\tests\integration.rs"),
            ""
        ));
        assert!(LintEngine::is_benchmark_file(Path::new(
            r"project\benchmarks\weights.rs"
        )));
    }

    #[test]
    fn source_analysis_uses_selected_cargo_features_for_cfg_attrs() {
        let dir = tempfile::tempdir().expect("temporary crate should be created");
        fs::create_dir_all(dir.path().join("src")).expect("source directory should be created");
        fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[package]
name = "cfg-attribute-fixture"
version = "0.1.0"
edition = "2021"

[features]
default = ["default-weight"]
default-weight = []
explicit-weight = []
"#,
        )
        .expect("manifest should be written");
        fs::write(
            dir.path().join("src/lib.rs"),
            r#"
#[cfg_attr(feature = "default-weight", pallet::weight(Weight::zero()))]
pub fn default_weight() {}

#[cfg_attr(feature = "explicit-weight", pallet::weight(Weight::zero()))]
pub fn explicit_weight() {}

#[cfg_attr(not(custom_cfg), pallet::weight(Weight::zero()))]
pub fn custom_cfg_weight() {}
"#,
        )
        .expect("source should be written");

        let mut engine = LintEngine::new(Config::default());
        engine.filter_rules(&["SEM011".to_string()]);

        let defaults = engine.scan(dir.path());
        assert_eq!(
            defaults
                .iter()
                .filter(|diagnostic| diagnostic.rule_id == "SEM011")
                .count(),
            1,
            "default Cargo features should activate the matching cfg_attr"
        );

        let without_defaults = engine.scan_with_cargo_features(dir.path(), true, &[]);
        assert!(
            without_defaults.is_empty(),
            "--no-default-features must remove source cfg_attr evidence"
        );

        let explicit =
            engine.scan_with_cargo_features(dir.path(), true, &["explicit-weight".to_string()]);
        assert_eq!(
            explicit
                .iter()
                .filter(|diagnostic| diagnostic.rule_id == "SEM011")
                .count(),
            1,
            "explicit Cargo features should activate matching source cfg_attr values"
        );
    }

    #[test]
    fn source_analysis_unifies_features_across_workspace_members() {
        let dir = tempfile::tempdir().expect("temporary workspace should be created");
        let write = |path: &str, contents: &str| {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).expect("directory should be created");
            fs::write(path, contents).expect("file should be written");
        };
        write(
            "Cargo.toml",
            "[workspace]\nmembers = [\"weights\", \"runtime\"]\nresolver = \"2\"\n",
        );
        write(
            "weights/Cargo.toml",
            "[package]\nname = \"weights\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[features]\nbenchmarks = []\n",
        );
        write(
            "weights/src/lib.rs",
            "#[cfg_attr(feature = \"benchmarks\", pallet::weight(Weight::zero()))]\npub fn weighted() {}\n",
        );
        write(
            "runtime/Cargo.toml",
            "[package]\nname = \"runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nweights = { path = \"../weights\", optional = true }\n\n[features]\ndefault = [\"bench\"]\nbench = [\"weights?/benchmarks\", \"dep:weights\"]\n",
        );
        write("runtime/src/lib.rs", "");

        let mut engine = LintEngine::new(Config::default());
        engine.filter_rules(&["SEM011".to_string()]);

        assert_eq!(
            engine.scan(dir.path()).len(),
            1,
            "a member's feature enabled by another member must activate its cfg_attr"
        );
        assert!(
            engine
                .scan_with_cargo_features(dir.path(), true, &[])
                .is_empty(),
            "without default features nothing enables the dependency feature"
        );
    }

    /// Writes `files` into a temporary workspace and returns how many SEM011
    /// findings a scan reports with `no_default_features` and `features`.
    fn sem011_findings(
        files: &[(&str, &str)],
        no_default_features: bool,
        features: &[&str],
    ) -> usize {
        let dir = tempfile::tempdir().expect("temporary workspace should be created");
        for (path, contents) in files {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).expect("directory should be created");
            fs::write(path, contents).expect("file should be written");
        }
        let mut engine = LintEngine::new(Config::default());
        engine.filter_rules(&["SEM011".to_string()]);
        let features = features.iter().map(ToString::to_string).collect::<Vec<_>>();
        engine
            .scan_with_cargo_features(dir.path(), no_default_features, &features)
            .len()
    }

    const WEIGHTED: &str =
        "#[cfg_attr(feature = \"weight\", pallet::weight(Weight::zero()))]\npub fn weighted() {}\n";

    #[test]
    fn dep_prefix_activates_only_the_dependency_not_a_same_named_feature() {
        let files = [
            ("Cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n"),
            (
                "a/Cargo.toml",
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nb = { path = \"../b\", optional = true }\n\n[features]\ndefault = [\"enable\"]\nenable = [\"dep:b\"]\nb = [\"weight\"]\nweight = []\n",
            ),
            ("a/src/lib.rs", WEIGHTED),
            ("b/Cargo.toml", "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
            ("b/src/lib.rs", ""),
        ];
        assert_eq!(sem011_findings(&files, false, &[]), 0);
    }

    #[test]
    fn patched_dependency_on_a_member_enables_its_features() {
        let files = [
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n\n[workspace.dependencies]\nb = { version = \"0.1.0\", default-features = false, features = [\"weight\"] }\n\n[patch.crates-io]\nb = { path = \"b\" }\n",
            ),
            (
                "a/Cargo.toml",
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nb = { workspace = true }\n",
            ),
            ("a/src/lib.rs", ""),
            (
                "b/Cargo.toml",
                "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[features]\nweight = []\n",
            ),
            ("b/src/lib.rs", WEIGHTED),
        ];
        assert_eq!(sem011_findings(&files, true, &[]), 1);
    }

    #[test]
    fn package_qualified_feature_applies_to_that_member_and_its_dependents_only() {
        // Matches `cargo metadata --features b/weight` at the workspace root:
        // member `b` gets `weight`, member `a` activates its optional `b`
        // dependency (and implicit feature `b`), and member `c` gets nothing.
        let files = [
            ("Cargo.toml", "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\nresolver = \"2\"\n"),
            (
                "a/Cargo.toml",
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nb = { path = \"../b\", optional = true }\n\n[features]\nweight = []\n",
            ),
            ("a/src/lib.rs", "#[cfg_attr(feature = \"b\", pallet::weight(Weight::zero()))]\npub fn weighted() {}\n"),
            (
                "b/Cargo.toml",
                "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[features]\nweight = []\n",
            ),
            ("b/src/lib.rs", WEIGHTED),
            (
                "c/Cargo.toml",
                "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[features]\nweight = []\n",
            ),
            ("c/src/lib.rs", &WEIGHTED.repeat(2)),
        ];
        // `a` (feature `b`) and `b`; `c` would add two more.
        assert_eq!(sem011_findings(&files, false, &["b/weight"]), 2);
        // `b` and both of `c`.
        assert_eq!(sem011_findings(&files, false, &["weight"]), 3);
    }
}
