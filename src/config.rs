use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

use crate::diagnostics::Severity;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: GeneralConfig,
    pub rules: RulesConfig,
    pub validation_order: ValidationOrderConfig,
    pub test_smells: TestSmellsConfig,
    pub mock_usage: MockUsageConfig,
    pub benchmarking: BenchmarkingConfig,
    pub terminology: TerminologyConfig,
}

/// Mirrors the complete bundled TOML schema without invoking `Config::default`
/// while serde is deserializing the defaults themselves.
#[derive(Deserialize)]
struct BundledConfig {
    general: GeneralConfig,
    rules: RulesConfig,
    validation_order: ValidationOrderConfig,
    test_smells: TestSmellsConfig,
    mock_usage: MockUsageConfig,
    benchmarking: BenchmarkingConfig,
    terminology: TerminologyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GeneralConfig {
    /// Paths to exclude from scanning
    pub exclude: Vec<String>,
    /// Paths to include (if empty, include everything)
    pub include: Vec<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RulesConfig {
    /// Map of rule_id -> enabled/disabled
    pub enabled: HashMap<String, bool>,
    /// Map of rule_id -> severity override
    pub severity: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ValidationOrderConfig {
    /// Known expensive/heavy operations (function name patterns)
    pub heavy_operations: Vec<String>,
    /// Known cheap validation operations
    pub cheap_validations: Vec<String>,
    /// Severity for this rule family
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TestSmellsConfig {
    /// Patterns indicating internal field access in assertions
    pub internal_field_patterns: Vec<String>,
    /// Severity for this rule family
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MockUsageConfig {
    /// Mock-related patterns to detect
    pub mock_patterns: Vec<String>,
    /// Maximum number of mock expectations per test before warning
    pub max_mock_expectations: usize,
    /// Maximum ratio of mock setup to actual assertions
    pub max_mock_ratio: f64,
    /// Severity for this rule family
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BenchmarkingConfig {
    /// Expected benchmark verification patterns
    pub verification_patterns: Vec<String>,
    /// Severity for this rule family
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerminologyConfig {
    /// British English preferred spellings: american -> british
    pub british_english: HashMap<String, String>,
    /// Project-specific forbidden terms: forbidden -> preferred
    pub forbidden_terms: HashMap<String, String>,
    /// Whether to check identifiers (not just comments/docs)
    pub check_identifiers: bool,
    /// Whether to check string literals
    pub check_strings: bool,
    /// Severity for this rule family
    pub severity: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            exclude: vec!["target/**".to_string(), ".git/**".to_string()],
            include: vec![],
        }
    }
}

impl Default for ValidationOrderConfig {
    fn default() -> Self {
        ValidationOrderConfig {
            heavy_operations: vec![
                // Polkadot SDK storage reads
                "::get(".to_string(),
                "::try_get(".to_string(),
                "::iter(".to_string(),
                "::iter_prefix(".to_string(),
                "::iter_keys(".to_string(),
                "::contains_key(".to_string(),
                "::decode_len(".to_string(),
                "::count(".to_string(),
                // FRAME storage
                "StorageValue::get".to_string(),
                "StorageMap::get".to_string(),
                "StorageDoubleMap::get".to_string(),
                "StorageNMap::get".to_string(),
                "CountedStorageMap::get".to_string(),
                // Weight/computation
                "T::DbWeight::get()".to_string(),
            ],
            cheap_validations: vec![
                "ensure!".to_string(),
                "ensure_signed".to_string(),
                "ensure_root".to_string(),
                "ensure_none".to_string(),
                ".is_empty()".to_string(),
                ".is_none()".to_string(),
                ".is_some()".to_string(),
                ".is_zero()".to_string(),
                "== 0".to_string(),
                "!= 0".to_string(),
                ".len()".to_string(),
            ],
            severity: "warning".to_string(),
        }
    }
}

impl Default for TestSmellsConfig {
    fn default() -> Self {
        TestSmellsConfig {
            internal_field_patterns: vec![
                r"\._\w+".to_string(), // underscore-prefixed private fields
                r"\.inner\b".to_string(),
                r"\.state\b".to_string(),
                r"\.cache\b".to_string(),
                r"\.counter\b".to_string(),
                r"\.buffer\b".to_string(),
                r"\.flag\b".to_string(),
            ],
            severity: "warning".to_string(),
        }
    }
}

impl Default for MockUsageConfig {
    fn default() -> Self {
        MockUsageConfig {
            mock_patterns: vec![
                "mock".to_string(),
                "Mock".to_string(),
                "MOCK".to_string(),
                "MockBuilder".to_string(),
                "with_mock".to_string(),
                "new_test_ext".to_string(),
            ],
            max_mock_expectations: 10,
            max_mock_ratio: 3.0,
            severity: "warning".to_string(),
        }
    }
}

impl Default for BenchmarkingConfig {
    fn default() -> Self {
        BenchmarkingConfig {
            verification_patterns: vec![
                "verify".to_string(),
                "assert_last_event".to_string(),
                "assert_has_event".to_string(),
            ],
            severity: "warning".to_string(),
        }
    }
}

impl Default for TerminologyConfig {
    fn default() -> Self {
        // Default spelling map is empty — configure in polkadot-linter.toml.
        // The project style guide specifies "Google English" which should be
        // customised per-project. Uncomment entries in the config file.
        let british = HashMap::new();

        let forbidden = HashMap::new();

        TerminologyConfig {
            british_english: british,
            forbidden_terms: forbidden,
            check_identifiers: false,
            check_strings: true,
            severity: "advisory".to_string(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Reject configuration that would otherwise silently change scan coverage.
    pub fn validate(&self) -> Result<(), String> {
        for pattern in self.general.exclude.iter().chain(&self.general.include) {
            glob::Pattern::new(pattern)
                .map_err(|error| format!("invalid glob pattern `{pattern}`: {error}"))?;
        }

        for (rule_id, severity) in &self.rules.severity {
            self.validate_severity(&format!("rules.severity.{rule_id}"), severity)?;
        }
        for (section, severity) in [
            ("validation_order.severity", &self.validation_order.severity),
            ("test_smells.severity", &self.test_smells.severity),
            ("mock_usage.severity", &self.mock_usage.severity),
            ("benchmarking.severity", &self.benchmarking.severity),
            ("terminology.severity", &self.terminology.severity),
        ] {
            self.validate_severity(section, severity)?;
        }

        Ok(())
    }

    fn validate_severity(&self, location: &str, value: &str) -> Result<(), String> {
        value.parse::<Severity>().map(|_| ()).map_err(|_| {
            format!(
                "invalid severity `{value}` for {location}; expected advisory, warning, or error"
            )
        })
    }

    pub fn rule_enabled(&self, rule_id: &str) -> bool {
        self.rules.enabled.get(rule_id).copied().unwrap_or(true)
    }

    pub fn rule_severity(&self, rule_id: &str, default: Severity) -> Severity {
        self.rules
            .severity
            .get(rule_id)
            .and_then(|s| s.parse().ok())
            .or_else(|| self.family_severity(rule_id))
            .unwrap_or(default)
    }

    fn family_severity(&self, rule_id: &str) -> Option<Severity> {
        let severity = match rule_id.get(..3) {
            Some("VAL") => &self.validation_order.severity,
            Some("TST") => &self.test_smells.severity,
            Some("MOK") => &self.mock_usage.severity,
            Some("BEN") => &self.benchmarking.severity,
            Some("TRM") => &self.terminology.severity,
            _ => return None,
        };
        severity.parse().ok()
    }
}

impl Default for Config {
    fn default() -> Self {
        let defaults: BundledConfig = toml::from_str(include_str!("../config/default.toml"))
            .expect("bundled default configuration must be valid");
        Self {
            general: defaults.general,
            rules: defaults.rules,
            validation_order: defaults.validation_order,
            test_smells: defaults.test_smells,
            mock_usage: defaults.mock_usage,
            benchmarking: defaults.benchmarking,
            terminology: defaults.terminology,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_default_configuration_matches_config_default_toml() {
        let config = Config::default();
        assert_eq!(
            config.rules.severity.get("TST002"),
            Some(&"error".to_string())
        );
        assert_eq!(config.general.exclude.first(), Some(&"*.lock".to_string()));
    }

    #[test]
    fn project_configuration_uses_the_supported_schema() {
        let config: Config = toml::from_str(include_str!("../polkadot-linter.toml"))
            .expect("repository sample configuration must use supported options");
        config
            .validate()
            .expect("repository sample configuration must validate");
    }

    #[test]
    fn deserialization_rejects_unknown_configuration_options() {
        let error = toml::from_str::<Config>("[general]\nmax_setup_ratio = 5.0\n")
            .expect_err("unknown settings must not be silently ignored");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn partial_test_smell_config_does_not_match_float_literals() {
        let config: Config = toml::from_str(
            r#"
[test_smells]
severity = "warning"
"#,
        )
        .expect("partial test smell configuration must deserialize");

        assert!(
            !config
                .test_smells
                .internal_field_patterns
                .iter()
                .any(|pattern| pattern == r"\.0\b"),
            "float literals must not be treated as tuple-field assertions"
        );
    }

    #[test]
    fn per_rule_severity_overrides_family_severity() {
        let mut config = Config::default();
        config.test_smells.severity = "error".to_string();
        config
            .rules
            .severity
            .insert("TST001".to_string(), "advisory".to_string());

        assert_eq!(
            config.rule_severity("TST001", Severity::Warning),
            Severity::Advisory
        );
        assert_eq!(
            config.rule_severity("TST003", Severity::Advisory),
            Severity::Error
        );
    }

    #[test]
    fn validation_rejects_invalid_globs_and_severities() {
        let mut config = Config::default();
        config.general.exclude = vec!["[".to_string()];
        assert!(config.validate().unwrap_err().contains("invalid glob"));

        config.general.exclude.clear();
        config.benchmarking.severity = "urgent".to_string();
        assert!(config.validate().unwrap_err().contains("invalid severity"));
    }
}
