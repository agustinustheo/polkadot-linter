pub mod benchmarking;
pub mod mock_usage;
pub mod semantic;
pub mod terminology;
pub mod test_smells;

use crate::{config::Config, diagnostics::Diagnostic, engine::FileContext};

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
        // Security rules (from security review findings)
        Box::new(semantic::UnboundedVecInExtrinsic),
        Box::new(semantic::DebugAssertInProduction),
        Box::new(semantic::MissingDecodeDepthLimit),
        Box::new(semantic::UnsafeWeightArithmetic),
        Box::new(semantic::ExpensiveWeightCalculation),
        Box::new(semantic::UncheckedRepatriateReserved),
        Box::new(semantic::LetUnderscoreResult),
        Box::new(semantic::PanicInProduction),
        Box::new(semantic::RawArithmeticInFallible),
        Box::new(semantic::StorageWriteBeforeValidation),
        Box::new(semantic::MissingTransactionalInHook),
        Box::new(semantic::StorageIterationInDispatchables),
        Box::new(semantic::UnboundedClearPrefix),
        Box::new(semantic::UnboundedStorageCollections),
        Box::new(semantic::IdentityHasherOnCommonKeys),
        Box::new(semantic::DispatchBypassFilterInProduction),
        Box::new(semantic::MissingStorageVersionCheckInRuntimeUpgrade),
        Box::new(semantic::VecInEvents),
    ];

    // Filter disabled rules
    rules.retain(|r| config.rule_enabled(r.id()));

    rules
}
