use clap::Parser;
use std::{path::PathBuf, process};

use polkadot_linter::{
    config::Config, diagnostics, engine::LintEngine, rustc_pipeline, rustdoc_analysis,
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

    /// rustdoc JSON files to analyze with the experimental rustc-backed path
    #[arg(long = "rustdoc-json")]
    rustdoc_json: Vec<PathBuf>,

    /// Source root used to resolve relative rustdoc JSON spans
    #[arg(long = "rustdoc-source-root")]
    rustdoc_source_root: Option<PathBuf>,

    /// Skip syntax/token scanning and emit only auxiliary analysis results
    #[arg(long)]
    no_syntax: bool,

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

    /// rust toolchain passed to cargo, for example nightly-2025-06-10
    #[arg(long = "rustc-toolchain")]
    rustc_toolchain: Option<String>,

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

    let mut results = Vec::new();
    if !cli.no_syntax {
        for path in &cli.paths {
            results.extend(engine.scan(path));
        }
    }

    if let Some(manifest_path) = &cli.rustc_cargo_manifest {
        let rustc_rules = if cli.compiler_backed_rules.is_empty() {
            cli.rules.clone().unwrap_or_default()
        } else {
            cli.compiler_backed_rules.clone()
        };
        let options = rustc_pipeline::RustcPipelineOptions {
            manifest_path: manifest_path.clone(),
            packages: cli.rustc_packages.clone(),
            driver_path: cli.rustc_driver.clone(),
            toolchain: cli.rustc_toolchain.clone(),
            target_dir: cli.rustc_target_dir.clone(),
            rules: rustc_rules,
            file_filters: cli.rustc_source_filters.clone(),
            lib: cli.rustc_lib,
            no_default_features: cli.rustc_no_default_features,
        };
        match rustc_pipeline::run_cargo_check(&options) {
            Ok(mut diagnostics) => results.append(&mut diagnostics),
            Err(e) => {
                eprintln!("Error running compiler-backed analysis: {e}");
                process::exit(2);
            }
        }
    }

    let run_rustdoc_sec013 = cli
        .rules
        .as_ref()
        .is_none_or(|rules| rules.iter().any(|rule| rule == "SEC" || rule == "SEC013"));
    if run_rustdoc_sec013 {
        for rustdoc_json_path in &cli.rustdoc_json {
            let content = match std::fs::read_to_string(rustdoc_json_path) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!(
                        "Error loading rustdoc JSON {}: {e}",
                        rustdoc_json_path.display()
                    );
                    process::exit(2);
                }
            };
            match rustdoc_analysis::analyze_rustdoc_json_str(
                &content,
                cli.rustdoc_source_root.as_deref(),
                &config,
            ) {
                Ok(mut diagnostics) => results.append(&mut diagnostics),
                Err(e) => {
                    eprintln!(
                        "Error analyzing rustdoc JSON {}: {e}",
                        rustdoc_json_path.display()
                    );
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
