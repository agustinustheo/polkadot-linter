use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

use crate::diagnostics::Severity;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub rules: RulesConfig,
    pub validation_order: ValidationOrderConfig,
    pub test_smells: TestSmellsConfig,
    pub mock_usage: MockUsageConfig,
    pub benchmarking: BenchmarkingConfig,
    pub terminology: TerminologyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Paths to exclude from scanning
    pub exclude: Vec<String>,
    /// Paths to include (if empty, include everything)
    pub include: Vec<String>,
    /// Default severity for new rules
    pub default_severity: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    /// Map of rule_id -> enabled/disabled
    pub enabled: HashMap<String, bool>,
    /// Map of rule_id -> severity override
    pub severity: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidationOrderConfig {
    /// Known expensive/heavy operations (function name patterns)
    pub heavy_operations: Vec<String>,
    /// Known cheap validation operations
    pub cheap_validations: Vec<String>,
    /// Severity for this rule family
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TestSmellsConfig {
    /// Patterns indicating internal field access in assertions
    pub internal_field_patterns: Vec<String>,
    /// Maximum ratio of setup lines to assertion lines before warning
    pub max_setup_ratio: f64,
    /// Severity for this rule family
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
#[serde(default)]
pub struct BenchmarkingConfig {
    /// Paths that are benchmark-sensitive
    pub sensitive_paths: Vec<String>,
    /// Expected benchmark verification patterns
    pub verification_patterns: Vec<String>,
    /// Dispatchable attribute patterns
    pub dispatchable_patterns: Vec<String>,
    /// Severity for this rule family
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
            default_severity: "warning".to_string(),
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
                r"\.0\b".to_string(),  // tuple field access
                r"\._\w+".to_string(), // underscore-prefixed private fields
                r"\.inner\b".to_string(),
                r"\.state\b".to_string(),
                r"\.cache\b".to_string(),
                r"\.counter\b".to_string(),
                r"\.buffer\b".to_string(),
                r"\.flag\b".to_string(),
            ],
            max_setup_ratio: 5.0,
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
            sensitive_paths: vec![
                "pallets/*/src/lib.rs".to_string(),
                "pallets/*/src/weights.rs".to_string(),
                "runtime/src/**".to_string(),
            ],
            verification_patterns: vec![
                "verify".to_string(),
                "assert_last_event".to_string(),
                "assert_has_event".to_string(),
            ],
            dispatchable_patterns: vec![
                "#[pallet::call_index".to_string(),
                "#[pallet::call]".to_string(),
                "pub fn ".to_string(),
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
        Ok(config)
    }

    pub fn rule_enabled(&self, rule_id: &str) -> bool {
        self.rules.enabled.get(rule_id).copied().unwrap_or(true)
    }

    pub fn rule_severity(&self, rule_id: &str, default: Severity) -> Severity {
        self.rules
            .severity
            .get(rule_id)
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    }
}
