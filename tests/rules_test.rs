use std::path::PathBuf;

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
        is_test_file: content.contains("#[test]")
            || content.contains("#[cfg(test)]")
            || filename.contains("test"),
        is_benchmark_file: filename.contains("benchmarking"),
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
        is_test_file: content.contains("#[test]")
            || content.contains("#[cfg(test)]")
            || filename.contains("test"),
        is_benchmark_file: filename.contains("benchmarking"),
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
        is_test_file: content.contains("#[test]")
            || content.contains("#[cfg(test)]")
            || path_str.contains("test"),
        is_benchmark_file: path_str.contains("benchmarking"),
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
fn sec007_skips_test_files() {
    let bad = include_str!("fixtures/bad_sec007.rs");
    let diags = check_fixture("pallets/foo/src/tests.rs", bad);
    assert!(!has_rule(&diags, "SEC007"), "SEC007 should skip test files");
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
