use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

use crate::diagnostics::Severity;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: GeneralConfig,
    pub rules: RulesConfig,
    pub test_smells: TestSmellsConfig,
    pub terminology: TerminologyConfig,
    pub runtime_config: RuntimeConfigConfig,
    /// Sections of retired rule families, accepted so 0.1.0 configs still load.
    #[serde(skip_serializing)]
    validation_order: Option<toml::Value>,
    #[serde(skip_serializing)]
    mock_usage: Option<toml::Value>,
    #[serde(skip_serializing)]
    benchmarking: Option<toml::Value>,
}

/// Mirrors the complete bundled TOML schema without invoking `Config::default`
/// while serde is deserializing the defaults themselves.
#[derive(Deserialize)]
struct BundledConfig {
    general: GeneralConfig,
    rules: RulesConfig,
    test_smells: TestSmellsConfig,
    terminology: TerminologyConfig,
    runtime_config: RuntimeConfigConfig,
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
pub struct TestSmellsConfig {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeConfigConfig {
    /// Severity for every rule of this family. Unset by default, so each rule
    /// keeps its own default (RTC009 is error, the others warning).
    pub severity: Option<String>,
    /// Also lint runtimes listed in `dev_runtime_packages`
    pub include_dev_runtimes: bool,
    /// Cargo package names of development runtimes that are not meant for production
    pub dev_runtime_packages: Vec<String>,
    /// Package or crate-directory name tokens that mark a dev runtime (e.g.
    /// `peaq-dev`, `development-runtime`), skipped like `dev_runtime_packages`.
    pub dev_runtime_tokens: Vec<String>,
    /// Package or crate-directory name tokens (split on `-`/`_`) that mark a
    /// test runtime, which is always skipped. Set to `[]` to lint them.
    pub test_runtime_tokens: Vec<String>,
    /// Retired RTC001 keys, accepted so 0.2.0 configs still load.
    #[serde(skip_serializing)]
    unit_weight_info_allowlist: Option<toml::Value>,
    #[serde(skip_serializing)]
    flag_substrate_weight: Option<toml::Value>,
    #[serde(skip_serializing)]
    substrate_weight_severity: Option<toml::Value>,
}

impl Default for RuntimeConfigConfig {
    fn default() -> Self {
        RuntimeConfigConfig {
            severity: None,
            include_dev_runtimes: false,
            dev_runtime_packages: vec![
                "kitchensink-runtime".to_string(),
                "pallet-staking-async-parachain-runtime".to_string(),
                "pallet-staking-async-rc-runtime".to_string(),
                "penpal-runtime".to_string(),
                "rococo-parachain-runtime".to_string(),
                "yet-another-parachain-runtime".to_string(),
            ],
            dev_runtime_tokens: vec!["dev".to_string(), "development".to_string()],
            test_runtime_tokens: vec![
                "test".to_string(),
                "tests".to_string(),
                "mock".to_string(),
                "mocks".to_string(),
            ],
            unit_weight_info_allowlist: None,
            flag_substrate_weight: None,
            substrate_weight_severity: None,
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            exclude: vec!["target/**".to_string(), ".git/**".to_string()],
            include: vec![],
        }
    }
}

impl Default for TestSmellsConfig {
    fn default() -> Self {
        TestSmellsConfig {
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
        let mut config: Config = toml::from_str(&content)?;
        for (section, value) in [
            ("validation_order", &config.validation_order),
            ("mock_usage", &config.mock_usage),
            ("benchmarking", &config.benchmarking),
        ] {
            if value.is_some() {
                eprintln!("warning: config section [{section}] is no longer used: its rules were retired, see docs/retired-rules.md");
            }
        }
        let runtime = &config.runtime_config;
        for (key, value) in [
            (
                "unit_weight_info_allowlist",
                &runtime.unit_weight_info_allowlist,
            ),
            ("flag_substrate_weight", &runtime.flag_substrate_weight),
            (
                "substrate_weight_severity",
                &runtime.substrate_weight_severity,
            ),
        ] {
            if value.is_some() {
                eprintln!("warning: config key runtime_config.{key} is no longer used: RTC001 reports only TestWeightInfo, see docs/retired-rules.md");
            }
        }
        // A `[rules]` table overrides the bundled per-rule entries instead of
        // replacing them, so default-off rules stay off unless re-enabled.
        let defaults = Config::default().rules;
        for (rule_id, enabled) in defaults.enabled {
            config.rules.enabled.entry(rule_id).or_insert(enabled);
        }
        for (rule_id, severity) in defaults.severity {
            config.rules.severity.entry(rule_id).or_insert(severity);
        }
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
            ("test_smells.severity", &self.test_smells.severity),
            ("terminology.severity", &self.terminology.severity),
        ] {
            self.validate_severity(section, severity)?;
        }
        if let Some(severity) = &self.runtime_config.severity {
            self.validate_severity("runtime_config.severity", severity)?;
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
            Some("TST") => &self.test_smells.severity,
            Some("TRM") => &self.terminology.severity,
            Some("RTC") => self.runtime_config.severity.as_ref()?,
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
            test_smells: defaults.test_smells,
            terminology: defaults.terminology,
            runtime_config: defaults.runtime_config,
            validation_order: None,
            mock_usage: None,
            benchmarking: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_rules_tables_override_bundled_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("polkadot-linter.toml");
        std::fs::write(
            &path,
            "[rules.enabled]\nSEC004 = true\n[rules.severity]\nSEC011 = \"error\"\n",
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert!(config.rule_enabled("SEC004"));
        assert!(!config.rule_enabled("RTC004"));
        assert_eq!(
            config.rules.severity.get("TST002"),
            Some(&"error".to_string())
        );
        assert_eq!(
            config.rules.severity.get("SEC011"),
            Some(&"error".to_string())
        );
    }

    #[test]
    fn retired_family_sections_from_older_configs_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("polkadot-linter.toml");
        std::fs::write(
            &path,
            "[validation_order]\nseverity = \"warning\"\nheavy_operations = [\"::get(\"]\n\n\
             [mock_usage]\nmax_mock_ratio = 3.0\n\n\
             [benchmarking]\nverification_patterns = [\"verify\"]\n\n\
             [runtime_config]\nunit_weight_info_allowlist = []\nflag_substrate_weight = false\n",
        )
        .unwrap();
        let config = Config::load(&path).expect("retired sections are ignored, not rejected");
        assert!(config.rule_enabled("SEC012"));
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
    fn per_rule_severity_overrides_family_severity() {
        let mut config = Config::default();
        config.test_smells.severity = "error".to_string();
        config
            .rules
            .severity
            .insert("TST002".to_string(), "advisory".to_string());

        assert_eq!(
            config.rule_severity("TST002", Severity::Warning),
            Severity::Advisory
        );
        assert_eq!(
            config.rule_severity("TST999", Severity::Advisory),
            Severity::Error
        );
    }

    #[test]
    fn minimal_user_config_keeps_per_rule_runtime_config_defaults() {
        let config: Config =
            toml::from_str("[rules]\n").expect("minimal configuration must deserialize");
        config
            .validate()
            .expect("minimal configuration must validate");
        assert_eq!(
            config.rule_severity("RTC003", Severity::Error),
            Severity::Error
        );
        assert_eq!(
            config.rule_severity("RTC004", Severity::Warning),
            Severity::Warning
        );

        let config: Config = toml::from_str("[runtime_config]\nseverity = \"advisory\"\n")
            .expect("family severity must deserialize");
        assert_eq!(
            config.rule_severity("RTC003", Severity::Error),
            Severity::Advisory
        );
    }

    #[test]
    fn validation_rejects_invalid_globs_and_severities() {
        let mut config = Config::default();
        config.general.exclude = vec!["[".to_string()];
        assert!(config.validate().unwrap_err().contains("invalid glob"));

        config.general.exclude.clear();
        config.test_smells.severity = "urgent".to_string();
        assert!(config.validate().unwrap_err().contains("invalid severity"));
    }
}
