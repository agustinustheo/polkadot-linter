use std::path::{Path, PathBuf};

use syn::File as SynFile;
use walkdir::WalkDir;

use crate::{
    config::Config,
    diagnostics::Diagnostic,
    project_model::{ProjectModel, SourceTargetKind},
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
        let mut diagnostics = Vec::new();
        let project_model = ProjectModel::discover(root);
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

            let file_ctx = FileContext {
                path: path.to_path_buf(),
                rel_path: rel_path.to_path_buf(),
                content: &content,
                is_rust,
                is_text,
                is_test_file: source_target_kinds
                    .iter()
                    .any(|kind| matches!(kind, SourceTargetKind::Test))
                    || Self::is_test_file(path, &content),
                is_benchmark_file: source_target_kinds
                    .iter()
                    .any(|kind| matches!(kind, SourceTargetKind::Bench))
                    || Self::is_benchmark_file(path),
                source_target_kinds,
                ast: if is_rust {
                    syn::parse_file(&content).ok()
                } else {
                    None
                },
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
        let path_str = path.to_string_lossy();
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
        let path_str = path.to_string_lossy();
        path_str.contains("/benchmarking")
            || path_str.contains("/benchmarks")
            || path_str.ends_with("benchmarking.rs")
            || path_str.ends_with("benchmarks.rs")
    }
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
}
