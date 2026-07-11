use std::path::PathBuf;

fn fixture_is_test_file(path: &str) -> bool {
    path.starts_with("tests/")
        || path.starts_with("test/")
        || path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("/mocks/")
        || path.contains("/mock/")
        || path.contains("/fuzzer/")
        || path.contains("integration_tests")
        || path.contains("integration-tests")
        || path.ends_with("_test.rs")
        || path.ends_with("_tests.rs")
        || path.ends_with("tests.rs")
        || path.ends_with("test.rs")
        || path.ends_with("mock_message_queue.rs")
        || path.ends_with("mock.rs")
        || path.ends_with("testing_utils.rs")
}

/// Helper: create a FileContext and run a specific rule against fixture content.
fn check_fixture(filename: &str, content: &str) -> Vec<polkadot_linter::diagnostics::Diagnostic> {
    let config = polkadot_linter::config::Config::default();
    let path = PathBuf::from(filename);
    let rel_path = PathBuf::from(filename);
    let is_rust = filename.ends_with(".rs");
    let is_text = !is_rust;

    let ctx = polkadot_linter::engine::FileContext {
        path: path.clone(),
        rel_path,
        content,
        is_rust,
        is_text,
        is_test_file: fixture_is_test_file(filename),
        is_benchmark_file: filename.contains("benchmarking"),
        source_target_kinds: Vec::new(),
        ast: if is_rust {
            syn::parse_file(content).ok()
        } else {
            None
        },
    };

    let rules = polkadot_linter::rules::all_rules(&config);
    let mut diags = Vec::new();
    for rule in &rules {
        if let Some(mut d) = rule.check(&ctx, &config) {
            diags.append(&mut d);
        }
    }
    diags
}

fn check_fixture_with_config(
    filename: &str,
    content: &str,
    config: &polkadot_linter::config::Config,
) -> Vec<polkadot_linter::diagnostics::Diagnostic> {
    let path = PathBuf::from(filename);
    let rel_path = PathBuf::from(filename);
    let is_rust = filename.ends_with(".rs");

    let ctx = polkadot_linter::engine::FileContext {
        path: path.clone(),
        rel_path,
        content,
        is_rust,
        is_text: !is_rust,
        is_test_file: fixture_is_test_file(filename),
        is_benchmark_file: filename.contains("benchmarking"),
        source_target_kinds: Vec::new(),
        ast: if is_rust {
            syn::parse_file(content).ok()
        } else {
            None
        },
    };

    let rules = polkadot_linter::rules::all_rules(config);
    let mut diags = Vec::new();
    for rule in &rules {
        if let Some(mut d) = rule.check(&ctx, config) {
            diags.append(&mut d);
        }
    }
    diags
}

fn check_fixture_path(
    path: PathBuf,
    content: &str,
) -> Vec<polkadot_linter::diagnostics::Diagnostic> {
    let config = polkadot_linter::config::Config::default();
    let path_str = path.to_string_lossy().to_string();
    let is_rust = path_str.ends_with(".rs");
    let ctx = polkadot_linter::engine::FileContext {
        path: path.clone(),
        rel_path: path.clone(),
        content,
        is_rust,
        is_text: !is_rust,
        is_test_file: fixture_is_test_file(&path_str),
        is_benchmark_file: path_str.contains("benchmarking"),
        source_target_kinds: Vec::new(),
        ast: if is_rust {
            syn::parse_file(content).ok()
        } else {
            None
        },
    };

    let rules = polkadot_linter::rules::all_rules(&config);
    let mut diags = Vec::new();
    for rule in &rules {
        if let Some(mut d) = rule.check(&ctx, &config) {
            diags.append(&mut d);
        }
    }
    diags
}

fn has_rule(diags: &[polkadot_linter::diagnostics::Diagnostic], rule_id: &str) -> bool {
    diags.iter().any(|d| d.rule_id == rule_id)
}

// ==========================================================================
// VAL001: Validation before heavy reads
// ==========================================================================
#[test]
fn val001_detects_heavy_read_before_validation() {
    let bad = include_str!("fixtures/bad_val001.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "VAL001"),
        "VAL001 should fire on bad fixture. Got: {:?}",
        diags.iter().map(|d| &d.rule_id).collect::<Vec<_>>()
    );
}

#[test]
fn val001_allows_validation_before_heavy_read() {
    let good = include_str!("fixtures/good_val001.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "VAL001"),
        "VAL001 should NOT fire on good fixture. Got: {:?}",
        diags
            .iter()
            .filter(|d| d.rule_id == "VAL001")
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn val001_skips_benchmark_files() {
    let bad = include_str!("fixtures/bad_val001.rs");
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", bad);
    assert!(
        !has_rule(&diags, "VAL001"),
        "VAL001 should skip benchmark files"
    );
}

// ==========================================================================
// SEM002: Prefer collect turbofish
// ==========================================================================
#[test]
fn sem002_detects_typed_collect() {
    let bad = include_str!("fixtures/bad_sem002.rs");
    let diags = check_fixture("src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM002"),
        "SEM002 should fire on bad fixture"
    );
}

#[test]
fn sem002_allows_turbofish_collect() {
    let good = include_str!("fixtures/good_sem002.rs");
    let diags = check_fixture("src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM002"),
        "SEM002 should NOT fire on good fixture"
    );
}

// ==========================================================================
// SEM003: Prefer reference iteration
// ==========================================================================
#[test]
fn sem003_detects_iter_pattern() {
    let bad = include_str!("fixtures/bad_sem003.rs");
    let diags = check_fixture("src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM003"),
        "SEM003 should fire on bad fixture"
    );
}

#[test]
fn sem003_allows_ref_iteration() {
    let good = include_str!("fixtures/good_sem003.rs");
    let diags = check_fixture("src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM003"),
        "SEM003 should NOT fire on good fixture"
    );
}

#[test]
fn sem003_skips_send_tx_utilities() {
    let bad = include_str!("fixtures/bad_sem003.rs");
    let diags = check_fixture("tools/send-tx/src/main.rs", bad);
    assert!(
        !has_rule(&diags, "SEM003"),
        "SEM003 should skip send-tx utilities that iterate over library types without IntoIterator"
    );
}

// ==========================================================================
// SEM004: No wildcard imports
// ==========================================================================
#[test]
fn sem004_detects_wildcard_import() {
    let bad = include_str!("fixtures/bad_sem004.rs");
    let diags = check_fixture("src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM004"),
        "SEM004 should fire on bad fixture"
    );
}

#[test]
fn sem004_allows_test_wildcards() {
    let good = include_str!("fixtures/good_sem004.rs");
    let diags = check_fixture("src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM004"),
        "SEM004 should NOT fire on good fixture"
    );
}

#[test]
fn sem004_skips_benchmark_files() {
    let bad = include_str!("fixtures/bad_sem004.rs");
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", bad);
    assert!(
        !has_rule(&diags, "SEM004"),
        "SEM004 should skip benchmark files"
    );
}

#[test]
fn sem004_allows_public_reexports() {
    let good = r#"
pub use pallet::*;
pub use types::*;
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM004"),
        "SEM004 should NOT fire on public wildcard re-exports"
    );
}

#[test]
fn sem004_allows_nested_prelude_globs() {
    let good = r#"
use frame_system::pallet_prelude::{BlockNumberFor, *};
use xcm::latest::prelude::{Junction::*, Location, NetworkId};
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM004"),
        "SEM004 should NOT fire on standard prelude wildcard imports"
    );
}

// ==========================================================================
// SEM005: Parameterised weight functions
// ==========================================================================
#[test]
fn sem005_detects_weight_multiplication() {
    let bad = include_str!("fixtures/bad_sem005.rs");
    let diags = check_fixture("src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM005"),
        "SEM005 should fire on bad fixture"
    );
}

#[test]
fn sem005_allows_parameterised_weight() {
    let good = include_str!("fixtures/good_sem005.rs");
    let diags = check_fixture("src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM005"),
        "SEM005 should NOT fire on good fixture"
    );
}

#[test]
fn sem005_detects_weight_multiplication_outside_weight_attr() {
    let bad = r#"
pub fn replay_missing_roots_worst_case_weight<T: Config>(chunks: u32) -> Weight {
    T::WeightInfo::send_replay_request().saturating_mul(chunks.into())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM005"),
        "SEM005 should fire when a zero-arg WeightInfo call is multiplied in normal code"
    );
}

// ==========================================================================
// TST001: Prefer assert_noop
// ==========================================================================
#[test]
fn tst001_detects_manual_error_checking() {
    let bad = include_str!("fixtures/bad_tst001.rs");
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST001"),
        "TST001 should fire on bad fixture"
    );
}

#[test]
fn tst001_allows_assert_noop() {
    let good = include_str!("fixtures/good_tst001.rs");
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST001"),
        "TST001 should NOT fire on good fixture"
    );
}

#[test]
fn tst001_detects_unwrap_err_inside_assert_macro() {
    let bad = r#"
#[test]
fn manual_error_assertion() {
    let result = call();
    assert!(result.is_err(), "should fail");
    assert_eq!(result.unwrap_err(), Error::<Test>::Boom.into());
}
"#;
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST001"),
        "TST001 should fire when unwrap_err is used inside another assertion macro"
    );
}

// ==========================================================================
// TST002: apply_extrinsic assert_ok
// ==========================================================================
#[test]
fn tst002_detects_assert_ok_apply_extrinsic() {
    let bad = include_str!("fixtures/bad_tst002.rs");
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST002"),
        "TST002 should fire on bad fixture"
    );
}

#[test]
fn tst002_allows_proper_nested_check() {
    let good = include_str!("fixtures/good_tst002.rs");
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST002"),
        "TST002 should NOT fire on good fixture"
    );
}

// ==========================================================================
// TST003: Imports inside closures
// ==========================================================================
#[test]
fn tst003_detects_imports_inside_closures() {
    let bad = include_str!("fixtures/bad_tst003.rs");
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST003"),
        "TST003 should fire on bad fixture"
    );
}

#[test]
fn tst003_allows_module_level_imports() {
    let good = include_str!("fixtures/good_tst003.rs");
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST003"),
        "TST003 should NOT fire on good fixture"
    );
}

#[test]
fn tst003_ignores_top_level_test_function_imports() {
    let code = r#"
#[test]
fn helper_heavy_test() {
    use crate::mock::Test;
    let _ = core::mem::size_of::<Test>();
}
"#;
    let diags = check_fixture("tests/test.rs", code);
    assert!(
        !has_rule(&diags, "TST003"),
        "TST003 should ignore imports at the top level of a test function body"
    );
}

// ==========================================================================
// TST004: Pays::Yes error path
// ==========================================================================
#[test]
fn tst004_detects_pays_no_without_companion_test() {
    let bad = include_str!("fixtures/bad_tst004.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "TST004"),
        "TST004 should fire when Pays::No has no companion test. Got: {:?}",
        diags.iter().map(|d| &d.rule_id).collect::<Vec<_>>()
    );
}

#[test]
fn tst004_works_with_inline_cfg_test() {
    // A lib.rs that has Pays::No AND an inline #[cfg(test)] module should still be checked
    let code = r#"
pub fn do_something() -> DispatchResultWithPostInfo {
    Ok(Pays::No.into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(1, 1);
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "TST004"),
        "TST004 should fire even with inline #[cfg(test)] module"
    );
}

// ==========================================================================
// TST005: Implementation detail assertions
// ==========================================================================
#[test]
fn tst005_detects_internal_field_assertions() {
    let bad = include_str!("fixtures/bad_tst005.rs");
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST005"),
        "TST005 should fire on bad fixture"
    );
}

#[test]
fn tst005_allows_observable_assertions() {
    let good = include_str!("fixtures/good_tst005.rs");
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST005"),
        "TST005 should NOT fire on good fixture"
    );
}

// ==========================================================================
// BEN002: Benchmark verification
// ==========================================================================
#[test]
fn ben002_detects_missing_verify() {
    let bad = include_str!("fixtures/bad_ben002.rs");
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", bad);
    assert!(
        has_rule(&diags, "BEN002"),
        "BEN002 should fire on bad fixture"
    );
}

#[test]
fn ben002_allows_benchmark_with_verify() {
    let good = include_str!("fixtures/good_ben002.rs");
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", good);
    assert!(
        !has_rule(&diags, "BEN002"),
        "BEN002 should NOT fire on good fixture"
    );
}

#[test]
fn ben002_does_not_treat_string_literals_as_assertions() {
    let code = r#"
#[benchmark]
fn noop() {
    let _note = "assert_eq!(1, 1)";
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    assert!(
		has_rule(&diags, "BEN002"),
		"BEN002 should still fire when benchmark only mentions assertion macros inside a string literal"
	);
}

#[test]
fn ben002_allows_ensure_postconditions_outside_measured_block() {
    let code = r#"
#[benchmark]
fn rename_sub() -> Result<(), BenchmarkError> {
    #[extrinsic_call]
    _(RawOrigin::Signed(caller), value);

    ensure!(SomeMap::<T>::contains_key(value), "value not written");
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    assert!(
        !has_rule(&diags, "BEN002"),
        "BEN002 should accept ensure! postconditions outside the measured block"
    );
}

#[test]
fn ben002_does_not_treat_assertions_inside_block_as_verification() {
    let code = r#"
#[benchmark]
fn authorize_only() -> Result<(), BenchmarkError> {
    #[block]
    {
        assert_ok!(Pallet::<T>::authorize_call());
    }

    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    assert!(
        has_rule(&diags, "BEN002"),
        "BEN002 should still require a postcondition outside #[block]"
    );
}

#[test]
fn ben001_detects_weight_function_without_matching_benchmark() {
    let root = std::env::temp_dir().join(format!(
        "polkadot-linter-ben001-missing-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pallet_dir = root.join("pallets/foo/src");
    std::fs::create_dir_all(&pallet_dir).unwrap();

    let weights_path = pallet_dir.join("weights.rs");
    let bench_path = pallet_dir.join("benchmarking.rs");
    let weights = r#"
pub trait WeightInfo {
    fn submit() -> Weight;
    fn prune() -> Weight;
}
"#;
    let benches = r#"
#[benchmark]
fn submit() {}
"#;

    std::fs::write(&weights_path, weights).unwrap();
    std::fs::write(&bench_path, benches).unwrap();

    let diags = check_fixture_path(weights_path, weights);
    assert!(
        has_rule(&diags, "BEN001"),
        "BEN001 should fire when a weight function is missing a benchmark"
    );
}

#[test]
fn ben001_allows_matching_weight_function_benchmarks() {
    let root = std::env::temp_dir().join(format!(
        "polkadot-linter-ben001-match-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pallet_dir = root.join("pallets/foo/src");
    std::fs::create_dir_all(&pallet_dir).unwrap();

    let weights_path = pallet_dir.join("weights.rs");
    let bench_path = pallet_dir.join("benchmarking.rs");
    let weights = r#"
pub trait WeightInfo {
    fn submit() -> Weight;
    fn prune() -> Weight;
}
"#;
    let benches = r#"
#[benchmark]
fn submit() {}

#[benchmark]
fn prune() {}
"#;

    std::fs::write(&weights_path, weights).unwrap();
    std::fs::write(&bench_path, benches).unwrap();

    let diags = check_fixture_path(weights_path, weights);
    assert!(
        !has_rule(&diags, "BEN001"),
        "BEN001 should not fire when each weight function has a benchmark"
    );
}

// ==========================================================================
// TRM001: Spelling conventions
// ==========================================================================
#[test]
fn trm001_detects_non_standard_spelling() {
    let mut config = polkadot_linter::config::Config::default();
    config
        .terminology
        .british_english
        .insert("optimisation".to_string(), "optimization".to_string());

    let bad = include_str!("fixtures/bad_trm001.rs");
    let diags = check_fixture_with_config("src/lib.rs", bad, &config);
    assert!(
        has_rule(&diags, "TRM001"),
        "TRM001 should fire on bad fixture with configured dictionary"
    );
}

#[test]
fn trm001_allows_standard_spelling() {
    let mut config = polkadot_linter::config::Config::default();
    config
        .terminology
        .british_english
        .insert("optimisation".to_string(), "optimization".to_string());

    let good = include_str!("fixtures/good_trm001.rs");
    let diags = check_fixture_with_config("src/lib.rs", good, &config);
    assert!(
        !has_rule(&diags, "TRM001"),
        "TRM001 should NOT fire on good fixture"
    );
}

// ==========================================================================
// SEM006: DbWeight missing proof size
// ==========================================================================
#[test]
fn sem006_detects_dbweight_reads() {
    let bad = include_str!("fixtures/bad_sem006.rs");
    // Must NOT be a weights.rs path (those are excluded)
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM006"),
        "SEM006 should fire on DbWeight::get().reads() in lib.rs"
    );
}

#[test]
fn sem006_allows_benchmarked_weight() {
    let good = include_str!("fixtures/good_sem006.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM006"),
        "SEM006 should NOT fire on benchmarked weights"
    );
}

#[test]
fn sem006_skips_weights_files() {
    let bad = include_str!("fixtures/bad_sem006.rs");
    let diags = check_fixture("pallets/foo/src/weights.rs", bad);
    assert!(
        !has_rule(&diags, "SEM006"),
        "SEM006 should skip auto-generated weights.rs"
    );
}

// ==========================================================================
// SEM007: RuntimeDebug deprecated
// ==========================================================================
#[test]
fn sem007_detects_runtime_debug() {
    let bad = include_str!("fixtures/bad_sem007.rs");
    let diags = check_fixture("pallets/foo/src/types.rs", bad);
    assert!(
        has_rule(&diags, "SEM007"),
        "SEM007 should fire on RuntimeDebug usage"
    );
}

#[test]
fn sem007_allows_debug() {
    let good = include_str!("fixtures/good_sem007.rs");
    let diags = check_fixture("pallets/foo/src/types.rs", good);
    assert!(
        !has_rule(&diags, "SEM007"),
        "SEM007 should NOT fire on Debug usage"
    );
}

// ==========================================================================
// SEM008: sp_std deprecated
// ==========================================================================
#[test]
fn sem008_detects_sp_std() {
    let bad = include_str!("fixtures/bad_sem008.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM008"),
        "SEM008 should fire on sp_std usage"
    );
}

#[test]
fn sem008_allows_alloc() {
    let good = include_str!("fixtures/good_sem008.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM008"),
        "SEM008 should NOT fire on alloc usage"
    );
}

#[test]
fn sem008_detects_grouped_sp_std_usage() {
    let bad = r#"
use sp_std::{vec, vec::Vec};

fn build() -> Vec<u32> {
    sp_std::vec![1, 2, 3]
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM008"),
        "SEM008 should fire on grouped sp_std imports and macro usage"
    );
}

// ==========================================================================
// SEM009: Redundant contains_key before remove
// ==========================================================================
#[test]
fn sem009_detects_contains_key_before_remove() {
    let bad = include_str!("fixtures/bad_sem009.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM009"),
        "SEM009 should fire on contains_key before remove"
    );
}

#[test]
fn sem009_allows_direct_remove() {
    let good = include_str!("fixtures/good_sem009.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM009"),
        "SEM009 should NOT fire on direct remove"
    );
}

// ==========================================================================
// SEM010: ^ used as exponentiation (XOR bug)
// ==========================================================================
#[test]
fn sem010_detects_xor_as_exponentiation() {
    let bad = include_str!("fixtures/bad_sem010.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM010"),
        "SEM010 should fire on 10 ^ 18 pattern"
    );
}

#[test]
fn sem010_allows_pow() {
    let good = include_str!("fixtures/good_sem010.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM010"),
        "SEM010 should NOT fire on .pow() usage"
    );
}

#[test]
fn sem010_detects_xor_with_trailing_comment() {
    let code = r#"
pub fn issuance() -> u128 {
    10u128 ^ 18 // decimal precision
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEM010"),
        "SEM010 should still fire when the XOR bug appears on a line with a trailing comment"
    );
}

// ==========================================================================
// SEM011: Weight::zero() placeholder
// ==========================================================================
#[test]
fn sem011_detects_weight_zero_in_attribute() {
    let bad = include_str!("fixtures/bad_sem011.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM011"),
        "SEM011 should fire on Weight::zero() in weight attribute"
    );
}

#[test]
fn sem011_allows_benchmarked_weight_in_attribute() {
    let good = include_str!("fixtures/good_sem011.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM011"),
        "SEM011 should NOT fire on benchmarked weight"
    );
}

// ==========================================================================
// VAL002: Division without zero guard
// ==========================================================================
#[test]
fn val002_detects_division_by_config_value() {
    let bad = include_str!("fixtures/bad_val002.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "VAL002"),
        "VAL002 should fire on division by config/storage value without guard"
    );
}

#[test]
fn val002_allows_guarded_division() {
    let good = include_str!("fixtures/good_val002.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "VAL002"),
        "VAL002 should NOT fire when zero guard or checked_div is present"
    );
}

// ==========================================================================
// SEM012: #[allow(dead_code)] in production code
// ==========================================================================
#[test]
fn sem012_detects_allow_dead_code() {
    let bad = include_str!("fixtures/bad_sem012.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM012"),
        "SEM012 should fire on #[allow(dead_code)] in pallet code"
    );
}

#[test]
fn sem012_allows_live_code() {
    let good = include_str!("fixtures/good_sem012.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM012"),
        "SEM012 should NOT fire when no dead_code suppression"
    );
}

#[test]
fn sem012_skips_test_files() {
    let bad = include_str!("fixtures/bad_sem012.rs");
    let diags = check_fixture("pallets/foo/src/tests.rs", bad);
    assert!(!has_rule(&diags, "SEM012"), "SEM012 should skip test files");
}

#[test]
fn sem012_ignores_inline_cfg_test_modules() {
    let code = r#"
pub fn live_code() {}

#[cfg(test)]
mod tests {
    #[allow(dead_code)]
    fn helper() {}
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEM012"),
        "SEM012 should ignore #[allow(dead_code)] inside inline #[cfg(test)] modules"
    );
}

// ==========================================================================
// SEM013: Custom invalidity enums should use #[repr(u8)]
// ==========================================================================
#[test]
fn sem013_detects_missing_repr_u8() {
    let bad = include_str!("fixtures/bad_sem013.rs");
    let diags = check_fixture("pallets/foo/src/extension.rs", bad);
    assert!(
        has_rule(&diags, "SEM013"),
        "SEM013 should fire on custom invalidity enums without #[repr(u8)]"
    );
}

#[test]
fn sem013_allows_repr_u8() {
    let good = include_str!("fixtures/good_sem013.rs");
    let diags = check_fixture("pallets/foo/src/extension.rs", good);
    assert!(
        !has_rule(&diags, "SEM013"),
        "SEM013 should NOT fire when #[repr(u8)] is present"
    );
}

// ==========================================================================
// SEM014: SubmitTransaction logs should use LOG_TARGET
// ==========================================================================
#[test]
fn sem014_detects_missing_log_target() {
    let bad = include_str!("fixtures/bad_sem014.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM014"),
        "SEM014 should fire when SubmitTransaction logging omits target: LOG_TARGET"
    );
}

#[test]
fn sem014_allows_multiline_log_target() {
    let good = include_str!("fixtures/good_sem014.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM014"),
        "SEM014 should NOT fire when target: LOG_TARGET is present on a following line"
    );
}

#[test]
fn sem014_ignores_unrelated_logs() {
    let code = r#"
fn log_other_issue() {
    log::warn!("background cleanup skipped");
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEM014"),
        "SEM014 should ignore log macros unrelated to SubmitTransaction"
    );
}

// ==========================================================================
// SEM015: #[pallet::authorize] should have #[pallet::weight_of_authorize]
// ==========================================================================
#[test]
fn sem015_detects_missing_weight_of_authorize() {
    let bad = include_str!("fixtures/bad_sem015.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM015"),
        "SEM015 should fire when #[pallet::authorize] has no companion #[pallet::weight_of_authorize]"
    );
}

#[test]
fn sem015_allows_weight_of_authorize() {
    let good = include_str!("fixtures/good_sem015.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM015"),
        "SEM015 should NOT fire when #[pallet::weight_of_authorize] is present"
    );
}

// ==========================================================================
// SEM016: CreateAuthorizedTransaction should include AuthorizeCall::new()
// ==========================================================================
#[test]
fn sem016_detects_missing_authorize_call() {
    let bad = include_str!("fixtures/bad_sem016.rs");
    let diags = check_fixture("pallets/foo/src/mock.rs", bad);
    assert!(
        has_rule(&diags, "SEM016"),
        "SEM016 should fire when create_extension omits AuthorizeCall::new()"
    );
}

#[test]
fn sem016_allows_generic_authorize_call() {
    let good = include_str!("fixtures/good_sem016.rs");
    let diags = check_fixture("runtime/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM016"),
        "SEM016 should NOT fire when create_extension includes frame_system::AuthorizeCall::<Runtime>::new()"
    );
}

// ==========================================================================
// TST006: Extrinsic without event
// ==========================================================================
#[test]
fn tst006_detects_extrinsic_without_event() {
    let bad = include_str!("fixtures/bad_tst006.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "TST006"),
        "TST006 should fire on extrinsic that mutates storage without emitting event"
    );
}

#[test]
fn tst006_allows_extrinsic_with_event() {
    let good = include_str!("fixtures/good_tst006.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "TST006"),
        "TST006 should NOT fire when extrinsic emits event"
    );
}

// ==========================================================================
// BEN003: Extrinsic without benchmark
// ==========================================================================
#[test]
fn ben003_detects_extrinsic_without_benchmark() {
    // BEN003 does cross-file analysis, so it needs a real benchmarking.rs sibling.
    // Using inline code that has #[pallet::call] but no sibling benchmark file.
    let bad = include_str!("fixtures/bad_ben003.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    // Should fire because there's no benchmarking.rs next to this lib.rs
    assert!(
        has_rule(&diags, "BEN003"),
        "BEN003 should fire when no benchmarking.rs exists for pallet with extrinsics"
    );
}

#[test]
fn ben003_skips_non_pallet_files() {
    let good = include_str!("fixtures/good_ben003.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "BEN003"),
        "BEN003 should NOT fire on lib.rs without #[pallet::call]"
    );
}

#[test]
fn ben003_allows_extrinsic_with_matching_benchmark_file() {
    let root = std::env::temp_dir().join(format!(
        "polkadot-linter-ben003-match-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pallet_dir = root.join("pallets/foo/src");
    std::fs::create_dir_all(&pallet_dir).unwrap();

    let lib_path = pallet_dir.join("lib.rs");
    let bench_path = pallet_dir.join("benchmarking.rs");
    let lib = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn submit(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;
    let benches = r#"
#[benchmark]
fn submit() {}
"#;

    std::fs::write(&lib_path, lib).unwrap();
    std::fs::write(&bench_path, benches).unwrap();

    let diags = check_fixture_path(lib_path, lib);
    assert!(
        !has_rule(&diags, "BEN003"),
        "BEN003 should not fire when the extrinsic has a sibling benchmark"
    );
}

#[test]
fn ben003_allows_benchmark_variants_for_one_extrinsic() {
    let root = std::env::temp_dir().join(format!(
        "polkadot-linter-ben003-variants-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pallet_dir = root.join("pallets/foo/src");
    std::fs::create_dir_all(&pallet_dir).unwrap();

    let lib_path = pallet_dir.join("lib.rs");
    let bench_path = pallet_dir.join("benchmarking.rs");
    let lib = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn unload_recycler_into_coins(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;
    let benches = r#"
#[benchmark]
fn unload_recycler_into_coins_1_2() {}

#[benchmark]
fn unload_recycler_into_coins_3_8() {}
"#;

    std::fs::write(&lib_path, lib).unwrap();
    std::fs::write(&bench_path, benches).unwrap();

    let diags = check_fixture_path(lib_path, lib);
    assert!(
        !has_rule(&diags, "BEN003"),
        "BEN003 should treat benchmark variants as coverage for the extrinsic"
    );
}

// ==========================================================================
// SEC001: Unbounded Vec in extrinsic params
// ==========================================================================
#[test]
fn sec001_detects_unbounded_vec() {
    let bad = include_str!("fixtures/bad_sec001.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC001"),
        "SEC001 should fire on Vec<T> in extrinsic params"
    );
}

#[test]
fn sec001_allows_bounded_vec() {
    let good = include_str!("fixtures/good_sec001.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC001"),
        "SEC001 should NOT fire on BoundedVec"
    );
}

#[test]
fn sec001_still_checks_lib_rs_with_inline_test_module() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn submit(origin: OriginFor<T>, values: Vec<u8>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn helper() {
        assert_eq!(1, 1);
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC001"),
        "SEC001 should still lint pallet lib.rs files that contain inline #[cfg(test)] modules"
    );
}

#[test]
fn sec001_allows_privileged_origin_vec_inputs() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn set_rules(origin: OriginFor<T>, rules: Vec<u8>) -> DispatchResult {
        T::AdminOrigin::ensure_origin(origin)?;
        Rules::<T>::put(T::Hashing::hash(&rules));
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC001"),
        "SEC001 should not report unbounded Vec inputs on privileged-origin dispatchables"
    );
}

#[test]
fn sec001_allows_privileged_origin_or_root_vec_inputs() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn relay_message(origin: OriginFor<T>, messages: Vec<u8>) -> DispatchResult {
        T::RelayChainOrigin::ensure_origin_or_root(origin)?;
        Messages::<T>::put(messages);
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC001"),
        "SEC001 should not report unbounded Vec inputs on ensure_origin_or_root dispatchables"
    );
}

#[test]
fn sec001_allows_vec_inputs_bounded_in_body() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn submit(origin: OriginFor<T>, values: Vec<u8>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        let bounded = BoundedVec::<u8, T::MaxValues>::try_from(values)
            .map_err(|_| Error::<T>::TooManyValues)?;
        Values::<T>::put(bounded);
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC001"),
        "SEC001 should not report Vec inputs converted to bounded collections in the dispatchable"
    );
}

#[test]
fn sec001_allows_vec_inputs_accounted_for_in_weight() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::remark(remark.len() as u32))]
    pub fn remark(origin: OriginFor<T>, remark: Vec<u8>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        let hash = T::Hashing::hash(&remark[..]);
        Self::deposit_event(Event::Remarked { sender: who, hash });
        Ok(())
    }

    #[pallet::call_index(1)]
    #[pallet::weight(T::WeightInfo::submit())]
    pub fn submit(origin: OriginFor<T>, values: Vec<u8>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        let _ = values;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec001_count = diags.iter().filter(|d| d.rule_id == "SEC001").count();
    assert_eq!(
        sec001_count, 1,
        "SEC001 should skip length-weighted Vec inputs while still reporting unaccounted Vec inputs"
    );
}

#[test]
fn sec001_skips_deprecated_dispatchables() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[deprecated(note = "use call instead")]
    pub fn call_old_weight(origin: OriginFor<T>, data: Vec<u8>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        let _ = data;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/contracts/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC001"),
        "SEC001 should not report deprecated compatibility dispatchables"
    );
}

#[test]
fn sec001_skips_max_weight_dispatchables() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(Weight::MAX)]
    pub fn eth_transact(origin: OriginFor<T>, payload: Vec<u8>) -> DispatchResult {
        let _ = origin;
        let _ = payload;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/revive/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC001"),
        "SEC001 should not report calls intentionally assigned Weight::MAX"
    );
}

// ==========================================================================
// SEC002: debug_assert in production code
// ==========================================================================
#[test]
fn sec002_detects_debug_assert() {
    let bad = include_str!("fixtures/bad_sec002.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC002"),
        "SEC002 should fire on debug_assert! in production"
    );
}

#[test]
fn sec002_allows_defensive() {
    let good = include_str!("fixtures/good_sec002.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC002"),
        "SEC002 should NOT fire on defensive!()"
    );
}

#[test]
fn sec002_skips_test_files() {
    let bad = include_str!("fixtures/bad_sec002.rs");
    let diags = check_fixture("pallets/foo/src/tests.rs", bad);
    assert!(!has_rule(&diags, "SEC002"), "SEC002 should skip test files");
}

#[test]
fn sec002_skips_qed_debug_assert_invariants() {
    let code = r#"
pub fn proven_invariant() {
    debug_assert!(supply >= amount, "checked in prep; qed");
}

pub fn normal_debug_assert() {
    debug_assert!(external_input_is_valid());
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip qed-marked invariant checks while reporting normal debug assertions"
    );
}

#[test]
fn sec002_skips_comment_marked_invariant_debug_asserts() {
    let code = r#"
pub fn checked_by_validate_unsigned() {
    // Checked by ValidateUnsigned before this path is reached.
    debug_assert_eq!(registration.session_index, CurrentSessionIndex::<T>::get());
}

pub fn should_not_fail_after_state_check() {
    let res = T::Currency::transfer(&source, &dest, amount, AllowDeath); // should not fail
    debug_assert!(res.is_ok());
}

pub fn normal_debug_assert() {
    debug_assert!(external_input_is_valid());
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip comment-marked invariants while reporting ordinary debug assertions"
    );
}

#[test]
fn sec002_skips_sanity_comment_marked_debug_asserts() {
    let code = r#"
pub fn get_submissions() {
    // validate that the stored state is sane
    debug_assert!(submissions.next_idx > max_idx);
}

pub fn insert_submission() {
    // verify the expectation that we never reuse an index
    debug_assert!(!indices.iter().any(|idx| *idx == next_idx));
}

pub fn trim_assignments() {
    // ensure our post-conditions are correct
    debug_assert!(encoded_size <= max_allowed_length);
}

pub fn normal_debug_assert() {
    debug_assert!(external_input_is_sane(), "input should be sane");
}
"#;
    let diags = check_fixture(
        "substrate/frame/election-provider-multi-phase/src/signed.rs",
        code,
    );
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip sanity/post-condition comments without treating sane messages as invariants"
    );
}

#[test]
fn sec002_skips_consistency_and_corruption_invariant_comments() {
    let code = r#"
pub fn apply_backers() {
    // consistency checks
    debug_assert_eq!(state.validators_for.get(index), Some(&true));
}

pub fn dissolve() {
    // Assuming state is not corrupted, all contributions have been cleaned up.
    debug_assert!(contribution_iterator().count().is_zero());
}

pub fn report_result() {
    // assumption of the trait.
    debug_assert!(matches!(verifier.status(), Status::Nothing));
}

pub fn normal_debug_assert() {
    // validate external input shape
    debug_assert!(external_input_is_valid());
}
"#;
    let diags = check_fixture("polkadot/runtime/parachains/src/disputes.rs", code);
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip documented consistency/corruption invariants while reporting ordinary debug assertions"
    );
}

#[test]
fn sec002_skips_safety_comments_with_caller_validated_invariants() {
    let code = r#"
pub fn relative_jump(offset: isize) {
    // SAFETY: The offset is validated by the caller to ensure it points within the bytecode
    debug_assert!(new_pc <= bytes.len());
}

pub fn read_slice(len: usize) {
    // SAFETY: The caller ensures that `len` bytes are available from the current instruction
    // pointer position.
    debug_assert!(pc.checked_add(len).map_or(false, |end| end <= bytes.len()));
}

pub fn generic_safety_claim() {
    // This should be safe enough for external input.
    debug_assert!(external_input_is_valid());
}
"#;
    let diags = check_fixture("substrate/frame/revive/src/vm/evm/ext_bytecode.rs", code);
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip only SAFETY comments that document caller-validated invariants"
    );
}

#[test]
fn sec002_skips_client_verified_runtime_invariant_comments() {
    let code = r#"
pub fn check_slot_claim() {
    // NOTE: this is verified by the client when importing the block, before
    // execution. We don't run the verification again here to avoid slowing
    // down the runtime.
    debug_assert!(public.vrf_verify(&payload, &signature));
}

pub fn vague_verified_claim() {
    // Verified somewhere before this point.
    debug_assert!(external_input_is_valid());
}
"#;
    let diags = check_fixture("substrate/frame/babe/src/lib.rs", code);
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip only comments that explicitly say the client already verified the invariant"
    );
}

#[test]
fn sec002_skips_explicit_invariant_assertion_messages() {
    let code = r#"
pub fn apply_mode() {
    debug_assert!(
        !is_potential,
        "`PotentialFullCore` should resolve to `FullCore` or `FractionOfCore` after applying a transaction.",
    );
}

pub fn prune_statuses() {
    debug_assert!(!statuses.iter().any(|s| s.signals_exist), "Signals should be handled");
}

pub fn append_queue() {
    debug_assert!(schedule.next_schedule.is_none(), "queue.end was supposed to be the end");
}

pub fn remove_agent() {
    debug_assert!(Agents::<T>::contains_key(key), "Agent should exist in storage");
}

pub fn unknit_single_item_ring() {
    debug_assert!(origin == neighbours.prev, "outgoing must be only item");
}

pub fn progress_page() {
    debug_assert!(book_state.ready_neighbours.is_some(), "Must be in ready ring if ready");
    debug_assert!(book_state.count > 0, "reaping a page implies there are pages");
    debug_assert!(status != Bailed, "we never bail if a page became complete");
}

pub fn update_member() {
    // The pool id of a member cannot change in any case, so we use it to make sure
    // `member_account` is the right one.
    debug_assert_eq!(member.pool_id, bonded_pool.id);
}

pub fn normal_debug_assert() {
    debug_assert!(external_input_is_valid(), "external input should be valid");
}
"#;
    let diags = check_fixture("substrate/frame/message-queue/src/lib.rs", code);
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip exact invariant messages without treating generic should/must messages as safe"
    );
}

#[test]
fn sec002_skips_extended_invariant_comments_for_false_and_result_asserts() {
    let code = r#"
pub fn notify_downward_message() {
    // this should never happen unless the max message size is configured to a
    // jokingly small number.
    log::error!(
        target: "runtime::hrmp",
        "sending notification failed."
    );
    debug_assert!(false);
}

pub fn payout() {
    // Should not fail because curator fee is always less than bounty value.
    let fee_transfer_result = T::Currency::transfer(
        &source,
        &dest,
        fee,
        AllowDeath,
    );
    debug_assert!(fee_transfer_result.is_ok());
}

pub fn unrelated_assertion() {
    // Should not fail.
    log::trace!("step 1");
    log::trace!("step 2");
    log::trace!("step 3");
    log::trace!("step 4");
    let value = external_value();
    debug_assert!(value > 0);
}
"#;
    let diags = check_fixture("substrate/frame/child-bounties/src/lib.rs", code);
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should extend invariant comments only for false/result debug assertions"
    );
}

#[test]
fn sec002_skips_defensive_result_assertions() {
    let code = r#"
pub fn settle_deposit(who: &T::AccountId, to_refund: Balance, to_slash: Balance) {
    let _res = T::Currency::release(
        &HoldReason::SignedSubmission.into(),
        who,
        to_refund,
        Precision::BestEffort,
    )
    .defensive();
    debug_assert_eq!(_res, Ok(to_refund));

    let _r = T::Currency::burn_held(
        &HoldReason::SignedSubmission.into(),
        who,
        to_slash,
        Precision::BestEffort,
        Fortitude::Force,
    )
    .defensive();
    debug_assert!(_r.is_ok());
}

pub fn unchecked_debug_assert() {
    let _res = T::Currency::transfer(&source, &dest, amount, AllowDeath);
    debug_assert!(_res.is_ok());
}
"#;
    let diags = check_fixture(
        "substrate/frame/election-provider-multi-block/src/signed/mod.rs",
        code,
    );
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip assertions over defensive results while reporting unchecked debug assertions"
    );
}

#[test]
fn sec002_skips_balance_remainder_debug_asserts() {
    let code = r#"
pub fn refund_deposit(who: &T::AccountId, deposit: Balance) {
    let err_amount = T::Currency::unreserve(who, deposit);
    debug_assert!(err_amount.is_zero());
}

pub fn slash_deposit(who: &T::AccountId, deposit: Balance) {
    let (imbalance, _remainder) = T::Currency::slash_reserved(who, deposit);
    debug_assert!(_remainder.is_zero());
    T::SlashHandler::on_unbalanced(imbalance);
}

pub fn unrelated_unreserve(who: &T::AccountId, deposit: Balance) {
    let err_amount = Balance::zero();
    T::Currency::unreserve(who, deposit);
    debug_assert!(err_amount.is_zero());
}

pub fn unrelated_debug_assert() {
    debug_assert!(false, "refund did not result in dead account?!");
}
"#;
    let diags = check_fixture("substrate/frame/elections-phragmen/src/lib.rs", code);
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 2,
        "SEC002 should skip balance remainder assertions without masking unrelated debug assertions"
    );
}

#[test]
fn sec002_skips_bounded_clear_prefix_result_assertions() {
    let code = r#"
pub fn take_submission_with_data(round: u32, who: &T::AccountId) {
    // NOTE: safe to remove unbounded, as at most `Pages` pages are stored.
    let r = SubmissionStorage::<T>::clear_prefix((round, who), u32::MAX, None);
    debug_assert!(r.unique <= T::Pages::get());
}

pub fn clear_era_information(era_index: EraIndex) {
    // FIXME: We can possibly set a reasonable limit since we do this only once per era.
    let mut cursor = ErasStakers::<T>::clear_prefix(era_index, u32::MAX, None);
    debug_assert!(cursor.maybe_cursor.is_none());
}
"#;
    let diags = check_fixture(
        "substrate/frame/election-provider-multi-block/src/signed/mod.rs",
        code,
    );
    let sec002_count = diags.iter().filter(|d| d.rule_id == "SEC002").count();
    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip bounded clear_prefix result assertions while reporting unbounded cursor assertions"
    );
}

#[test]
fn sec002_skips_non_runtime_utility_crates() {
    let bad = include_str!("fixtures/bad_sec002.rs");
    for path in [
        "substrate/client/network/src/protocol/notifications/behaviour.rs",
        "polkadot/node/core/backing/src/lib.rs",
        "cumulus/client/consensus/common/src/level_monitor.rs",
        "cumulus/pallets/parachain-system/proc-macro/src/lib.rs",
        "substrate/frame/revive/rpc/src/cli.rs",
        "substrate/frame/contracts/uapi/src/host.rs",
        "substrate/frame/staking-async/rc-client/src/lib.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(
            !has_rule(&diags, "SEC002"),
            "SEC002 should skip non-runtime utility path {path}"
        );
    }
}

#[test]
fn sec002_skips_documented_benchmark_test_builder_files() {
    let code = r#"
/// This is directly from frame-benchmarking so it can be used in benchmarks and tests.
/// Paras inherent `enter` benchmark scenario builder.
pub(crate) struct BenchBuilder;

pub fn build_scenario() {
    debug_assert!(true);
}
"#;
    let diags = check_fixture("polkadot/runtime/parachains/src/builder.rs", code);
    assert!(
        !has_rule(&diags, "SEC002"),
        "SEC002 should skip documented benchmark/test builder helpers"
    );
}

#[test]
fn sec002_still_lints_runtime_and_pallet_paths() {
    let bad = include_str!("fixtures/bad_sec002.rs");
    for path in [
        "polkadot/runtime/common/src/slots/mod.rs",
        "substrate/frame/staking/src/pallet/impls.rs",
        "cumulus/pallets/parachain-system/src/lib.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(
            has_rule(&diags, "SEC002"),
            "SEC002 should still lint runtime/pallet path {path}"
        );
    }
}

// ==========================================================================
// SEC003: Missing decode depth limit
// ==========================================================================
#[test]
fn sec003_detects_decode_without_limit() {
    let bad = include_str!("fixtures/bad_sec003.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC003"),
        "SEC003 should fire on Decode::decode without depth limit"
    );
}

#[test]
fn sec003_allows_depth_limited_decode() {
    let good = include_str!("fixtures/good_sec003.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC003"),
        "SEC003 should NOT fire on decode_with_depth_limit"
    );
}

#[test]
fn sec003_checks_each_decode_individually() {
    let code = r#"
pub fn decode_two(mut safe: &[u8], mut unsafe_data: &[u8]) -> DispatchResult {
    let _safe_call = <T as Config>::RuntimeCall::decode_with_depth_limit(
        sp_io::MAX_EXTRINSIC_DEPTH,
        &mut safe,
    )?;
    let _unsafe_call = <T as Config>::RuntimeCall::decode(&mut unsafe_data)?;
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(has_rule(&diags, "SEC003"), "SEC003 should still fire when one decode is unsafe even if another decode uses a depth limit");
}

#[test]
fn sec003_allows_decode_of_non_recursive_internal_types() {
    let code = r#"
pub struct MigrationState;

pub fn load_state(mut data: &[u8]) -> Result<MigrationState, Error> {
    let state = MigrationState::decode(&mut data)?;
    Ok(state)
}
"#;
    let diags = check_fixture("pallets/foo/src/migration.rs", code);
    assert!(
        !has_rule(&diags, "SEC003"),
        "SEC003 should not fire on concrete internal types with no recursive runtime-call structure"
    );
}

#[test]
fn sec003_detects_unchecked_extrinsic_decode_without_limit() {
    let code = r#"
pub fn decode_extrinsic(mut data: &[u8]) -> DispatchResult {
    let xt = UncheckedExtrinsic::decode(&mut data)?;
    Executive::apply_extrinsic(xt)
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC003"),
        "SEC003 should still fire on recursive extrinsic/call decodes without a depth limit"
    );
}

#[test]
fn sec003_allows_runtime_call_decode_from_local_using_encoded_tuple() {
    let code = r#"
pub fn notify(pallet_index: u8, call_index: u8, query_id: QueryId, response: Response) -> Weight {
    let bare = (pallet_index, call_index, query_id, response);
    if let Ok(call) = bare.using_encoded(|mut bytes| {
        <T as Config>::RuntimeCall::decode(&mut bytes)
    }) {
        return call.get_dispatch_info().call_weight;
    }
    Weight::zero()
}
"#;
    let diags = check_fixture("pallets/xcm/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC003"),
        "SEC003 should not report RuntimeCall decodes from bytes locally produced by using_encoded"
    );
}

// ==========================================================================
// SEC004: Unsafe arithmetic in weight attributes
// ==========================================================================
#[test]
fn sec004_detects_unsafe_weight_arithmetic() {
    let bad = include_str!("fixtures/bad_sec004.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC004"),
        "SEC004 should fire on .add()/.mul() in weight attr"
    );
}

#[test]
fn sec004_allows_saturating_weight_arithmetic() {
    let good = include_str!("fixtures/good_sec004.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC004"),
        "SEC004 should NOT fire on saturating_add/mul"
    );
}

#[test]
fn sec004_detects_infix_weight_arithmetic() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::weight(
        T::WeightInfo::base() + T::WeightInfo::per_item() * items.len() as u64
    )]
    #[pallet::call_index(0)]
    pub fn process(origin: OriginFor<T>, items: BoundedVec<u8, MaxItems>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC004"),
        "SEC004 should fire on infix +/* inside weight attributes"
    );
}

// ==========================================================================
// SEC005: Expensive operations in weight calculation
// ==========================================================================
#[test]
fn sec005_detects_storage_read_in_weight() {
    let bad = include_str!("fixtures/bad_sec005.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC005"),
        "SEC005 should fire on ::get() inside #[pallet::weight]"
    );
}

#[test]
fn sec005_allows_pure_weight_function() {
    let good = include_str!("fixtures/good_sec005.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC005"),
        "SEC005 should NOT fire on pure WeightInfo calls"
    );
}

// ==========================================================================
// SEC006: Unchecked repatriate_reserved return value
// ==========================================================================
#[test]
fn sec006_detects_discarded_repatriate() {
    let bad = include_str!("fixtures/bad_sec006.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC006"),
        "SEC006 should fire on let _ = repatriate_reserved"
    );
}

#[test]
fn sec006_allows_checked_repatriate() {
    let good = include_str!("fixtures/good_sec006.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC006"),
        "SEC006 should NOT fire when return value is checked"
    );
}

#[test]
fn sec006_detects_bound_but_unchecked_remaining() {
    let code = r#"
pub fn transfer_deposit(from: &T::AccountId, to: &T::AccountId, amount: Balance) -> DispatchResult {
    let remaining = T::Currency::repatriate_reserved(from, to, amount, BalanceStatus::Free)?;
    log::debug!("remaining = {:?}", remaining);
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC006"),
        "SEC006 should fire when the remaining amount is bound but never checked"
    );
}

#[test]
fn sec006_allows_remaining_accounted_in_transfer_amount() {
    let code = r#"
pub fn transfer_deposit(from: &T::AccountId, to: &T::AccountId, amount: Balance) -> DispatchResult {
    let lost = T::Currency::repatriate_reserved(from, to, amount, BalanceStatus::Free)?;
    *stored_deposit = amount.saturating_sub(lost);

    let remain = T::Currency::repatriate_reserved(from, to, amount, BalanceStatus::Free)?;
    let actual = amount.defensive_saturating_sub(remain);
    log::debug!("actual = {:?}", actual);
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC006"),
        "SEC006 should allow returned remaining amounts that are used to compute the actual transfer"
    );
}

#[test]
fn sec006_allows_documented_infallible_debug_asserts() {
    let code = r#"
/// Either repatriate the deposit into the Society account or ban the vouching member.
///
/// In neither case can we do much if the action isn't completable, but there's
/// no reason that either should fail.
///
/// WARNING: This alters voucher state. You must ensure that you do not
/// accidentally overwrite it with an older value after calling this.
fn reject_candidate(who: &T::AccountId, deposit: Balance) {
    let pot = Self::account_id();
    let r = T::Currency::repatriate_reserved(&who, &pot, deposit, BalanceStatus::Free);
    debug_assert!(r.is_ok());
}

fn unchecked_repatriate(from: &T::AccountId, to: &T::AccountId, amount: Balance) {
    let remaining = T::Currency::repatriate_reserved(from, to, amount, BalanceStatus::Free);
    log::debug!("remaining = {:?}", remaining);
}
"#;
    let diags = check_fixture("pallets/society/src/lib.rs", code);
    let sec006_count = diags.iter().filter(|d| d.rule_id == "SEC006").count();
    assert_eq!(
        sec006_count, 1,
        "SEC006 should skip documented infallible debug assertions while reporting unchecked results"
    );
}

#[test]
fn sec006_only_checks_direct_repatriate_initializer() {
    let code = r#"
pub fn transfer_deposit(from: &T::AccountId, to: &T::AccountId, amount: Balance) -> DispatchResult {
    let actual = if use_reserved {
        let remain = T::Currency::repatriate_reserved(from, to, amount, BalanceStatus::Free)?;
        amount.defensive_saturating_sub(remain)
    } else {
        amount
    };
    log::debug!("actual = {:?}", actual);
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC006"),
        "SEC006 should not report outer variables whose initializer only contains a nested repatriate call"
    );
}

// ==========================================================================
// SEC007: let _ = swallowing Result
// ==========================================================================
#[test]
fn sec007_detects_let_underscore_result() {
    let bad = include_str!("fixtures/bad_sec007.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC007"),
        "SEC007 should fire on let _ = Result-returning call"
    );
}

#[test]
fn sec007_allows_propagated_errors() {
    let good = include_str!("fixtures/good_sec007.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC007"),
        "SEC007 should NOT fire when errors are propagated with ?"
    );
}

#[test]
fn sec007_allows_let_underscore_when_error_is_propagated() {
    let code = r#"
pub fn process_member(who: &T::AccountId) -> DispatchResult {
    let _ = T::Currency::transfer(who, &pot, amount, Preservation::Expendable)?;
    let _ = Self::ensure_score_quality(claimed_score).map_err(|error| Error::<T>::BadScore(error))?;
    let _ = T::Currency::reserve(who, deposit);
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec007_count = diags.iter().filter(|d| d.rule_id == "SEC007").count();
    assert_eq!(
        sec007_count, 1,
        "SEC007 should skip top-level ? expressions while reporting truly discarded Results"
    );
}

#[test]
fn sec007_allows_intentionally_ignored_result_with_comment() {
    let code = r#"
pub fn refund_best_effort(who: &T::AccountId) -> DispatchResult {
    // Ignore errors since this can only fail if the receiver does not exist.
    let _ = T::Currency::transfer(who, &pot, amount, Preservation::Expendable);
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC007"),
        "SEC007 should allow explicitly documented intentional result discards"
    );
}

#[test]
fn sec007_allows_documented_ignore_after_setup_lines() {
    let code = r#"
pub fn drain_reward_account(reward_account: &T::AccountId, depositor: &T::AccountId) {
    // This shouldn't fail, but if it does we don't really care.
    let reward_pool_remaining = T::Currency::reducible_balance(
        reward_account,
        Preservation::Expendable,
        Fortitude::Polite,
    );
    let _ = T::Currency::transfer(
        reward_account,
        depositor,
        reward_pool_remaining,
        Preservation::Expendable,
    );
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC007"),
        "SEC007 should allow documented intentional ignores even after nearby setup lines"
    );
}

#[test]
fn sec007_does_not_treat_unreserve_as_result() {
    let code = r#"
pub fn refund_deposit(who: &T::AccountId) {
    let _ = T::Currency::unreserve(who, deposit);
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC007"),
        "SEC007 should not report Currency::unreserve, which returns a balance rather than a Result"
    );
}

#[test]
fn sec007_allows_map_err_with_explicit_error_side_effect() {
    let code = r#"
pub fn migrate_pool(id: PoolId) {
    let _ = Pallet::<T>::migrate_to_delegate_stake(id).map_err(|err| {
        log!(warn, "failed to migrate pool {:?}: {:?}", id, err)
    });

    let mut failures = 0;
    let _ = ledger.clone().set_controller_to_stash().map_err(|_| failures += 1);

    let _ = T::Currency::transfer(&from, &to, amount, Preservation::Expendable);
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec007_count = diags.iter().filter(|d| d.rule_id == "SEC007").count();
    assert_eq!(
        sec007_count, 1,
        "SEC007 should allow map_err logging/counting while still reporting silent result discards"
    );
}

#[test]
fn sec007_allows_try_mutate_unit_error_control_flow() {
    let code = r#"
pub fn maybe_update_fee_factor() {
    let _ = Bridge::<T>::try_mutate(|bridge| {
        if !bridge.is_congested {
            return Err(());
        }
        bridge.delivery_fee_factor = bridge.delivery_fee_factor.saturating_add(1);
        Ok(())
    });
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC007"),
        "SEC007 should allow try_mutate unit errors used only to abort storage mutation"
    );
}

#[test]
fn sec007_reports_try_mutate_real_error_discards() {
    let code = r#"
pub fn spend_credit(who: &T::AccountId) {
    let _ = Credits::<T>::try_mutate(who, |credit| -> Result<(), Error<T>> {
        if *credit == 0 {
            return Err(Error::<T>::NoCredit);
        }
        *credit -= 1;
        Ok(())
    });
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC007"),
        "SEC007 should still report discarded try_mutate results with real pallet errors"
    );
}

#[test]
fn sec007_skips_test_files() {
    let bad = include_str!("fixtures/bad_sec007.rs");
    let diags = check_fixture("pallets/foo/src/tests.rs", bad);
    assert!(!has_rule(&diags, "SEC007"), "SEC007 should skip test files");
}

#[test]
fn sec007_is_scoped_to_runtime_and_pallet_paths() {
    let bad = include_str!("fixtures/bad_sec007.rs");

    let client_diags = check_fixture("substrate/client/service/src/client/client.rs", bad);
    assert!(
        !has_rule(&client_diags, "SEC007"),
        "SEC007 should skip non-runtime client infrastructure"
    );

    let support_diags = check_fixture(
        "substrate/frame/support/src/traits/tokens/imbalance/on_unbalanced.rs",
        bad,
    );
    assert!(
        !has_rule(&support_diags, "SEC007"),
        "SEC007 should skip FRAME support infrastructure"
    );

    let pallet_diags = check_fixture("substrate/frame/staking/src/pallet/mod.rs", bad);
    assert!(
        has_rule(&pallet_diags, "SEC007"),
        "SEC007 should still fire in FRAME pallet code"
    );
}

// ==========================================================================
// SEC008: Panic in production code
// ==========================================================================
#[test]
fn sec008_detects_unwrap() {
    let bad = include_str!("fixtures/bad_sec008.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC008"),
        "SEC008 should fire on .unwrap()/.expect()/panic!() in production"
    );
    // Should find multiple: unwrap, expect, panic, todo
    let count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert!(
        count >= 3,
        "SEC008 should find at least 3 panic-capable patterns, found {count}"
    );
}

#[test]
fn sec008_allows_defensive_patterns() {
    let good = include_str!("fixtures/good_sec008.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC008"),
        "SEC008 should NOT fire on defensive!() or unwrap_or_default()"
    );
}

#[test]
fn sec008_skips_test_files() {
    let bad = include_str!("fixtures/bad_sec008.rs");
    let diags = check_fixture("pallets/foo/src/tests.rs", bad);
    assert!(!has_rule(&diags, "SEC008"), "SEC008 should skip test files");
}

#[test]
fn sec008_skips_benchmark_files() {
    let bad = include_str!("fixtures/bad_sec008.rs");
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", bad);
    assert!(
        !has_rule(&diags, "SEC008"),
        "SEC008 should skip benchmark files"
    );
}

#[test]
fn sec008_skips_genesis_build_blocks() {
    let code = r#"
#[pallet::genesis_build]
impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
    fn build(&self) {
        let first = self.nodes.first().expect("genesis config must include a node");
        let _ = first;
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC008"),
        "SEC008 should not report genesis-build panics that fail invalid chain configuration"
    );
}

#[test]
fn sec008_skips_genesis_config_helper_impls() {
    let code = r#"
impl<T: Config<I>, I: 'static> GenesisConfig<T, I> {
    fn generate_random_accounts(count: u32) -> Vec<T::AccountId> {
        (0..count)
            .map(|index| {
                let pair = Pair::from_string(&format!("//{index}"), None)
                    .expect("genesis seed must derive");
                T::AccountId::decode(&mut &pair.public().encode()[..])
                    .expect("genesis account id must decode")
            })
            .collect()
    }
}

impl<T: Config> GenesisConfig<T> {
    fn generate_endowed_bonded_account(derivation: &str) -> T::AccountId {
        let pair = Pair::from_string(derivation, None)
            .expect("genesis seed must parse");
        T::AccountId::decode(&mut &pair.public().encode()[..])
            .expect("genesis account id must decode")
    }
}

pub fn production_helper() {
    let _value = Some(1u32).unwrap();
}
"#;
    let diags = check_fixture("substrate/frame/staking-async/src/pallet/mod.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip genesis config helper impls while still reporting production panics"
    );
}

#[test]
fn sec008_skips_const_unit_initializer_assertions() {
    let code = r#"
impl<T: Config> Precompiles<T> for Tuple {
    const CHECK_COLLISION: () = {
        let matchers = [for_tuples!( #( Tuple::MATCHER ),* )];
        if BuiltinAddressMatcher::has_duplicates(&matchers) {
            panic!("Precompiles with duplicate matcher detected")
        }
        for_tuples!(
            #(
                let is_fixed = Tuple::MATCHER.is_fixed();
                let has_info = Tuple::HAS_CONTRACT_INFO;
                assert!(is_fixed || !has_info, "Only fixed precompiles can have a contract info.");
            )*
        );
    };
}

pub fn production_helper() {
    panic!("runtime panic");
}
"#;
    let diags = check_fixture("substrate/frame/revive/src/precompiles.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip compile-time unit const assertions while reporting runtime panics"
    );
}

#[test]
fn sec008_skips_runtime_benchmark_cfg_blocks() {
    let code = r#"
#[cfg(feature = "runtime-benchmarks")]
pub fn benchmark_setup() {
    let account = MaybeAccount::get().expect("benchmark account exists");
    let _ = account;
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC008"),
        "SEC008 should not report panic helpers inside runtime-benchmark cfg blocks"
    );
}

#[test]
fn production_security_rules_skip_crate_level_non_production_cfg() {
    let code = r#"
#![cfg(any(feature = "runtime-benchmarks", test))]

pub fn helper(first: u32, second: u32) -> Result<u32, Error> {
    debug_assert!(first <= second);
    let _value = Some(first).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("substrate/frame/revive/src/call_builder.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        assert!(
            !has_rule(&diags, rule_id),
            "{rule_id} should skip crate-level test/runtime-benchmark-only files"
        );
    }
}

#[test]
fn production_security_rules_skip_crate_level_test_helpers_cfg() {
    let code = r#"
#![cfg(any(feature = "test-helpers", test))]

pub fn helper(first: u32, second: u32) -> Result<u32, Error> {
    debug_assert!(first <= second);
    let _value = Some(first).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("bridges/primitives/runtime/src/storage_proof.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        assert!(
            !has_rule(&diags, rule_id),
            "{rule_id} should skip crate-level test-helper-only files"
        );
    }
}

#[test]
fn production_security_rules_skip_remote_test_cfg_blocks() {
    let code = r#"
#[cfg(feature = "remote-test")]
pub(crate) mod remote_tests {
    pub async fn run_with_limits(first: u32, second: u32) -> Result<u32, Error> {
        debug_assert!(first <= second);
        let value = Some(first).unwrap();
        Ok(value + second)
    }
}

pub fn production_helper(first: u32, second: u32) -> Result<u32, Error> {
    debug_assert!(second > 0);
    let value = Some(second).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("substrate/frame/state-trie-migration/src/lib.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        let count = diags.iter().filter(|diag| diag.rule_id == rule_id).count();
        assert_eq!(
            count, 1,
            "{rule_id} should skip remote-test-only blocks without masking following production code"
        );
    }
}

#[test]
fn production_security_rules_skip_integrity_test_cfg_blocks() {
    let code = r#"
#[cfg(feature = "integrity-test")]
mod integrity_tests {
    fn ensure_priority_boost_is_sane(first: u32, second: u32) -> Result<u32, Error> {
        debug_assert!(first <= second);
        let value = Some(first).expect("integrity test setup must produce a value");
        if value + second > 10 {
            panic!("integrity test threshold exceeded");
        }
        Ok(value)
    }
}

pub fn production_helper(first: u32, second: u32) -> Result<u32, Error> {
    debug_assert!(second > 0);
    let value = Some(second).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("bridges/modules/relayers/src/extension/priority.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        let count = diags.iter().filter(|diag| diag.rule_id == rule_id).count();
        assert_eq!(
            count, 1,
            "{rule_id} should skip integrity-test-only blocks without masking following production code"
        );
    }
}

#[test]
fn production_security_rules_skip_cfg_expression_blocks() {
    let code = r#"
pub fn prod(first: u32, second: u32) -> Result<u32, Error> {
    #[cfg(test)]
    {
        debug_assert!(first <= second);
        let _value = Some(first).unwrap();
        let _sum = first + second;
    }

    debug_assert!(second > 0);
    let _value = Some(second).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        let count = diags.iter().filter(|diag| diag.rule_id == rule_id).count();
        assert_eq!(
            count, 1,
            "{rule_id} should skip cfg(test) expression blocks without masking following production code"
        );
    }
}

#[test]
fn production_security_rules_skip_debug_assertions_cfg_blocks() {
    let code = r#"
pub fn prod(first: u32, second: u32) -> Result<u32, Error> {
    if cfg!(debug_assertions) && cfg!(not(feature = "runtime-benchmarks")) {
        debug_assert!(first <= second);
        let _value = Some(first).unwrap();
        let _sum = first + second;
    }

    debug_assert!(second > 0);
    let _value = Some(second).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture(
        "substrate/frame/staking-async/src/session_rotation.rs",
        code,
    );
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        let count = diags.iter().filter(|diag| diag.rule_id == rule_id).count();
        assert_eq!(
            count, 1,
            "{rule_id} should skip cfg!(debug_assertions) blocks without masking following production code"
        );
    }
}

#[test]
fn production_security_rules_skip_cfg_feature_expression_blocks() {
    let code = r#"
pub fn upgrade_failed(first: u32, second: u32) -> Result<u32, Error> {
    if cfg!(feature = "try-runtime") {
        debug_assert!(first <= second);
        let _value = Some(first).unwrap();
        let _sum = first + second;
        panic!("try-runtime migration failed");
    } else {
        return Ok(first);
    }

    debug_assert!(second > 0);
    let _value = Some(second).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("substrate/frame/migrations/src/lib.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        let count = diags.iter().filter(|diag| diag.rule_id == rule_id).count();
        assert_eq!(
            count, 1,
            "{rule_id} should skip positive non-production cfg! feature blocks without masking following production code"
        );
    }
}

#[test]
fn production_security_rules_skip_cfg_control_flow_blocks() {
    let code = r#"
pub fn prod(first: u32, second: u32, run_all: bool) -> Result<u32, Error> {
    #[cfg(feature = "try-runtime")]
    if run_all {
        debug_assert!(first <= second);
        let _value = Some(first).unwrap();
        let _sum = first + second;
    }

    debug_assert!(second > 0);
    let _value = Some(second).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("substrate/frame/contracts/src/migration.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        let count = diags.iter().filter(|diag| diag.rule_id == rule_id).count();
        assert_eq!(
            count, 1,
            "{rule_id} should skip cfg-gated control-flow blocks without masking following production code"
        );
    }
}

#[test]
fn production_security_rules_skip_cfg_multiline_statements() {
    let code = r#"
pub fn prod(first: u32, second: u32, cursor: Cursor) -> Result<u32, Error> {
    #[cfg(feature = "try-runtime")]
    T::Migrations::nth_post_upgrade(
        cursor.index,
        PreUpgradeBytes::<T>::get(&bounded_id).0,
    )
    .expect("Invalid cursor.index.")
    .expect("Post-upgrade failed.");

    #[cfg(feature = "try-runtime")]
    let _sum = first + second;

    debug_assert!(second > 0);
    let _value = Some(second).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("substrate/frame/migrations/src/lib.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        let count = diags.iter().filter(|diag| diag.rule_id == rule_id).count();
        assert_eq!(
            count, 1,
            "{rule_id} should skip cfg-gated multiline statements without masking following production code"
        );
    }
}

#[test]
fn production_security_rules_skip_integrity_tests() {
    let code = r#"
impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn integrity_test() {
        debug_assert!(T::MaxMembers::get() > 0);
        T::BlockWeights::get().validate().expect("runtime config must be valid");
    }
}

pub fn integrity_test(first: u32, second: u32) -> DispatchResult {
    let _sum = first + second;
    Ok(())
}

#[cfg(feature = "std")]
pub fn native_only_helper(first: u32, second: u32) -> DispatchResult {
    debug_assert!(first <= second);
    let _value = Some(first).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("substrate/frame/system/src/lib.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        let count = diags.iter().filter(|diag| diag.rule_id == rule_id).count();
        assert_eq!(
            count, 1,
            "{rule_id} should skip integrity_test bodies while still reporting std-gated production helpers"
        );
    }
}

#[test]
fn production_security_rules_skip_cfg_items_after_comments() {
    let code = r#"
pub struct Pallet;

#[cfg(any(feature = "runtime-benchmarks", test))]
// helper code for testing and benchmarking
impl Pallet {
    pub fn helper(first: u32, second: u32) -> Result<u32, Error> {
        debug_assert!(first <= second);
        let _value = Some(first).unwrap();
        Ok(first + second)
    }
}

pub fn prod(first: u32, second: u32) -> Result<u32, Error> {
    debug_assert!(second > 0);
    let _value = Some(second).unwrap();
    Ok(first + second)
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    for rule_id in ["SEC002", "SEC008", "SEC009"] {
        let count = diags.iter().filter(|diag| diag.rule_id == rule_id).count();
        assert_eq!(
            count, 1,
            "{rule_id} should skip cfg-gated items after comments without masking production code"
        );
    }
}

#[test]
fn production_security_rules_skip_documented_test_only_items() {
    let code = r#"
/// Return valid storage proof and state root.
///
/// Note: This should only be used for **testing**.
#[cfg(feature = "std")]
pub fn craft_valid_storage_proof(first: u32, second: u32) -> Result<u32, Error> {
    debug_assert!(first <= second);
    let _value = Some(first).unwrap();
    Ok(first + second)
}

pub fn production_debug_assert() {
    debug_assert!(external_input_is_valid());
}

pub fn production_panic() {
    let _value = Some(2u32).unwrap();
}

pub fn production_arithmetic(first: u32, second: u32) -> Result<u32, Error> {
    Ok(first + second)
}

/// Attestation data is only used after validation.
pub fn production_attestation_doc() {
    let _value = Some(3u32).unwrap();
}
"#;
    let diags = check_fixture("bridges/primitives/runtime/src/storage_proof.rs", code);
    let sec002_count = diags.iter().filter(|diag| diag.rule_id == "SEC002").count();
    let sec008_count = diags.iter().filter(|diag| diag.rule_id == "SEC008").count();
    let sec009_count = diags.iter().filter(|diag| diag.rule_id == "SEC009").count();

    assert_eq!(
        sec002_count, 1,
        "SEC002 should skip documented test-only items while reporting production code"
    );
    assert_eq!(
        sec008_count, 2,
        "SEC008 should skip documented test-only items without treating attestation as test"
    );
    assert_eq!(
        sec009_count, 1,
        "SEC009 should skip documented test-only items while reporting production code"
    );
}

#[test]
fn production_security_rules_skip_parent_cfg_gated_module_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("polkadot/runtime/common/src");
    std::fs::create_dir_all(&src).expect("create runtime src");
    std::fs::write(
        src.join("lib.rs"),
        r#"
#[cfg(feature = "try-runtime")]
pub mod try_runtime;

pub mod production;
"#,
    )
    .expect("write parent module");
    std::fs::write(
        src.join("try_runtime.rs"),
        r#"
pub fn try_runtime_only() {
    panic!("try-runtime assertion failed");
}
"#,
    )
    .expect("write cfg-gated module");
    std::fs::write(
        src.join("production.rs"),
        r#"
pub fn production_path() {
    panic!("production assertion failed");
}
"#,
    )
    .expect("write production module");

    let config = polkadot_linter::config::Config::default();
    let engine = polkadot_linter::engine::LintEngine::new(config);
    let diags = engine.scan(tmp.path());
    let sec008_files = diags
        .iter()
        .filter(|diag| diag.rule_id == "SEC008")
        .map(|diag| diag.file.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    assert!(
        sec008_files
            .iter()
            .any(|file| file.ends_with("production.rs")),
        "SEC008 should still report ungated production modules"
    );
    assert!(
        !sec008_files
            .iter()
            .any(|file| file.ends_with("try_runtime.rs")),
        "SEC008 should skip modules gated by a parent try-runtime cfg"
    );
}

#[test]
fn production_security_rules_skip_proc_macro_targets() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let crate_root = tmp
        .path()
        .join("substrate/frame/election-provider-support/solution-type");
    let src = crate_root.join("src");
    std::fs::create_dir_all(&src).expect("create proc macro src");
    std::fs::write(
        crate_root.join("Cargo.toml"),
        r#"
[package]
name = "frame-election-provider-solution-type"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true
"#,
    )
    .expect("write proc macro manifest");
    std::fs::write(
        src.join("lib.rs"),
        r#"
mod single_page;

pub fn parse_input() {
    panic!("proc macro parse error");
}
"#,
    )
    .expect("write proc macro lib");
    std::fs::write(
        src.join("single_page.rs"),
        r#"
pub(crate) fn generate() -> Result<(), ()> {
    let _ = Some(1u32).unwrap();
    Ok(())
}
"#,
    )
    .expect("write proc macro module");

    let config = polkadot_linter::config::Config::default();
    let engine = polkadot_linter::engine::LintEngine::new(config);
    let diags = engine.scan(&crate_root);
    assert!(
        !has_rule(&diags, "SEC008"),
        "production security rules should skip proc-macro target sources"
    );
    assert!(
        !has_rule(&diags, "SEC009"),
        "production security rules should skip proc-macro target module sources"
    );
}

#[test]
fn sec008_skips_runtime_benchmark_cfg_multiline_signatures() {
    let code = r#"
#[cfg(feature = "runtime-benchmarks")]
pub fn benchmark_setup(
) {
    let account = MaybeAccount::get().expect("benchmark account exists");
    let _ = account;
}

pub fn prod() {
    let value = Some(1u32).unwrap();
    let _ = value;
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip multiline runtime-benchmark items without masking following production code"
    );
}

#[test]
fn sec008_skips_try_runtime_cfg_blocks() {
    let code = r#"
#[cfg(feature = "try-runtime")]
pub fn pre_upgrade(
) {
    let state = Some(1u32).unwrap();
    let _ = state;
}

#[cfg(any(feature = "try-runtime", test))]
pub fn try_state() {
    panic!("try-runtime check failed");
}

pub fn prod() {
    let value = Some(2u32).unwrap();
    let _ = value;
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip try-runtime-only items without masking following production code"
    );
}

#[test]
fn sec008_skips_test_helpers_cfg_blocks() {
    let code = r#"
#[cfg(feature = "test-helpers")]
pub fn grow_storage_proof(
) {
    let value = Some(1u32).expect("helper state exists");
    let _ = value;
}

#[cfg(any(test, feature = "test-helpers"))]
pub fn helper_state() {
    panic!("test helper assertion failed");
}

pub fn prod() {
    let value = Some(2u32).unwrap();
    let _ = value;
}
"#;
    let diags = check_fixture("bridges/primitives/runtime/src/storage_proof.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip test-helper-only items without masking following production code"
    );
}

#[test]
fn sec008_does_not_skip_cfg_attr_doc_hidden_items() {
    let code = r#"
#[cfg_attr(not(any(test, feature = "test-helpers")), doc(hidden))]
pub fn production_path() {
    let value = Some(2u32).unwrap();
    let _ = value;
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC008"),
        "SEC008 should still report production items that only use cfg_attr for documentation"
    );
}

#[test]
fn sec008_skips_inline_test_functions() {
    let code = r#"
#[test]
fn unit_test() {
    let value = Some(1u32).unwrap();
    let _ = value;
}

#[tokio::test]
async fn async_unit_test() {
    panic!("test failure");
}

pub fn prod() {
    let value = Some(2u32).unwrap();
    let _ = value;
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip inline test functions without masking production code"
    );
}

#[test]
fn sec008_skips_nested_cfg_test_items_without_unmasking_module() {
    let code = r#"
#[cfg(test)]
mod tests {
    fn helper() {
        let value = Some(1u32).unwrap();
        let _ = value;
    }

    #[cfg(test)]
    fn nested_helper(
    ) {
        let value = Some(2u32).unwrap();
        let _ = value;
    }

    fn later_helper() {
        let value = Some(3u32).unwrap();
        let _ = value;
    }
}

pub fn prod() {
    let value = Some(4u32).unwrap();
    let _ = value;
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should keep cfg(test) modules masked after nested cfg(test) multiline items"
    );
}

#[test]
fn sec008_skips_sdk_support_benchmark_fixture_paths() {
    let bad = include_str!("fixtures/bad_sec008.rs");
    for path in [
        "polkadot/node/subsystem-bench/src/lib.rs",
        "substrate/frame/support/src/traits/tokens/conformance_tests/regular/mutate.rs",
        "substrate/frame/revive/fixtures/src/builder.rs",
        "substrate/client/foo/src/test_utils.rs",
        "substrate/bin/node/bench/src/construct.rs",
        "substrate/bin/node/testing/src/bench.rs",
        "substrate/frame/contracts/mock-network/src/lib.rs",
        "substrate/frame/bags-list/remote-tests/src/lib.rs",
        "cumulus/pallets/session-benchmarking/src/inner.rs",
        "substrate/frame/election-provider-multi-phase/test-staking-e2e/src/lib.rs",
        "substrate/frame/election-provider-multi-phase/src/remote_mining.rs",
        "bridges/snowbridge/runtime/test-common/src/lib.rs",
        "polkadot/runtime/test-runtime/src/xcm_config.rs",
        "substrate/primitives/runtime/src/testing.rs",
        "substrate/frame/revive/rpc/build.rs",
        "substrate/frame/revive/ui-tests/src/ui/precompiles_ui.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(
            !has_rule(&diags, "SEC008"),
            "SEC008 should skip SDK support path {path}"
        );
    }
}

#[test]
fn sec008_skips_non_runtime_utility_crates() {
    let bad = include_str!("fixtures/bad_sec008.rs");
    for path in [
        "substrate/primitives/io/src/lib.rs",
        "substrate/utils/wasm-builder/src/wasm_project.rs",
        "polkadot/node/core/pvf/common/src/executor_interface.rs",
        "substrate/client/state-db/src/lib.rs",
        "substrate/scripts/ci/node-template-release/src/main.rs",
        "polkadot/node/metrics/src/runtime/mod.rs",
        "substrate/frame/support/procedural/src/pallet/parse/tasks.rs",
        "substrate/frame/examples/kitchensink/src/lib.rs",
        "cumulus/pallets/parachain-system/proc-macro/src/lib.rs",
        "substrate/frame/revive/rpc/src/cli.rs",
        "substrate/frame/revive/uapi/src/host/riscv64.rs",
        "substrate/frame/revive/dev-node/node/src/service.rs",
        "substrate/frame/staking/reward-curve/src/lib.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(
            !has_rule(&diags, "SEC008"),
            "SEC008 should skip non-runtime utility path {path}"
        );
    }
}

#[test]
fn sec008_skips_documented_benchmark_test_builder_files() {
    let code = r#"
/// This is directly from frame-benchmarking so it can be used in benchmarks and tests.
/// Paras inherent `enter` benchmark scenario builder.
pub(crate) struct BenchBuilder;

pub fn build_scenario() {
    let _value = Some(1u32).unwrap();
}
"#;
    let diags = check_fixture("polkadot/runtime/parachains/src/builder.rs", code);
    assert!(
        !has_rule(&diags, "SEC008"),
        "SEC008 should skip documented benchmark/test builder helpers"
    );
}

#[test]
fn sec008_still_lints_runtime_and_pallet_paths() {
    let bad = include_str!("fixtures/bad_sec008.rs");
    for path in [
        "polkadot/runtime/parachains/src/builder.rs",
        "substrate/frame/contracts/src/lib.rs",
        "substrate/bin/node/runtime/src/lib.rs",
        "substrate/frame/revive/dev-node/runtime/src/lib.rs",
        "bridges/modules/grandpa/src/lib.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(
            has_rule(&diags, "SEC008"),
            "SEC008 should still lint runtime/pallet path {path}"
        );
    }
}

#[test]
fn sec008_does_not_skip_production_after_cfg_test_use() {
    let code = r#"
#[cfg(test)]
use crate::mock_helpers::*;

pub fn prod() {
    let value = Some(1u32).unwrap();
    let _ = value;
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC008"),
        "SEC008 should still lint production code after a cfg(test) single-line item"
    );
}

#[test]
fn sec008_lints_std_gated_production_code() {
    let code = r#"
#[cfg(feature = "std")]
pub fn native_only_helper() {
    panic!("still production code");
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC008"),
        "SEC008 should lint std-gated production code instead of treating it like test code"
    );
}

#[test]
fn sec008_ignores_string_literal_mentions() {
    let code = r#"
pub fn docs() {
    let _help = ".unwrap() and panic!() are forbidden";
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC008"),
        "SEC008 should ignore panic-capable patterns that only appear inside string literals"
    );
}

#[test]
fn sec008_reports_once_per_source_line() {
    let code = r#"
pub fn same_line() {
    let _values = (Some(1u32).unwrap(), Some(2u32).expect("external input"));
    panic!("external input");
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 2,
        "SEC008 should report one panic-capable finding per affected source line"
    );
}

#[test]
fn sec008_skips_qed_expect_messages_but_reports_other_expects() {
    let code = r#"
pub fn proven_invariant() {
    let value = Some(1u32).expect("checked immediately above; qed");
    let _ = value;
}

pub fn proven_invariant_with_variable() {
    let qed = "slice was checked before conversion; qed";
    let value = Some(2u32).expect(qed);
    let _ = value;
}

pub fn fallible_input() {
    let value = Some(3u32).expect("external input must be valid");
    let _ = value;
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip qed-marked expect invariants while reporting normal expects"
    );
}

#[test]
fn sec008_skips_qed_panic_macros_but_reports_other_panics() {
    let code = r#"
pub fn proven_invariant() {
    panic!("serialized properly; qed");
}

pub fn proven_unreachable() {
    unreachable!("state machine cannot reach this branch; qed");
}

pub fn fallible_input() {
    panic!("external input is invalid");
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip qed-marked panic invariants while reporting normal panics"
    );
}

#[test]
fn sec008_skips_unreachable_in_uninhabited_enum_impls() {
    let code = r#"
pub trait Pipeline {
    fn validate_only(&self);
}

/// This type cannot be instantiated.
pub enum InvalidVersion {}

impl Pipeline for InvalidVersion {
    fn validate_only(&self) {
        unreachable!()
    }
}

pub enum ReachableVersion {
    Current,
}

impl Pipeline for ReachableVersion {
    fn validate_only(&self) {
        unreachable!()
    }
}
"#;
    let diags = check_fixture(
        "substrate/primitives/runtime/src/traits/vers_tx_ext/invalid.rs",
        code,
    );
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip unreachable impls for uninhabited enums while reporting reachable impls"
    );
}

#[test]
fn sec008_skips_nonzero_literal_new_unwraps() {
    let code = r#"
use core::num::NonZero;

const HEX: NonZero<u16> = NonZero::new(0x0902).unwrap();
const DECIMAL: NonZero<u32> = core::num::NonZero::new(42u32).unwrap();

pub fn report_zero_literal() {
    let _ = NonZero::new(0).unwrap();
}

pub fn report_variable(value: u16) {
    let _ = NonZero::new(value).unwrap();
}

pub fn report_other_unwrap() {
    let _ = Some(1u32).unwrap();
}
"#;
    let diags = check_fixture(
        "substrate/frame/revive/src/precompiles/builtin/sha256.rs",
        code,
    );
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 3,
        "SEC008 should skip only NonZero::new(nonzero-literal).unwrap()"
    );
}

#[test]
fn sec008_skips_bounded_vec_try_from_vec_literals_with_static_capacity() {
    let code = r#"
const MAX_ITEMS: u32 = 2;

pub fn bounded_literal(value: Item, other: Item, runtime_values: Vec<Item>) {
    let _ = BoundedVec::<_, ConstU32<MAX_ITEMS>>::try_from(vec![value]).expect("MAX_ITEMS >= 1");
    let _ = BoundedVec::<_, ConstU32<2>>::try_from(vec![value, other]).unwrap();
    let _ = BoundedVec::<_, ConstU32<1>>::try_from(vec![value, other]).unwrap();
    let _ = BoundedVec::<_, ConstU32<MAX_ITEMS>>::try_from(runtime_values).unwrap();
}
"#;
    let diags = check_fixture("substrate/frame/babe/src/lib.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 2,
        "SEC008 should skip only BoundedVec::try_from(vec![...]) when literal length fits the static bound"
    );
}

#[test]
fn sec008_skips_single_item_vec_try_into_with_min_one_bound_message() {
    let code = r#"
pub fn self_vote(voter: AccountId, other: AccountId) {
    let _safe: BoundedVec<AccountId, MaxVotesPerVoter> = vec![voter.clone()]
        .try_into()
        .expect("`MaxVotesPerVoter` must be greater than or equal to 1");

    let _too_many: BoundedVec<AccountId, MaxVotesPerVoter> = vec![voter.clone(), other]
        .try_into()
        .expect("`MaxVotesPerVoter` must be greater than or equal to 1");

    let _wrong_message: BoundedVec<AccountId, MaxVotesPerVoter> = vec![voter.clone()]
        .try_into()
        .expect("external input should fit");

    let _unwrap: BoundedVec<AccountId, MaxVotesPerVoter> = vec![voter]
        .try_into()
        .unwrap();
}
"#;
    let diags = check_fixture("substrate/frame/staking/src/pallet/impls.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 3,
        "SEC008 should skip only one-item vec literal try_into expects with an explicit min-one bound message"
    );
}

#[test]
fn sec008_skips_fixed_range_try_into_unwraps() {
    let code = r#"
use sp_core::H160;

pub fn fixed_ranges(address: &H160, data: &[u8], offset: usize, start: usize, runtime_end: usize) {
    let _word: &[u8; 32] = data[offset..offset + 32].try_into().unwrap();
    let _prefix: &[u8; 4] = data[..4].try_into().expect("fixed prefix");
    let _middle: &[u8; 8] = data[12..20].try_into().unwrap();
    let _account = u64::from_be_bytes(address.as_ref()[12..].try_into().unwrap());

    let _open: &[u8; 8] = data[12..].try_into().unwrap();
    let _untyped_suffix = u64::from_be_bytes(data[12..].try_into().unwrap());
    let _runtime: &[u8; 8] = data[start..runtime_end].try_into().unwrap();
}
"#;
    let diags = check_fixture("substrate/frame/revive/src/vm/evm/memory.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 3,
        "SEC008 should skip only try_into unwraps whose slice length is statically known"
    );
}

#[test]
fn sec008_skips_biguint_to_usize_after_explicit_upper_bound() {
    let code = r#"
use num_bigint::BigUint;

pub fn guarded_expect(input: &[u8]) -> DispatchResult {
    let max_size_big = BigUint::from(1024u32);
    let len_big = BigUint::from_bytes_be(input);
    if len_big > max_size_big {
        Err(DispatchError::from("too large"))?;
    }
    let _len = len_big.to_usize().expect("bounds checked above");
    Ok(())
}

pub fn guarded_unwrap(input: &[u8]) -> DispatchResult {
    let len_big = BigUint::from_bytes_be(input);
    if len_big >= BigUint::from(1025u32) {
        return Err(DispatchError::from("too large"));
    }
    let _len = len_big.to_usize().unwrap();
    Ok(())
}

pub fn unguarded(input: &[u8]) -> DispatchResult {
    let len_big = BigUint::from_bytes_be(input);
    let _len = len_big.to_usize().expect("external input");
    Ok(())
}

pub fn guarded_without_reject(input: &[u8]) -> DispatchResult {
    let max_size_big = BigUint::from(1024u32);
    let len_big = BigUint::from_bytes_be(input);
    if len_big > max_size_big {
        log::warn!("too large");
    }
    let _len = len_big.to_usize().unwrap();
    Ok(())
}
"#;
    let diags = check_fixture(
        "substrate/frame/revive/src/precompiles/builtin/modexp.rs",
        code,
    );
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 2,
        "SEC008 should skip only BigUint conversions whose accepted path has an explicit usize-safe upper bound"
    );
}

#[test]
fn sec008_skips_test_default_config_impls() {
    let code = r#"
pub struct TestDefaultConfig;

impl Randomness<Output, BlockNumber> for TestDefaultConfig {
    fn random(_subject: &[u8]) -> (Output, BlockNumber) {
        unimplemented!("No default random implementation in TestDefaultConfig")
    }
}

impl Time for TestDefaultConfig {
    type Moment = u64;
    fn now() -> Self::Moment {
        unimplemented!("No default time implementation in TestDefaultConfig")
    }
}

pub struct ProductionConfig;

impl Time for ProductionConfig {
    type Moment = u64;
    fn now() -> Self::Moment {
        unimplemented!("No production time implementation")
    }
}
"#;
    let diags = check_fixture("substrate/frame/contracts/src/lib.rs", code);
    let sec008_count = diags.iter().filter(|d| d.rule_id == "SEC008").count();
    assert_eq!(
        sec008_count, 1,
        "SEC008 should skip TestDefaultConfig impl panics while reporting normal impls"
    );
}

// ==========================================================================
// SEC009: Raw arithmetic in fallible functions
// ==========================================================================
#[test]
fn sec009_detects_raw_arithmetic() {
    let bad = include_str!("fixtures/bad_sec009.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC009"),
        "SEC009 should fire on raw + * in function returning Result"
    );
}

#[test]
fn sec009_allows_saturating_arithmetic() {
    let good = include_str!("fixtures/good_sec009.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC009"),
        "SEC009 should NOT fire on saturating/checked arithmetic"
    );
}

#[test]
fn sec009_skips_overloaded_group_arithmetic_in_curve_conversions() {
    let code = r#"
pub fn verify_point(p1: G1, p2: G1, scalar: Fr, a: u32, b: u32) -> Result<u32, Error> {
    let _sum = AffineG1::from_jacobian(p1 + p2);
    let _mul = AffineG1::from_jacobian(p1 * scalar);
    let total = a + b;
    Ok(total)
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec009_count = diags.iter().filter(|d| d.rule_id == "SEC009").count();
    assert_eq!(
        sec009_count, 1,
        "SEC009 should skip overloaded curve arithmetic while still reporting integer arithmetic"
    );
}

#[test]
fn sec009_skips_nonnegative_max_min_differences() {
    let code = r#"
pub fn rebalance_delta(current: Balance, target: Balance, raw: Balance) -> Result<Balance, Error> {
    let delta = current.max(target) - current.min(target);
    let reversed = target.max(current) - current.min(target);
    let unsafe_delta = raw - target;
    Ok(delta.saturating_add(reversed).saturating_add(unsafe_delta))
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec009_count = diags.iter().filter(|d| d.rule_id == "SEC009").count();
    assert_eq!(
        sec009_count, 1,
        "SEC009 should skip provably nonnegative max-min differences while reporting raw subtraction"
    );
}

#[test]
fn sec009_skips_branch_ordered_deposit_deltas() {
    let code = r#"
pub fn update_deposit(old_deposit: Balance, new_deposit: Balance, raw: Balance) -> DispatchResult {
    if old_deposit < new_deposit {
        T::Currency::reserve(&sender, new_deposit - old_deposit)?;
    } else if old_deposit > new_deposit {
        T::Currency::unreserve(&sender, old_deposit - new_deposit);
    }

    if current >= target {
        let _delta = current - target;
    }

    if new_len > old_len {
        diff.bytes_added = new_len - old_len;
    } else {
        diff.bytes_removed = old_len - new_len;
    }

    let unsafe_delta = raw - new_deposit;
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/identity/src/lib.rs", code);
    let sec009_count = diags.iter().filter(|d| d.rule_id == "SEC009").count();
    assert_eq!(
        sec009_count, 1,
        "SEC009 should skip branch-ordered nonnegative deltas while reporting unguarded subtraction"
    );
}

#[test]
fn sec009_skips_method_ordered_subtractions() {
    let code = r#"
pub fn extract_chain_id(v: U256, max_amount: Balance, spot_price: Balance, raw: Balance) -> Result<Option<U256>, Error> {
    let chain_id = if v.ge(&35u32.into()) {
        Some((v - 35) / 2)
    } else {
        None
    };

    if spot_price.le(&max_amount) {
        let _slack = max_amount - spot_price;
    } else {
        let _excess = spot_price - max_amount;
    }

    let unsafe_delta = raw - max_amount;
    Ok(chain_id)
}
"#;
    let diags = check_fixture("substrate/frame/revive/src/evm/api/rlp_codec.rs", code);
    let sec009_count = diags.iter().filter(|d| d.rule_id == "SEC009").count();
    assert_eq!(
        sec009_count, 1,
        "SEC009 should skip method-ordered deltas while reporting unguarded subtraction"
    );
}

#[test]
fn sec009_skips_cmp_match_ordered_subtractions() {
    let code = r#"
pub fn external_to_internal(
    amount: Balance,
    ext_decimals: u8,
    internal_decimals: u8,
    raw: u8,
) -> Result<Balance, Error> {
    use core::cmp::Ordering::*;
    let scaled = match ext_decimals.cmp(&internal_decimals) {
        Equal => amount,
        Less => {
            let diff = (internal_decimals - ext_decimals) as u32;
            amount.checked_mul(pow10(diff)?).ok_or(Error::Overflow)?
        },
        Greater => {
            let diff = (ext_decimals - internal_decimals) as u32;
            amount.checked_div(pow10(diff)?).unwrap_or_default()
        },
    };
    let unsafe_delta = raw - ext_decimals;
    Ok(scaled)
}
"#;
    let diags = check_fixture("substrate/frame/psm/src/lib.rs", code);
    let sec009_count = diags.iter().filter(|d| d.rule_id == "SEC009").count();
    assert_eq!(
        sec009_count, 1,
        "SEC009 should skip cmp-match deltas while still reporting unrelated arithmetic"
    );
}

#[test]
fn sec009_detects_multiline_fallible_signature() {
    let code = r#"
pub fn calculate_share(
    total: u128,
    count: u32,
) -> Result<u128, Error> {
    let per_member = total * count as u128;
    Ok(per_member)
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(has_rule(&diags, "SEC009"), "SEC009 should detect raw arithmetic when the return type is declared on a multi-line signature");
}

#[test]
fn sec009_detects_dispatch_result_alias() {
    let code = r#"
pub fn timeout(now: u32, since: u32, timeout: u32) -> DispatchResultWithPostInfo {
    ensure!(now > since + timeout, Error::<T>::TooEarly);
    Ok(().into())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC009"),
        "SEC009 should detect raw arithmetic in fallible return aliases like DispatchResultWithPostInfo"
    );
}

#[test]
fn sec009_reports_once_per_source_line() {
    let code = r#"
pub fn calculate(first: u32, second: u32, third: u32) -> Result<u32, Error> {
    let nested = (first + second) * third;
    let separate = first + second;
    Ok(nested + separate)
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sec009_count = diags.iter().filter(|d| d.rule_id == "SEC009").count();
    assert_eq!(
        sec009_count, 3,
        "SEC009 should report one finding per affected source line"
    );
}

#[test]
fn sec009_ignores_ensure_without_arithmetic() {
    let code = r#"
pub fn validate<T>(first_alias: &u32, value: u32) -> DispatchResultWithPostInfo {
    ensure!(SomeMap::<T>::contains_key((value, *first_alias)), Error::<T>::Missing);
    ensure!(value >= 1, Error::<T>::TooSmall);
    ensure!(value == 1, "proof-of-ink count mismatch");
    Ok(().into())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC009"),
        "SEC009 should NOT fire on ensure! checks that only use deref or comparisons"
    );
}

#[test]
fn sec009_skips_non_runtime_utility_crates() {
    let bad = include_str!("fixtures/bad_sec009.rs");
    for path in [
        "substrate/client/db/src/lib.rs",
        "substrate/utils/fork-tree/src/lib.rs",
        "polkadot/node/core/approval-voting/src/lib.rs",
        "docs/sdk/packages/guides/first-pallet/src/lib.rs",
        "substrate/frame/revive/rpc/src/cli.rs",
        "substrate/frame/revive/uapi/src/host/riscv64.rs",
        "substrate/frame/staking-async/rc-client/src/lib.rs",
        "substrate/frame/asset-conversion/ops/src/lib.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(
            !has_rule(&diags, "SEC009"),
            "SEC009 should skip non-runtime utility path {path}"
        );
    }
}

#[test]
fn sec009_skips_documented_benchmark_test_builder_files() {
    let code = r#"
/// This is directly from frame-benchmarking so it can be used in benchmarks and tests.
/// Paras inherent `enter` benchmark scenario builder.
pub(crate) struct BenchBuilder;

pub fn build_scenario(a: u32, b: u32) -> Result<u32, ()> {
    Ok(a + b)
}
"#;
    let diags = check_fixture("polkadot/runtime/parachains/src/builder.rs", code);
    assert!(
        !has_rule(&diags, "SEC009"),
        "SEC009 should skip documented benchmark/test builder helpers"
    );
}

#[test]
fn sec009_still_lints_runtime_and_pallet_paths() {
    let bad = include_str!("fixtures/bad_sec009.rs");
    for path in [
        "polkadot/runtime/parachains/src/builder.rs",
        "substrate/frame/staking/src/pallet/impls.rs",
        "cumulus/pallets/xcmp-queue/src/lib.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(
            has_rule(&diags, "SEC009"),
            "SEC009 should still lint runtime/pallet path {path}"
        );
    }
}

// ==========================================================================
// VAL003: Storage write before validation
// ==========================================================================
#[test]
fn val003_detects_write_before_ensure() {
    let bad = include_str!("fixtures/bad_val003.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "VAL003"),
        "VAL003 should fire on storage write before ensure!"
    );
}

#[test]
fn val003_allows_validation_first() {
    let good = include_str!("fixtures/good_val003.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "VAL003"),
        "VAL003 should NOT fire when ensure! comes before write"
    );
}

#[test]
fn val003_ignores_try_mutate_and_try_append() {
    let code = r#"
pub fn update_config(origin: OriginFor<T>, key: u32, item: u32) -> DispatchResult {
    Items::<T>::try_mutate(key, |values| -> DispatchResult {
        values.try_append(item).map_err(|_| Error::<T>::TooManyItems)?;
        Ok(())
    })?;
    ensure_signed(origin)?;
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL003"),
        "VAL003 should not treat try_mutate/try_append as unconditional writes"
    );
}

// ==========================================================================
// SEC010: Missing transactional in hook
// ==========================================================================
#[test]
fn sec010_detects_hook_without_transactional() {
    let bad = include_str!("fixtures/bad_sec010.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC010"),
        "SEC010 should fire on on_poll with multiple writes and no with_storage_layer"
    );
}

#[test]
fn sec010_allows_transactional_hook() {
    let good = include_str!("fixtures/good_sec010.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC010"),
        "SEC010 should NOT fire when with_storage_layer is used"
    );
}

#[test]
fn sec010_allows_transactional_attribute_near_hook_signature() {
    let code = r#"
impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    #[transactional]
    fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
        ProcessedCount::<T>::put(1);
        PendingItems::<T>::kill();
        Weight::zero()
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC010"),
        "SEC010 should honor #[transactional] when it appears above the hook signature"
    );
}

#[test]
fn sec010_allows_deterministic_multi_write_hooks() {
    let code = r#"
impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
        ProcessedCount::<T>::put(1);
        PendingItems::<T>::kill();
        Weight::zero()
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC010"),
        "SEC010 should not require transactional storage for infallible maintenance writes"
    );
}

// ==========================================================================
// SEC011: Storage iteration in dispatchables/hooks
// ==========================================================================
#[test]
fn sec011_detects_storage_iteration() {
    let bad = include_str!("fixtures/bad_sec011.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC011"),
        "SEC011 should fire on storage iteration in a dispatchable"
    );
}

#[test]
fn sec011_allows_bounded_access_patterns() {
    let good = include_str!("fixtures/good_sec011.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC011"),
        "SEC011 should NOT fire on bounded storage access"
    );
}

#[test]
fn sec011_allows_in_memory_iteration() {
    let good = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn submit(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        let split_into = vec![1u32, 2u32, 3u32];
        let _sum: u32 = split_into.iter().copied().sum();
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC011"),
        "SEC011 should NOT fire on iteration over in-memory collections"
    );
}

#[test]
fn sec011_allows_privileged_storage_iteration() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn cancel(origin: OriginFor<T>) -> DispatchResult {
        ensure_root(origin)?;
        for (_account, balance) in Accounts::<T>::drain() {
            Total::<T>::put(balance);
        }
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC011"),
        "SEC011 should not report storage iteration in root-only maintenance dispatchables"
    );
}

#[test]
fn sec011_allows_weight_meter_bounded_hook_iteration() {
    let code = r#"
impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
        let mut meter = WeightMeter::from_limit(remaining_weight);
        if meter.try_consume(T::DbWeight::get().reads(1)).is_err() {
            return meter.consumed();
        }

        let accounts: Vec<_> = Accounts::<T>::iter().take(T::MaxPerBlock::get() as usize).collect();
        for (account, _) in accounts {
            if meter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() {
                break;
            }
            Accounts::<T>::remove(account);
        }
        meter.consumed()
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC011"),
        "SEC011 should allow hook iteration that is capped by take() and guarded by a WeightMeter"
    );
}

// ==========================================================================
// SEC012: Unbounded clear_prefix
// ==========================================================================
#[test]
fn sec012_detects_unbounded_clear_prefix() {
    let bad = include_str!("fixtures/bad_sec012.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC012"),
        "SEC012 should fire on clear_prefix with None/u32::MAX"
    );
}

#[test]
fn sec012_allows_bounded_clear_prefix() {
    let good = include_str!("fixtures/good_sec012.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC012"),
        "SEC012 should NOT fire on bounded clear_prefix calls"
    );
}

#[test]
fn sec012_allows_documented_unbounded_clear_with_static_page_bound() {
    let code = r#"
pub fn take_submission_with_data(round: u32, who: &T::AccountId) {
    // NOTE: safe to remove unbounded, as at most `Pages` pages are stored.
    let r = SubmissionStorage::<T>::clear_prefix((round, who), u32::MAX, None);
    debug_assert!(r.unique <= T::Pages::get());
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC012"),
        "SEC012 should allow documented unbounded clears when a static page bound is asserted"
    );
}

#[test]
fn sec012_reports_comments_that_admit_unbounded_clear_needs_fixing() {
    let code = r#"
pub fn clear_era_information(era_index: EraIndex) {
    // FIXME: We can possibly set a reasonable limit since we do this only once per era.
    let mut cursor = ErasStakers::<T>::clear_prefix(era_index, u32::MAX, None);
    debug_assert!(cursor.maybe_cursor.is_none());
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC012"),
        "SEC012 should still report comments that acknowledge the clear should be bounded"
    );
}

#[test]
fn sec012_skips_frame_support_migration_helpers() {
    let bad = include_str!("fixtures/bad_sec012.rs");
    for path in [
        "substrate/frame/support/src/migrations.rs",
        "substrate/frame/support/src/storage/migration.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(
            !has_rule(&diags, "SEC012"),
            "SEC012 should skip FRAME support migration helper path {path}"
        );
    }
}

#[test]
fn sec012_still_checks_real_pallet_migration_files() {
    let bad = include_str!("fixtures/bad_sec012.rs");
    let diags = check_fixture("substrate/frame/staking/src/pallet/impls.rs", bad);
    assert!(
        has_rule(&diags, "SEC012"),
        "SEC012 should still report unbounded clear_prefix in real pallet code"
    );
}

// ==========================================================================
// MOK001: Excessive mock setup
// ==========================================================================
#[test]
fn mok001_detects_mock_heavy_test_setup() {
    let bad = include_str!("fixtures/bad_mok001.rs");
    let diags = check_fixture("tests/mock_usage.rs", bad);
    assert!(
        has_rule(&diags, "MOK001"),
        "MOK001 should fire on mock-heavy tests"
    );
}

#[test]
fn mok001_allows_outcome_focused_tests() {
    let good = include_str!("fixtures/good_mok001.rs");
    let diags = check_fixture("tests/mock_usage.rs", good);
    assert!(
        !has_rule(&diags, "MOK001"),
        "MOK001 should not fire on outcome-focused tests"
    );
}

// ==========================================================================
// SEC013: Unbounded storage collections
// ==========================================================================
#[test]
fn sec013_detects_unbounded_storage_collection() {
    let bad = include_str!("fixtures/bad_sec013.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC013"),
        "SEC013 should fire on Vec/BTreeMap storage without #[pallet::unbounded]"
    );
}

#[test]
fn sec013_allows_explicit_unbounded_annotation() {
    let good = include_str!("fixtures/good_sec013.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC013"),
        "SEC013 should NOT fire when #[pallet::unbounded] is present"
    );
}

#[test]
fn sec013_allows_bounded_storage_wrappers() {
    let code = r#"
#[pallet::storage]
pub type BoundedBytes<T: Config> =
    StorageValue<_, BoundedVec<Vec<u8>, T::MaxItems>, ValueQuery>;

#[pallet::storage]
pub type WeakBoundedBytes<T: Config> =
    StorageMap<_, Blake2_128Concat, T::AccountId, WeakBoundedVec<Vec<u8>, T::MaxItems>, ValueQuery>;

#[pallet::storage]
pub type BoundedMap<T: Config> =
    StorageValue<_, BoundedBTreeMap<T::AccountId, Vec<u8>, T::MaxItems>, ValueQuery>;
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC013"),
        "SEC013 should NOT fire when raw collection types are inside bounded storage wrappers"
    );
}

#[test]
fn sec013_allows_documented_capacity_limited_storage_collections() {
    let code = r#"
/// Latest included block descendants accepted by the runtime.
///
/// The segment length is limited by the capacity returned from the configured consensus hook.
#[pallet::storage]
pub type UnincludedSegment<T: Config> = StorageValue<_, Vec<Ancestor<T::Hash>>, ValueQuery>;
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC013"),
        "SEC013 should allow storage collections documented as capacity-limited"
    );
}

#[test]
fn sec013_reports_docs_that_admit_no_global_bound() {
    let code = r#"
// NOTE: could become bounded, but we don't have a global maximum for this.
#[pallet::storage]
pub type HrmpOpenChannelRequestsList<T: Config> = StorageValue<_, Vec<HrmpChannelId>, ValueQuery>;
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC013"),
        "SEC013 should report storage collections whose docs admit no global bound"
    );
}

#[test]
fn sec013_allows_pallet_dev_mode_storage() {
    let code = r#"
#[frame_support::pallet(dev_mode)]
pub mod pallet {
    #[pallet::storage]
    pub type Dummy<T: Config> = StorageValue<_, Vec<T::AccountId>>;
}
"#;
    let diags = check_fixture("substrate/frame/examples/dev-mode/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC013"),
        "SEC013 should NOT fire on pallet dev_mode storage"
    );
}

#[test]
fn security_rules_skip_mock_harness_paths() {
    let bad = include_str!("fixtures/bad_sec013.rs");
    let diags = check_fixture(
        "substrate/frame/contracts/mock-network/src/mocks/msg_queue.rs",
        bad,
    );
    assert!(
        !has_rule(&diags, "SEC013"),
        "production security rules should skip mock harness paths"
    );
}

#[test]
fn security_rules_skip_sdk_test_support_pallets() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(0)]
    pub fn helper(origin: OriginFor<T>, data: Vec<u8>) -> DispatchResult {
        ensure_root(origin)?;
        let _ = data;
        Ok(())
    }
}
"#;
    for path in [
        "substrate/frame/root-offences/src/lib.rs",
        "cumulus/parachains/pallets/ping/src/lib.rs",
    ] {
        let diags = check_fixture(path, code);
        assert!(
            !has_rule(&diags, "SEC018"),
            "production security rules should skip SDK test-support pallet path {path}"
        );
    }
}

// ==========================================================================
// SEC014: Identity hasher on common key types
// ==========================================================================
#[test]
fn sec014_detects_identity_hasher_on_account_id() {
    let bad = include_str!("fixtures/bad_sec014.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC014"),
        "SEC014 should fire on Identity hasher with AccountId/u32/u64/Balance keys"
    );
}

#[test]
fn sec014_allows_non_identity_hashers() {
    let good = include_str!("fixtures/good_sec014.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC014"),
        "SEC014 should NOT fire on Blake2_128Concat"
    );
}

#[test]
fn sec014_only_checks_keys_paired_with_identity_hashers() {
    let value_only_common_types = r#"
#[pallet::storage]
pub type HashToCount<T: Config> = StorageMap<_, Identity, T::Hash, u32, ValueQuery>;

#[pallet::storage]
pub type HashToAccount<T: Config> =
    StorageMap<_, Identity, T::Hash, (T::AccountId, BalanceOf<T>), ValueQuery>;
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", value_only_common_types);
    assert!(
        !has_rule(&diags, "SEC014"),
        "SEC014 should not treat StorageMap value types as identity-hashed keys"
    );

    let double_map_key = r#"
#[pallet::storage]
pub type HashAndIndex<T: Config> =
    StorageDoubleMap<_, Identity, T::Hash, Identity, u32, (), ValueQuery>;
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", double_map_key);
    assert!(
        has_rule(&diags, "SEC014"),
        "SEC014 should still report identity-hashed common keys in StorageDoubleMap"
    );
}

#[test]
fn sec014_allows_documented_internal_numeric_layout_keys() {
    let code = r#"
/// Ring buffer containing imported block numbers, ordered by insertion time.
#[pallet::storage]
pub type ImportedBlockNumbers<T: Config> = StorageMap<_, Identity, u32, T::BlockNumber>;

/// Mapping an asset index derived from the precompile address to an asset id.
#[pallet::storage]
pub type AssetIndexToAssetId<T: Config> = StorageMap<_, Identity, u32, T::AssetId>;

/// Next epoch unsorted ticket segments.
#[pallet::storage]
pub type UnsortedSegments<T: Config> =
    StorageMap<_, Identity, u32, BoundedVec<T::TicketId, T::MaxSegments>, ValueQuery>;
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC014"),
        "SEC014 should allow documented numeric identity keys used as internal indexes"
    );
}

#[test]
fn sec014_still_reports_undocumented_numeric_identity_keys() {
    let code = r#"
#[pallet::storage]
pub type UserScores<T: Config> = StorageMap<_, Identity, u32, BalanceOf<T>, ValueQuery>;
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC014"),
        "SEC014 should still report generic numeric identity keys without internal-layout docs"
    );
}

// ==========================================================================
// SEC015: dispatch_bypass_filter in production
// ==========================================================================
#[test]
fn sec015_detects_dispatch_bypass_filter() {
    let bad = include_str!("fixtures/bad_sec015.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC015"),
        "SEC015 should fire on dispatch_bypass_filter in production code"
    );
}

#[test]
fn sec015_allows_normal_dispatch() {
    let good = include_str!("fixtures/good_sec015.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC015"),
        "SEC015 should NOT fire on normal dispatch"
    );
}

#[test]
fn sec015_allows_strict_root_guarded_bypass() {
    let code = r#"
pub fn dispatch_as_root(origin: OriginFor<T>, call: Box<T::RuntimeCall>) -> DispatchResult {
    ensure_root(origin)?;
    call.dispatch_bypass_filter(frame_system::RawOrigin::Root.into()).map_err(|e| e.error)?;
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC015"),
        "SEC015 should allow dispatch_bypass_filter after a strict ensure_root guard"
    );
}

#[test]
fn sec015_allows_bypass_inside_verified_root_branch() {
    let code = r#"
pub fn batch(origin: OriginFor<T>, call: T::RuntimeCall) -> DispatchResult {
    let is_root = ensure_root(origin.clone()).is_ok();
    let result = if is_root {
        call.dispatch_bypass_filter(origin.clone())
    } else {
        call.dispatch(origin.clone())
    };
    result.map(|_| ()).map_err(|e| e.error)
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC015"),
        "SEC015 should allow bypass only inside branches gated by ensure_root(...).is_ok()"
    );
}

#[test]
fn sec015_reports_unverified_root_flag_bypass() {
    let code = r#"
pub fn batch(origin: OriginFor<T>, call: T::RuntimeCall, is_root: bool) -> DispatchResult {
    let result = if is_root {
        call.dispatch_bypass_filter(origin.clone())
    } else {
        call.dispatch(origin.clone())
    };
    result.map(|_| ()).map_err(|e| e.error)
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC015"),
        "SEC015 should not trust arbitrary variables named is_root"
    );
}

// ==========================================================================
// SEC016: Missing StorageVersion check in runtime upgrade
// ==========================================================================
#[test]
fn sec016_detects_runtime_upgrade_without_storage_version_gate() {
    let bad = include_str!("fixtures/bad_sec016.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC016"),
        "SEC016 should fire on on_runtime_upgrade writes without StorageVersion checks"
    );
}

#[test]
fn sec016_allows_storage_version_guarded_runtime_upgrade() {
    let good = include_str!("fixtures/good_sec016.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC016"),
        "SEC016 should NOT fire when on_runtime_upgrade checks StorageVersion"
    );
}

#[test]
fn sec016_allows_documented_idempotent_migrations() {
    let code = r#"
/// Idempotent migration to initialize pallet parameters.
///
/// Safe to run multiple times -- existing values are not overwritten.
pub struct InitializeParams<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for InitializeParams<T> {
    fn on_runtime_upgrade() -> Weight {
        if !MaxDebt::<T>::exists() {
            MaxDebt::<T>::put(Permill::from_percent(50));
        }
        T::DbWeight::get().reads_writes(1, 1)
    }
}

/// Set `NextAssetId` if it does not exist yet.
pub struct SetNextAssetId<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for SetNextAssetId<T> {
    fn on_runtime_upgrade() -> Weight {
        if !NextAssetId::<T>::exists() {
            NextAssetId::<T>::put(0u32);
        }
        T::DbWeight::get().reads_writes(1, 1)
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/migration.rs", code);
    assert!(
        !has_rule(&diags, "SEC016"),
        "SEC016 should allow migrations documented as idempotent or no-op when storage exists"
    );
}

#[test]
fn sec016_allows_documented_current_value_reconciliation() {
    let code = r#"
/// Checks and updates `TotalValueLocked` if out of sync.
pub struct TotalValueLockedSync<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for TotalValueLockedSync<T> {
    fn on_runtime_upgrade() -> Weight {
        let expected = Self::calculate_tvl();
        let current = TotalValueLocked::<T>::get();
        if expected != current {
            TotalValueLocked::<T>::set(expected);
        }
        T::DbWeight::get().reads_writes(1, 1)
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/migration.rs", code);
    assert!(
        !has_rule(&diags, "SEC016"),
        "SEC016 should allow migrations documented as current-value reconciliation"
    );
}

#[test]
fn sec016_allows_structural_current_value_reconciliation() {
    let code = r#"
impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_runtime_upgrade() -> Weight {
        let current_timestamp = Timestamp::<T>::get();
        let old_slot = CurrentSlot::<T>::get();
        let new_slot = current_timestamp / T::SlotDuration::get();

        if old_slot != new_slot {
            CurrentSlot::<T>::put(new_slot);
            T::DbWeight::get().reads_writes(2, 1)
        } else {
            T::DbWeight::get().reads(2)
        }
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC016"),
        "SEC016 should allow idempotent writes guarded by old/new value reconciliation"
    );
}

#[test]
fn sec016_reports_mixed_reconciliation_and_unguarded_writes() {
    let code = r#"
impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_runtime_upgrade() -> Weight {
        let expected = Self::calculate_total();
        let current = Total::<T>::get();

        if current != expected {
            Total::<T>::put(expected);
        }

        LegacyFlag::<T>::put(true);
        T::DbWeight::get().reads_writes(2, 2)
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC016"),
        "SEC016 should still report migrations with additional unguarded writes"
    );
}

#[test]
fn sec016_allows_unchecked_migration_impls_wrapped_by_versioned_migration() {
    let code = r#"
pub type MigrateToV2<T> = frame_support::migrations::VersionedMigration<
    1,
    2,
    UncheckedMigrateToV2<T>,
    Pallet<T>,
    T::DbWeight,
>;

pub struct UncheckedMigrateToV2<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV2<T> {
    fn on_runtime_upgrade() -> Weight {
        OldValues::<T>::put(1);
        T::DbWeight::get().writes(1)
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/migration.rs", code);
    assert!(
        !has_rule(&diags, "SEC016"),
        "SEC016 should not require a second StorageVersion check inside UncheckedOnRuntimeUpgrade impls"
    );
}

#[test]
fn sec016_skips_frame_support_migration_helpers() {
    let bad = include_str!("fixtures/bad_sec016.rs");
    let diags = check_fixture("substrate/frame/support/src/migrations.rs", bad);
    assert!(
        !has_rule(&diags, "SEC016"),
        "SEC016 should skip generic FRAME support migration helper implementations"
    );
}

#[test]
fn sec016_still_checks_real_pallet_migration_files() {
    let bad = include_str!("fixtures/bad_sec016.rs");
    let diags = check_fixture("substrate/frame/assets/src/migration.rs", bad);
    assert!(
        has_rule(&diags, "SEC016"),
        "SEC016 should still report unguarded migrations in real pallet migration files"
    );
}

// ==========================================================================
// SEC017: Vec<T> in pallet events
// ==========================================================================
#[test]
fn sec017_detects_vec_in_events() {
    let bad = include_str!("fixtures/bad_sec017.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEC017"),
        "SEC017 should fire on Vec<T> in #[pallet::event]"
    );
}

#[test]
fn sec017_allows_bounded_event_payloads() {
    let good = include_str!("fixtures/good_sec017.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEC017"),
        "SEC017 should NOT fire on bounded event payloads"
    );
}

#[test]
fn sec017_allows_vec_event_payloads_from_bounded_vec_inputs() {
    let code = r#"
#[pallet::event]
pub enum Event<T: Config> {
    NewFeedData { sender: T::AccountId, values: Vec<(T::OracleKey, T::OracleValue)> },
}

#[pallet::call]
impl<T: Config> Pallet<T> {
    pub fn feed_values(
        origin: OriginFor<T>,
        values: BoundedVec<(T::OracleKey, T::OracleValue), T::MaxFeedValues>,
    ) -> DispatchResult {
        let who = ensure_signed(origin)?;
        Self::do_feed_values(who, values.into());
        Ok(())
    }
}

impl<T: Config> Pallet<T> {
    fn do_feed_values(who: T::AccountId, values: Vec<(T::OracleKey, T::OracleValue)>) {
        Self::deposit_event(Event::NewFeedData { sender: who, values });
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC017"),
        "SEC017 should allow Vec event fields sourced from BoundedVec extrinsic inputs"
    );
}

#[test]
fn sec017_reports_vec_event_payloads_from_unbounded_inputs() {
    let code = r#"
#[pallet::event]
pub enum Event<T: Config> {
    NewFeedData { sender: T::AccountId, values: Vec<(T::OracleKey, T::OracleValue)> },
}

#[pallet::call]
impl<T: Config> Pallet<T> {
    pub fn feed_values(
        origin: OriginFor<T>,
        values: Vec<(T::OracleKey, T::OracleValue)>,
    ) -> DispatchResult {
        let who = ensure_signed(origin)?;
        Self::deposit_event(Event::NewFeedData { sender: who, values });
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC017"),
        "SEC017 should still report Vec event fields sourced from unbounded inputs"
    );
}

// ==========================================================================
// SEC018: Missing weight term for unbounded input length
// ==========================================================================
#[test]
fn sec018_detects_contract_call_data_missing_from_weight() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(6)]
    #[pallet::weight(T::WeightInfo::call().saturating_add(*gas_limit))]
    pub fn call(
        origin: OriginFor<T>,
        gas_limit: Weight,
        data: Vec<u8>,
    ) -> DispatchResultWithPostInfo {
        let _ = ensure_signed(origin)?;
        let _ = data;
        Ok(().into())
    }
}
"#;
    let diags = check_fixture("pallets/contracts/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC018"),
        "SEC018 should fire when a Vec input length is absent from the weight expression"
    );
}

#[test]
fn sec018_detects_multisig_signatories_missing_from_weight() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight({
        let dispatch_info = call.get_dispatch_info();
        (
            T::WeightInfo::as_multi_threshold_1(call.using_encoded(|c| c.len() as u32))
                .saturating_add(dispatch_info.call_weight),
            dispatch_info.class,
        )
    })]
    pub fn as_multi_threshold_1(
        origin: OriginFor<T>,
        other_signatories: Vec<T::AccountId>,
        call: Box<<T as Config>::RuntimeCall>,
    ) -> DispatchResultWithPostInfo {
        let _ = ensure_signed(origin)?;
        let _ = other_signatories;
        let _ = call;
        Ok(().into())
    }
}
"#;
    let diags = check_fixture("pallets/multisig/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC018"),
        "SEC018 should fire when one unbounded Vec parameter is missing even if another input is length-accounted"
    );
}

#[test]
fn sec018_detects_privileged_rules_hash_missing_from_weight() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(8)]
    #[pallet::weight(T::WeightInfo::found_society())]
    pub fn found_society(
        origin: OriginFor<T>,
        founder: AccountIdLookupOf<T>,
        rules: Vec<u8>,
    ) -> DispatchResult {
        T::FounderSetOrigin::ensure_origin(origin)?;
        Rules::<T>::put(T::Hashing::hash(&rules));
        Self::deposit_event(Event::<T>::Founded { founder });
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/society/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEC018"),
        "SEC018 should still check privileged calls because pre-dispatch weight must cover input decoding"
    );
}

#[test]
fn sec018_allows_vec_len_in_weight() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::submit(data.len() as u32))]
    pub fn submit(origin: OriginFor<T>, data: Vec<u8>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        let _ = data;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not fire when the Vec length is passed into the weight expression"
    );
}

#[test]
fn sec018_skips_deprecated_dispatchables() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::old_call())]
    #[deprecated(note = "use call instead")]
    pub fn old_call(origin: OriginFor<T>, data: Vec<u8>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        let _ = data;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report deprecated compatibility dispatchables"
    );
}

#[test]
fn sec018_skips_max_weight_dispatchables() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(Weight::MAX)]
    pub fn eth_transact(origin: OriginFor<T>, payload: Vec<u8>) -> DispatchResult {
        let _ = origin;
        let _ = payload;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report calls already assigned Weight::MAX"
    );
}

#[test]
fn sec018_skips_dispatchables_that_consume_max_block_post_dispatch() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight((T::SystemWeightInfo::set_code(), DispatchClass::Operational))]
    pub fn set_code(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
        ensure_root(origin)?;
        T::OnSetCode::set_code(code)?;
        Ok(Some(T::BlockWeights::get().max_block).into())
    }
}
"#;
    let diags = check_fixture("pallets/system/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report calls that intentionally consume max-block post-dispatch weight"
    );
}

#[test]
fn sec018_skips_intentionally_ignored_vec_params() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::heartbeat())]
    pub fn heartbeat(origin: OriginFor<T>, _remark: Vec<u8>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report intentionally ignored Vec parameters"
    );
}

#[test]
fn sec018_skips_vec_params_with_explicit_body_length_bound() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::add_memo())]
    pub fn add_memo(origin: OriginFor<T>, memo: Vec<u8>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        ensure!(memo.len() <= T::MaxMemoLength::get() as usize, Error::<T>::TooLarge);
        Memo::<T>::set(memo);
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report Vec inputs with explicit accepted-path length bounds"
    );
}

#[test]
fn sec018_skips_vec_params_converted_to_bounded_types() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::set_items())]
    pub fn set_items(origin: OriginFor<T>, items: Vec<T::AccountId>) -> DispatchResult {
        T::AdminOrigin::ensure_origin(origin)?;
        let bounded = BoundedVec::<_, T::MaxItems>::try_from(items)
            .map_err(|_| Error::<T>::TooManyItems)?;
        Items::<T>::set(bounded);
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report Vec inputs converted into bounded types"
    );
}

#[test]
fn sec018_skips_vec_params_passed_to_bound_helpers() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::set_groups())]
    pub fn set_groups(origin: OriginFor<T>, groups: Vec<GroupOf<T>>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        let groups = Self::bound_groups(&who, groups)?;
        Groups::<T>::set(&who, groups);
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report Vec inputs passed through bound_* helpers"
    );
}

#[test]
fn sec018_skips_signature_params_verified_by_fixed_size_helpers() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::create_account())]
    pub fn create_account(
        origin: OriginFor<T>,
        who: T::AccountId,
        signature: Vec<u8>,
    ) -> DispatchResult {
        T::ValidityOrigin::ensure_origin(origin)?;
        Self::verify_signature(&who, &signature)?;
        Signatures::<T>::insert(&who, signature);
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/purchase/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report signature Vec inputs passed through fixed-size verification helpers"
    );
}

#[test]
fn sec018_skips_vec_params_compared_to_static_statement_text() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::attest())]
    pub fn attest(origin: OriginFor<T>, statement: Vec<u8>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        if let Some(s) = Signing::<T>::get(&who) {
            ensure!(s.to_text() == &statement[..], Error::<T>::InvalidStatement);
        }
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/claims/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report statement inputs compared to static StatementKind text"
    );
}

#[test]
fn sec018_skips_vec_params_decoded_into_concrete_key_types() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::set_keys())]
    pub fn set_keys(origin: OriginFor<T>, keys: Vec<u8>) -> DispatchResult {
        let stash = ensure_signed(origin)?;
        let session_keys = T::RelayChainSessionKeys::decode(&mut &keys[..])
            .map_err(|_| Error::<T>::InvalidKeys)?;
        T::SessionInterface::set_keys(&stash, session_keys)?;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/session/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report key bytes decoded into concrete session key types"
    );
}

#[test]
fn sec018_skips_opaque_keys_ownership_proofs() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::set_keys())]
    pub fn set_keys(origin: OriginFor<T>, keys: T::Keys, proof: Vec<u8>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        ensure!(
            who.using_encoded(|who| keys.ownership_proof_is_valid(who, &proof)),
            Error::<T>::InvalidProof,
        );
        Self::do_set_keys(&who, keys)?;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/session/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should not report OpaqueKeys proof tuples validated via ownership_proof_is_valid"
    );
}

#[test]
fn sec018_skips_test_utils_runtime_code() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(Weight::zero())]
    pub fn include_data(origin: OriginFor<T>, _data: Vec<u8>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;
    let diags = check_fixture("substrate/test-utils/runtime/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEC018"),
        "SEC018 should skip test utility runtimes"
    );
}

// ==========================================================================
// Config severity override
// ==========================================================================
#[test]
fn config_severity_override_works() {
    let mut config = polkadot_linter::config::Config::default();
    config
        .rules
        .severity
        .insert("SEM003".to_string(), "error".to_string());

    let bad = include_str!("fixtures/bad_sem003.rs");
    let diags = check_fixture_with_config("src/lib.rs", bad, &config);
    let sem003 = diags.iter().find(|d| d.rule_id == "SEM003");
    assert!(sem003.is_some(), "SEM003 should fire");
    assert_eq!(
        sem003.unwrap().severity,
        polkadot_linter::diagnostics::Severity::Error,
        "Severity should be overridden to Error"
    );
}

#[test]
fn config_rule_disable_works() {
    let mut config = polkadot_linter::config::Config::default();
    config.rules.enabled.insert("SEM003".to_string(), false);

    let bad = include_str!("fixtures/bad_sem003.rs");
    let diags = check_fixture_with_config("src/lib.rs", bad, &config);
    assert!(!has_rule(&diags, "SEM003"), "SEM003 should be disabled");
}

#[test]
fn engine_uses_cargo_test_targets_for_custom_test_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::write(
        root.join("Cargo.toml"),
        r#"
[package]
name = "custom-test-target"
version = "0.1.0"
edition = "2021"

[[test]]
name = "driver"
path = "support/test_driver.rs"
"#,
    )
    .unwrap();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("support")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn lib_helper() {}\n").unwrap();
    std::fs::write(
        root.join("support/test_driver.rs"),
        r#"
pub fn helper() {
    debug_assert!(true);
}
"#,
    )
    .unwrap();

    let config = polkadot_linter::config::Config::default();
    let mut engine = polkadot_linter::engine::LintEngine::new(config);
    engine.filter_rules(&["SEC002".to_string()]);

    let diags = engine.scan(root);
    assert!(
        !has_rule(&diags, "SEC002"),
        "SEC002 should skip files declared as cargo test targets even when their path does not match test heuristics"
    );
}

#[test]
fn engine_uses_cargo_bench_targets_for_custom_bench_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::write(
        root.join("Cargo.toml"),
        r#"
[package]
name = "custom-bench-target"
version = "0.1.0"
edition = "2021"

[[bench]]
name = "driver"
path = "support/bench_driver.rs"
"#,
    )
    .unwrap();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("support")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn lib_helper() {}\n").unwrap();
    std::fs::write(
        root.join("support/bench_driver.rs"),
        r#"
pub fn helper() {
    debug_assert!(true);
}
"#,
    )
    .unwrap();

    let config = polkadot_linter::config::Config::default();
    let mut engine = polkadot_linter::engine::LintEngine::new(config);
    engine.filter_rules(&["SEC002".to_string()]);

    let diags = engine.scan(root);
    assert!(
        !has_rule(&diags, "SEC002"),
        "SEC002 should skip files declared as cargo bench targets even when their path does not match benchmark heuristics"
    );
}
