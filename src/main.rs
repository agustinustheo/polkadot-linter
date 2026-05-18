use clap::Parser;
use std::{path::PathBuf, process};

use polkadot_linter::{config::Config, diagnostics, engine::LintEngine};

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

    let mut engine = LintEngine::new(config);

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
    for path in &cli.paths {
        results.extend(engine.scan(path));
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
