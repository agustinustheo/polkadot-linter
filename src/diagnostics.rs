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
    /// Text and terminology rules
    Terminology,
    /// Runtime configuration rules
    RuntimeConfig,
}

impl fmt::Display for RuleCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleCategory::Semantic => write!(f, "semantic"),
            RuleCategory::TestSmell => write!(f, "test-smell"),
            RuleCategory::Terminology => write!(f, "terminology"),
            RuleCategory::RuntimeConfig => write!(f, "runtime-config"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    #[default]
    Medium,
    High,
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Confidence::Low => write!(f, "low"),
            Confidence::Medium => write!(f, "medium"),
            Confidence::High => write!(f, "high"),
        }
    }
}

impl FromStr for Confidence {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low" => Ok(Confidence::Low),
            "medium" | "med" => Ok(Confidence::Medium),
            "high" => Ok(Confidence::High),
            _ => Err(format!("Unknown confidence: {s}")),
        }
    }
}

/// Explicit confidence mapping for known rule IDs.
pub fn known_rule_confidence(rule_id: &str) -> Option<Confidence> {
    match rule_id {
        "RTC007" | "RTC008" | "RTC009" | "RTC010" | "RTC011" | "SEC006" | "SEC010" | "SEC012"
        | "SEC015" | "SEC019" | "SEC020" | "SEC021" | "SEC022" | "SEC023" | "SEM010" | "SEM019"
        | "SEM020" | "TST002" | "LNT001" => Some(Confidence::High),
        "RTC001" | "RTC003" | "RTC005" | "RTC006" | "SEC024" | "SEM011" | "SEM015" | "SEM016" => {
            Some(Confidence::Medium)
        }
        "RTC004" | "SEC004" | "SEC011" | "SEM009" | "TRM001" => Some(Confidence::Low),
        _ => None,
    }
}

/// Default triage confidence for a rule when a rule does not override it.
pub fn default_confidence(rule_id: &str) -> Confidence {
    known_rule_confidence(rule_id).unwrap_or(Confidence::Medium)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub rule_id: String,
    pub rule_name: String,
    pub category: RuleCategory,
    pub severity: Severity,
    #[serde(default)]
    pub confidence: Confidence,
    pub file: PathBuf,
    pub line: usize,
    pub column: Option<usize>,
    pub end_line: Option<usize>,
    pub message: String,
    pub explanation: String,
    pub suggestion: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also_in: Vec<String>,
}

/// Group diagnostics that share `(rule_id, message)` into a single
/// representative diagnostic, keeping the highest-severity/confidence
/// occurrence as the primary location and recording all other occurrences in
/// `also_in`.
pub fn group_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut grouped: Vec<Diagnostic> = Vec::new();
    let mut index_by_key: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();

    for mut diag in diagnostics {
        let key = (diag.rule_id.clone(), diag.message.clone());
        if let Some(&idx) = index_by_key.get(&key) {
            let rep = &mut grouped[idx];
            if (diag.severity, diag.confidence) > (rep.severity, rep.confidence) {
                let old_loc = format!("{}:{}", rep.file.display(), rep.line);
                let mut combined_also = std::mem::take(&mut rep.also_in);
                if !combined_also.contains(&old_loc) {
                    combined_also.push(old_loc);
                }
                for extra in std::mem::take(&mut diag.also_in) {
                    if !combined_also.contains(&extra) {
                        combined_also.push(extra);
                    }
                }
                diag.also_in = combined_also;
                *rep = diag;
            } else {
                let loc = format!("{}:{}", diag.file.display(), diag.line);
                if !rep.also_in.contains(&loc) {
                    rep.also_in.push(loc);
                }
                for extra in diag.also_in {
                    if !rep.also_in.contains(&extra) {
                        rep.also_in.push(extra);
                    }
                }
            }
        } else {
            index_by_key.insert(key, grouped.len());
            grouped.push(diag);
        }
    }
    grouped
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
        if !self.also_in.is_empty() {
            println!("  = {}: {}", "also in".dimmed(), self.also_in.join(", "));
        }
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
            let mut region = serde_json::json!({
                "startLine": d.line,
                "startColumn": d.column.unwrap_or(1),
            });
            if let Some(end_line) = d.end_line {
                region["endLine"] = serde_json::json!(end_line);
            }
            let mut properties = serde_json::json!({
                "confidence": d.confidence,
            });
            if !d.also_in.is_empty() {
                properties["alsoIn"] = serde_json::json!(d.also_in);
            }
            let mut result = serde_json::json!({
                "ruleId": d.rule_id,
                "level": match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Advisory => "note",
                },
                "message": { "text": d.message },
                "properties": properties,
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": d.file.display().to_string() },
                        "region": region,
                    }
                }]
            });
            if !d.also_in.is_empty() {
                let related: Vec<serde_json::Value> = d
                    .also_in
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| {
                        let (uri, start_line) = entry
                            .rsplit_once(':')
                            .and_then(|(path, line)| line.parse::<usize>().ok().map(|l| (path, l)))
                            .unwrap_or((entry.as_str(), 1));
                        serde_json::json!({
                            "id": index + 1,
                            "message": { "text": format!("Also reported at {entry}") },
                            "physicalLocation": {
                                "artifactLocation": { "uri": uri },
                                "region": {
                                    "startLine": start_line,
                                    "startColumn": 1,
                                }
                            }
                        })
                    })
                    .collect();
                result["relatedLocations"] = serde_json::json!(related);
            }
            result
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        group_diagnostics, known_rule_confidence, to_sarif, Confidence, Diagnostic, RuleCategory,
        Severity,
    };

    #[test]
    fn sarif_preserves_end_line_when_available() {
        let diagnostic = Diagnostic {
            rule_id: "SEC011".to_string(),
            rule_name: "unbounded-input".to_string(),
            category: RuleCategory::Semantic,
            severity: Severity::Warning,
            confidence: Confidence::Low,
            file: PathBuf::from("src/lib.rs"),
            line: 10,
            column: Some(4),
            end_line: Some(14),
            message: "example".to_string(),
            explanation: "example explanation".to_string(),
            suggestion: None,
            also_in: Vec::new(),
        };

        let sarif: serde_json::Value =
            serde_json::from_str(&to_sarif(&[diagnostic])).expect("SARIF must be valid JSON");
        let region = &sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startLine"], 10);
        assert_eq!(region["endLine"], 14);
        assert_eq!(
            sarif["runs"][0]["results"][0]["properties"]["confidence"],
            "low"
        );
    }

    #[test]
    fn group_diagnostics_merges_identical_findings_across_files() {
        let mk = |file: &str, line: usize, sev: Severity, conf: Confidence| Diagnostic {
            rule_id: "RTC007".to_string(),
            rule_name: "xcm-deny-barrier-not-recursive".to_string(),
            category: RuleCategory::RuntimeConfig,
            severity: sev,
            confidence: conf,
            file: PathBuf::from(file),
            line,
            column: None,
            end_line: None,
            message: "same message".to_string(),
            explanation: "why".to_string(),
            suggestion: None,
            also_in: Vec::new(),
        };
        let grouped = group_diagnostics(vec![
            mk(
                "runtimes/a/xcm.rs",
                10,
                Severity::Warning,
                Confidence::Medium,
            ),
            mk("runtimes/b/xcm.rs", 20, Severity::Error, Confidence::High),
            mk("runtimes/c/xcm.rs", 30, Severity::Warning, Confidence::Low),
        ]);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].file, PathBuf::from("runtimes/b/xcm.rs"));
        assert_eq!(grouped[0].line, 20);
        assert_eq!(grouped[0].severity, Severity::Error);
        assert_eq!(grouped[0].confidence, Confidence::High);
        assert_eq!(
            grouped[0].also_in,
            vec!["runtimes/a/xcm.rs:10", "runtimes/c/xcm.rs:30"]
        );

        let sarif: serde_json::Value =
            serde_json::from_str(&to_sarif(&grouped)).expect("SARIF must be valid JSON");
        let related = sarif["runs"][0]["results"][0]["relatedLocations"]
            .as_array()
            .expect("grouped SARIF must emit relatedLocations");
        assert_eq!(related.len(), 2);
        assert_eq!(
            related[0]["physicalLocation"]["artifactLocation"]["uri"],
            "runtimes/a/xcm.rs"
        );
        assert_eq!(related[0]["physicalLocation"]["region"]["startLine"], 10);
        assert_eq!(
            related[1]["physicalLocation"]["artifactLocation"]["uri"],
            "runtimes/c/xcm.rs"
        );
        assert_eq!(related[1]["physicalLocation"]["region"]["startLine"], 30);
    }

    #[test]
    fn every_registered_rule_has_an_explicit_confidence_mapping() {
        for rule_id in crate::rules::all_rule_ids() {
            assert!(
                known_rule_confidence(&rule_id).is_some(),
                "{rule_id} must be explicitly mapped in known_rule_confidence"
            );
        }
    }
}
