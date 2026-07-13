pub mod benchmarking;
pub mod mock_usage;
pub mod semantic;
pub mod terminology;
pub mod test_smells;

use crate::{config::Config, diagnostics::Diagnostic, engine::FileContext};

/// Security rules whose public implementation is the rustc-backed driver.
///
/// They deliberately do not appear in [`all_rules`]. Keeping a `syn` rule
/// registered for the same ID would create a second, lower-fidelity authority
/// for a compiler-backed diagnostic.
pub const COMPILER_BACKED_SECURITY_RULE_IDS: &[&str] = &[
    "SEC001", "SEC002", "SEC003", "SEC004", "SEC005", "SEC006", "SEC007", "SEC008", "SEC009",
    "SEC010", "SEC011", "SEC012", "SEC013", "SEC014", "SEC015", "SEC016", "SEC017", "SEC018",
];

/// All public rules implemented by the rustc driver.
pub const COMPILER_BACKED_RULE_IDS: &[&str] = &[
    "VAL003", "SEC001", "SEC002", "SEC003", "SEC004", "SEC005", "SEC006", "SEC007", "SEC008",
    "SEC009", "SEC010", "SEC011", "SEC012", "SEC013", "SEC014", "SEC015", "SEC016", "SEC017",
    "SEC018",
];

/// Trait that all lint rules implement
pub trait LintRule: Send + Sync {
    /// Unique rule identifier (e.g., "VAL001")
    fn id(&self) -> &str;

    /// Human-readable rule name
    fn name(&self) -> &str;

    /// Rule family for filtering (e.g., "semantic", "test-smell")
    fn family(&self) -> &str;

    /// Check a file and return any diagnostics
    fn check(&self, ctx: &FileContext, config: &Config) -> Option<Vec<Diagnostic>>;
}

/// Return all registered lint rules
pub fn all_rules(config: &Config) -> Vec<Box<dyn LintRule>> {
    let mut rules: Vec<Box<dyn LintRule>> = vec![
        // Semantic rules
        Box::new(semantic::ValidationBeforeHeavyRead),
        Box::new(semantic::PreferCollectTurbofish),
        Box::new(semantic::PreferRefIteration),
        Box::new(semantic::NoWildcardImports),
        Box::new(semantic::ParameteriseWeightFunctions),
        Box::new(semantic::DbWeightMissingPov),
        Box::new(semantic::RuntimeDebugDeprecated),
        Box::new(semantic::SpStdDeprecated),
        Box::new(semantic::RedundantContainsKeyBeforeRemove),
        Box::new(semantic::XorAsExponentiation),
        Box::new(semantic::WeightZeroPlaceholder),
        Box::new(semantic::DivisionWithoutZeroGuard),
        Box::new(semantic::AllowDeadCodeInPallet),
        Box::new(semantic::CustomInvalidityReprU8),
        Box::new(semantic::SubmitTransactionLogTarget),
        Box::new(semantic::MissingWeightOfAuthorize),
        Box::new(semantic::MissingAuthorizeCallInCreateAuthorizedTransaction),
        // Test smell rules
        Box::new(test_smells::AssertNoop),
        Box::new(test_smells::ApplyExtrinsicAssertOk),
        Box::new(test_smells::ImportsInsideClosures),
        Box::new(test_smells::PaysYesErrorPath),
        Box::new(test_smells::ImplementationDetailAssertions),
        Box::new(test_smells::ExtrinsicWithoutEvent),
        // Mock usage rules
        Box::new(mock_usage::ExcessiveMockSetup),
        // Benchmarking rules
        Box::new(benchmarking::BenchmarkForWeightFunction),
        Box::new(benchmarking::BenchmarkVerification),
        Box::new(benchmarking::ExtrinsicWithoutBenchmark),
        // Terminology rules
        Box::new(terminology::SpellingConventions),
    ];

    // Filter disabled rules
    rules.retain(|r| config.rule_enabled(r.id()));

    rules
}
