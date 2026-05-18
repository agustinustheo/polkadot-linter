use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Advisory,
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Advisory => write!(f, "advisory"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

impl FromStr for Severity {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "advisory" | "info" | "note" => Ok(Severity::Advisory),
            "warning" | "warn" => Ok(Severity::Warning),
            "error" | "err" | "deny" => Ok(Severity::Error),
            _ => Err(format!("Unknown severity: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleCategory {
    /// Rust semantic/structural rules
    Semantic,
    /// Test smell rules
    TestSmell,
    /// Benchmarking compliance rules
    Benchmark,
    /// Text and terminology rules
    Terminology,
}

impl fmt::Display for RuleCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleCategory::Semantic => write!(f, "semantic"),
            RuleCategory::TestSmell => write!(f, "test-smell"),
            RuleCategory::Benchmark => write!(f, "benchmark"),
            RuleCategory::Terminology => write!(f, "terminology"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule_id: String,
    pub rule_name: String,
    pub category: RuleCategory,
    pub severity: Severity,
    pub file: PathBuf,
    pub line: usize,
    pub column: Option<usize>,
    pub end_line: Option<usize>,
    pub message: String,
    pub explanation: String,
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn print_human(&self) {
        let severity_str = match self.severity {
            Severity::Error => "error".red().bold(),
            Severity::Warning => "warning".yellow().bold(),
            Severity::Advisory => "advisory".blue().bold(),
        };

        let location = if let Some(col) = self.column {
            format!("{}:{}:{}", self.file.display(), self.line, col)
        } else {
            format!("{}:{}", self.file.display(), self.line)
        };

        println!("{severity_str}[{}]: {}", self.rule_id.bold(), self.message);
        println!("  --> {location}");
        println!("  = {}: {}", "why".dimmed(), self.explanation);
        if let Some(ref suggestion) = self.suggestion {
            println!("  = {}: {}", "suggestion".green(), suggestion);
        }
        println!();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
    Sarif,
}

impl FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "human" | "text" => Ok(OutputFormat::Human),
            "json" => Ok(OutputFormat::Json),
            "sarif" => Ok(OutputFormat::Sarif),
            _ => Err(format!("Unknown output format: {s}")),
        }
    }
}

/// Produce a minimal SARIF 2.1.0 output
pub fn to_sarif(diagnostics: &[Diagnostic]) -> String {
    let results: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            serde_json::json!({
                "ruleId": d.rule_id,
                "level": match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Advisory => "note",
                },
                "message": { "text": d.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": d.file.display().to_string() },
                        "region": {
                            "startLine": d.line,
                            "startColumn": d.column.unwrap_or(1),
                        }
                    }
                }]
            })
        })
        .collect();

    let sarif = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "polkadot-linter",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://crates.io/crates/polkadot-linter",
                }
            },
            "results": results,
        }]
    });

    serde_json::to_string_pretty(&sarif).expect("SARIF serialization failed")
}
