use clap::Parser;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process,
};

use polkadot_linter::{
    config::Config, diagnostics, engine::LintEngine, rules::COMPILER_BACKED_RULE_IDS,
    rustc_pipeline,
};

#[derive(Parser, Debug)]
#[command(
    name = "polkadot-linter",
    version,
    about = "Polkadot SDK-specific linter"
)]
struct Cli {
    /// Paths to scan (defaults to current directory)
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Configuration file path
    #[arg(short, long, default_value = "polkadot-linter.toml")]
    config: PathBuf,

    /// Output format: human, json, or sarif
    #[arg(short = 'f', long, default_value = "human")]
    format: diagnostics::OutputFormat,

    /// Severity threshold: advisory, warning, or error
    #[arg(short, long, default_value = "advisory")]
    severity: diagnostics::Severity,

    /// Fail on warnings (exit code 1 if any warning or error)
    #[arg(long)]
    fail_on_warning: bool,

    /// Only check specific rule families (comma-separated)
    #[arg(long, value_delimiter = ',')]
    rules: Option<Vec<String>>,

    /// Glob patterns for files to include
    #[arg(long, value_delimiter = ',')]
    include: Option<Vec<String>>,

    /// Glob patterns for files to exclude
    #[arg(long, value_delimiter = ',')]
    exclude: Option<Vec<String>>,

    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Hide Cargo compiler progress for the compiler-backed analysis phase
    #[arg(long = "no-rustc-progress")]
    no_rustc_progress: bool,

    /// Skip syntax/token scanning and emit only auxiliary analysis results
    #[arg(long)]
    no_syntax: bool,

    /// Disable compiler-backed analysis; compiler-backed SEC rules are not run
    #[arg(long)]
    no_rustc: bool,

    /// Cargo manifest to analyze through the compiler-backed rustc driver
    #[arg(long = "rustc-cargo-manifest")]
    rustc_cargo_manifest: Option<PathBuf>,

    /// Package to pass to compiler-backed cargo check; may be repeated
    #[arg(long = "rustc-package")]
    rustc_packages: Vec<String>,

    /// Analyze only the library target for compiler-backed cargo check
    #[arg(long = "rustc-lib")]
    rustc_lib: bool,

    /// Pass --no-default-features to compiler-backed cargo check
    #[arg(long = "rustc-no-default-features")]
    rustc_no_default_features: bool,

    /// Cargo target directory for compiler-backed cargo check
    #[arg(long = "rustc-target-dir")]
    rustc_target_dir: Option<PathBuf>,

    /// rust toolchain passed to cargo for compiler-backed analysis
    #[arg(long = "rustc-toolchain", default_value = "nightly-2025-06-10")]
    rustc_toolchain: String,

    /// Path to the polkadot-linter-rustc driver binary
    #[arg(
        long = "rustc-driver",
        default_value = "target/debug/polkadot-linter-rustc"
    )]
    rustc_driver: PathBuf,

    /// Rule IDs to run through the compiler-backed rustc driver
    #[arg(long = "compiler-backed-rules", value_delimiter = ',')]
    compiler_backed_rules: Vec<String>,

    /// File substring filters for compiler-backed diagnostics
    #[arg(long = "rustc-source-filter", value_delimiter = ',')]
    rustc_source_filters: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::init();
    }

    let config = match Config::load(&cli.config) {
        Ok(c) => c,
        Err(e) => {
            if cli.config.to_str() == Some("polkadot-linter.toml") && !cli.config.exists() {
                log::info!("No config file found, using defaults");
                Config::default()
            } else {
                eprintln!("Error loading config: {e}");
                process::exit(2);
            }
        }
    };

    let mut engine = LintEngine::new(config.clone());

    // Apply CLI overrides
    if let Some(ref rules) = cli.rules {
        engine.filter_rules(rules);
    }
    if let Some(ref include) = cli.include {
        engine.set_include_patterns(include);
    }
    if let Some(ref exclude) = cli.exclude {
        engine.set_exclude_patterns(exclude);
    }

    let manifest_path = match effective_rustc_manifest(&cli) {
        Ok(manifest_path) => manifest_path,
        Err(error) => {
            eprintln!("Error discovering Cargo project: {error}");
            process::exit(2);
        }
    };
    let auto_source_filters = if cli.rustc_source_filters.is_empty()
        && cli.rustc_cargo_manifest.is_none()
        && manifest_path.is_some()
    {
        rustc_source_filters_for_paths(&cli.paths)
    } else {
        Vec::new()
    };

    let mut results = Vec::new();
    let compiler_backed_rules =
        selected_compiler_backed_rules(cli.rules.as_deref(), &cli.compiler_backed_rules, &config);
    if !cli.no_syntax {
        for path in &cli.paths {
            results.extend(engine.scan(path));
        }
    }

    if !cli.no_rustc && !compiler_backed_rules.is_empty() {
        if let Some(manifest_path) = &manifest_path {
            if !cli.rustc_driver.is_file() {
                eprintln!(
                    "Compiler-backed analysis requires {}. Build it with `cargo +nightly-2025-06-10 build --features rustc-driver --bin polkadot-linter-rustc`.",
                    cli.rustc_driver.display()
                );
                process::exit(2);
            }
            let options = rustc_pipeline::RustcPipelineOptions {
                manifest_path: manifest_path.clone(),
                packages: cli.rustc_packages.clone(),
                driver_path: cli.rustc_driver.clone(),
                toolchain: cli.rustc_toolchain.clone(),
                target_dir: cli.rustc_target_dir.clone(),
                rules: compiler_backed_rules.clone(),
                file_filters: if cli.rustc_source_filters.is_empty() {
                    auto_source_filters
                } else {
                    cli.rustc_source_filters.clone()
                },
                lib: cli.rustc_lib,
                no_default_features: cli.rustc_no_default_features,
                show_cargo_progress: !cli.no_rustc_progress,
            };
            match rustc_pipeline::run_cargo_check(&options) {
                Ok(mut diagnostics) => results.append(&mut diagnostics),
                Err(e) => {
                    eprintln!("Error running compiler-backed analysis: {e}");
                    process::exit(2);
                }
            }
        }
    }

    let filtered = results
        .into_iter()
        .filter(|d| d.severity >= cli.severity)
        .collect::<Vec<_>>();

    match cli.format {
        diagnostics::OutputFormat::Human => {
            for d in &filtered {
                d.print_human();
            }
        }
        diagnostics::OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&filtered).expect("JSON serialization failed");
            println!("{json}");
        }
        diagnostics::OutputFormat::Sarif => {
            let sarif = diagnostics::to_sarif(&filtered);
            println!("{sarif}");
        }
    }

    let has_errors = filtered
        .iter()
        .any(|d| d.severity == diagnostics::Severity::Error);
    let has_warnings = filtered
        .iter()
        .any(|d| d.severity == diagnostics::Severity::Warning);

    if has_errors {
        eprintln!(
            "\npolkadot-linter: {} diagnostic(s) emitted ({} error(s))",
            filtered.len(),
            filtered
                .iter()
                .filter(|d| d.severity == diagnostics::Severity::Error)
                .count()
        );
        process::exit(1);
    } else if has_warnings && cli.fail_on_warning {
        eprintln!(
            "\npolkadot-linter: {} diagnostic(s) emitted ({} warning(s), --fail-on-warning is set)",
            filtered.len(),
            filtered
                .iter()
                .filter(|d| d.severity == diagnostics::Severity::Warning)
                .count()
        );
        process::exit(1);
    } else if !filtered.is_empty() {
        eprintln!(
            "\npolkadot-linter: {} diagnostic(s) emitted",
            filtered.len()
        );
    }
}

fn effective_rustc_manifest(cli: &Cli) -> Result<Option<PathBuf>, String> {
    if cli.no_rustc {
        return Ok(None);
    }
    if let Some(manifest_path) = &cli.rustc_cargo_manifest {
        return Ok(Some(manifest_path.clone()));
    }
    discover_cargo_manifest(&cli.paths)
}

fn discover_cargo_manifest(paths: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    let manifests = paths
        .iter()
        .filter_map(|path| nearest_cargo_manifest(path).transpose())
        .collect::<Result<BTreeSet<_>, _>>()?;

    match manifests.len() {
        0 => Ok(None),
        1 => Ok(manifests.into_iter().next()),
        _ => Err(format!(
            "scan paths belong to multiple Cargo projects ({}); pass --rustc-cargo-manifest explicitly",
            manifests
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn nearest_cargo_manifest(path: &Path) -> Result<Option<PathBuf>, String> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    let mut candidate = if absolute_path.is_file() {
        absolute_path.parent().map(Path::to_path_buf)
    } else {
        Some(absolute_path)
    };

    while let Some(directory) = candidate {
        let manifest = directory.join("Cargo.toml");
        if manifest.is_file() {
            return manifest
                .canonicalize()
                .map(Some)
                .map_err(|error| error.to_string());
        }
        candidate = directory.parent().map(Path::to_path_buf);
    }

    Ok(None)
}

fn rustc_source_filters_for_paths(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .map(|path| path.display().to_string())
        .collect()
}

fn selected_compiler_backed_rules(
    cli_rules: Option<&[String]>,
    explicit_compiler_rules: &[String],
    config: &Config,
) -> Vec<String> {
    let selectors = if explicit_compiler_rules.is_empty() {
        cli_rules.unwrap_or_default()
    } else {
        explicit_compiler_rules
    };

    if selectors.is_empty() {
        return COMPILER_BACKED_RULE_IDS
            .iter()
            .filter(|rule| config.rule_enabled(rule))
            .map(|rule| (*rule).to_string())
            .collect();
    }

    COMPILER_BACKED_RULE_IDS
        .iter()
        .filter(|rule_id| {
            selectors.iter().any(|selector| {
                *rule_id == selector
                    || (selector == "SEC" && rule_id.starts_with("SEC"))
                    || (selector == "VAL" && rule_id.starts_with("VAL"))
            })
        })
        .filter(|rule_id| config.rule_enabled(rule_id))
        .map(|rule| (*rule).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use polkadot_linter::config::Config;

    use super::{discover_cargo_manifest, selected_compiler_backed_rules};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn default_compiler_backed_rules_cover_migrated_rules() {
        assert_eq!(
            selected_compiler_backed_rules(None, &[], &Config::default()),
            strings(&[
                "VAL003", "SEC001", "SEC002", "SEC003", "SEC004", "SEC005", "SEC006", "SEC007",
                "SEC008", "SEC009", "SEC010", "SEC011", "SEC012", "SEC013", "SEC014", "SEC015",
                "SEC016", "SEC017", "SEC018",
            ])
        );
    }

    #[test]
    fn cli_security_family_expands_to_migrated_rules_only() {
        let cli_rules = strings(&["SEC"]);

        assert_eq!(
            selected_compiler_backed_rules(Some(&cli_rules), &[], &Config::default()),
            strings(&[
                "SEC001", "SEC002", "SEC003", "SEC004", "SEC005", "SEC006", "SEC007", "SEC008",
                "SEC009", "SEC010", "SEC011", "SEC012", "SEC013", "SEC014", "SEC015", "SEC016",
                "SEC017", "SEC018",
            ])
        );
    }

    #[test]
    fn cli_specific_rules_select_only_migrated_matches() {
        let cli_rules = strings(&["SEC003", "SEC014", "VAL"]);

        assert_eq!(
            selected_compiler_backed_rules(Some(&cli_rules), &[], &Config::default()),
            strings(&["VAL003", "SEC003", "SEC014"])
        );
    }

    #[test]
    fn explicit_compiler_rules_override_cli_rule_filter() {
        let cli_rules = strings(&["VAL"]);
        let compiler_rules = strings(&["SEC009"]);

        assert_eq!(
            selected_compiler_backed_rules(Some(&cli_rules), &compiler_rules, &Config::default()),
            strings(&["SEC009"])
        );
    }

    #[test]
    fn disabled_compiler_backed_rules_are_not_sent_to_the_driver() {
        let mut config = Config::default();
        config.rules.enabled.insert("SEC009".to_string(), false);

        assert!(!selected_compiler_backed_rules(None, &[], &config)
            .iter()
            .any(|rule| rule == "SEC009"));
    }

    #[test]
    fn discovers_nearest_manifest_for_source_path() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let crate_dir = dir.path().join("crate");
        let source_dir = crate_dir.join("src/nested");
        fs::create_dir_all(&source_dir).expect("source directory should be created");
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .expect("manifest should be written");
        let source_path = source_dir.join("lib.rs");
        fs::write(&source_path, "pub fn fixture() {}\n").expect("source should be written");

        assert_eq!(
            discover_cargo_manifest(&[source_path]).expect("manifest discovery should succeed"),
            Some(
                crate_dir
                    .join("Cargo.toml")
                    .canonicalize()
                    .expect("manifest should canonicalize")
            )
        );
    }

    #[test]
    fn rejects_scan_paths_from_multiple_cargo_projects() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        for crate_dir in [&first, &second] {
            fs::create_dir_all(crate_dir.join("src")).expect("source directory should be created");
            fs::write(
                crate_dir.join("Cargo.toml"),
                "[package]\nname = \"fixture\"\n",
            )
            .expect("manifest should be written");
        }

        let error = discover_cargo_manifest(&[first.join("src"), second.join("src")])
            .expect_err("multiple projects should require an explicit manifest");

        assert!(error.contains("multiple Cargo projects"));
    }
}
