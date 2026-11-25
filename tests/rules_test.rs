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

fn test_rules(
    config: &polkadot_linter::config::Config,
) -> Vec<Box<dyn polkadot_linter::rules::LintRule>> {
    let mut rules = polkadot_linter::rules::all_rules(config);
    rules.retain(|rule| config.rule_enabled(rule.id()));
    rules
}

/// Helper: create a FileContext and run a specific rule against fixture content.
/// The bundled defaults plus the default-off rules, so every rule is tested.
fn all_rules_config() -> polkadot_linter::config::Config {
    polkadot_linter::config::Config::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config/all-rules.toml"),
    )
    .expect("config/all-rules.toml is valid")
}

fn check_fixture(filename: &str, content: &str) -> Vec<polkadot_linter::diagnostics::Diagnostic> {
    let config = all_rules_config();
    let path = PathBuf::from(filename);
    let rel_path = PathBuf::from(filename);
    let is_rust = filename.ends_with(".rs");
    let is_text = !is_rust;

    let ctx = polkadot_linter::engine::FileContext {
        classification_path: polkadot_linter::engine::classification_path(&path, &rel_path),
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

    let rules = test_rules(&config);
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
        classification_path: polkadot_linter::engine::classification_path(&path, &rel_path),
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

    let rules = test_rules(config);
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
    let config = all_rules_config();
    let path_str = path.to_string_lossy().to_string();
    let is_rust = path_str.ends_with(".rs");
    let ctx = polkadot_linter::engine::FileContext {
        classification_path: polkadot_linter::engine::classification_path(&path, &path),
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

    let rules = test_rules(&config);
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

#[test]
fn syntax_engine_does_not_register_compiler_backed_rules() {
    // Every rule enabled, so a default-off rule registered by mistake is still caught.
    let config = all_rules_config();
    let registered = polkadot_linter::rules::all_rules(&config)
        .into_iter()
        .map(|rule| rule.id().to_string())
        .collect::<Vec<_>>();

    for rule_id in polkadot_linter::rules::COMPILER_BACKED_RULE_IDS {
        assert!(
            !registered
                .iter()
                .any(|registered_id| registered_id == rule_id),
            "{rule_id} must have exactly one public implementation authority"
        );
    }
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

#[test]
fn tst002_detects_apply_extrinsic_paths_and_methods() {
    let bad = r#"
#[test]
fn apply_extrinsic_helpers() {
    new_test_ext().execute_with(|| {
        assert_ok!(Executive::apply_extrinsic(xt.clone()));
        assert_ok!(apply_extrinsic(xt.clone()));
        assert_ok!(block_builder.apply_extrinsic(xt));
    });
}
"#;
    let diags = check_fixture("tests/test.rs", bad);
    let count = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "TST002")
        .count();
    assert_eq!(
        count, 3,
        "TST002 must flag apply_extrinsic called as a path or a method: {diags:?}"
    );
}

#[test]
fn tst002_ignores_identifiers_that_only_contain_apply_extrinsic() {
    let good = r#"
#[test]
fn apply_extrinsics_returns_single_result() {
    new_test_ext().execute_with(|| {
        assert_ok!(Executive::apply_extrinsics(Block::default().extrinsics(), 1, ()), ());
        assert_ok!(apply_extrinsic_with_weight(xt), ());
        assert_ok!(Pallet::submit(origin), apply_extrinsic);
        assert_ok!(Executive::apply_extrinsic(xt.clone()).unwrap());
        assert_ok!(Executive::apply_extrinsic(xt).expect("valid extrinsic"));
        assert_ok!(Executive::apply_extrinsics(mode, xts, |xt, _| Executive::apply_extrinsic(xt)), ());
        // Single-argument forms, so the exact-name match is tested on its own.
        assert_ok!(Executive::apply_extrinsics(Block::default().extrinsics(), 1));
        assert_ok!(apply_extrinsic_with_weight(xt));
        assert_ok!(Executive::apply_extrinsics(mode, xts, |xt, _| Executive::apply_extrinsic(xt)));
        assert_ok!(Executive::apply_extrinsic(xt)?);
        // `assert_ok!(x, y)` is `assert_eq!(x, Ok(y))`, which checks both layers.
        assert_ok!(Executive::apply_extrinsic(xt), Ok(()));
    });
}
"#;
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST002"),
        "TST002 must match the apply_extrinsic identifier exactly, and accept results whose outer layer is unwrapped: {diags:?}"
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

#[test]
fn trm001_handles_non_ascii_before_inline_comments() {
    let mut config = polkadot_linter::config::Config::default();
    config
        .terminology
        .british_english
        .insert("optimisation".to_string(), "optimization".to_string());

    let diags = check_fixture_with_config(
        "src/lib.rs",
        "let label = \"éééé\"; // optimisation\n",
        &config,
    );
    assert!(
        has_rule(&diags, "TRM001"),
        "TRM001 must not panic or lose inline comments after non-ASCII source text"
    );
}

#[test]
fn trm001_checks_identifiers_on_lines_with_strings_and_comments() {
    let mut config = polkadot_linter::config::Config::default();
    config.terminology.check_identifiers = true;
    config
        .terminology
        .british_english
        .insert("optimisation".to_string(), "optimization".to_string());

    let diags = check_fixture_with_config(
        "src/lib.rs",
        "let optimisation = \"already optimized\"; // keep this value\n",
        &config,
    );
    assert!(
        has_rule(&diags, "TRM001"),
        "TRM001 must inspect identifiers even when the line contains strings or comments: {diags:?}"
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
fn sem011_detects_literal_zero_weights() {
    let diags = check_fixture(
        "pallets/foo/src/lib.rs",
        include_str!("fixtures/bad_sem011.rs"),
    );
    let lines: Vec<_> = diags
        .iter()
        .filter(|d| d.rule_id == "SEM011")
        .map(|d| d.line)
        .collect();
    assert_eq!(lines, vec![6, 20, 27], "{diags:?}");
    let good = check_fixture(
        "pallets/foo/src/lib.rs",
        include_str!("fixtures/good_sem011.rs"),
    );
    assert!(!has_rule(&good, "SEM011"), "{good:?}");
}

#[test]
fn sem011_skips_root_only_testnet_calls() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::weight(Weight::zero())]
    #[cfg(feature = "testnet")]
    pub fn testnet_force(origin: OriginFor<T>) -> DispatchResult {
        if !T::TESTNET {
            return Err(Error::<T>::InvalidState.into());
        }
        ensure_root(origin)?;
        Ok(())
    }

    #[pallet::weight(Weight::zero())]
    #[cfg(all(feature = "paseo-testnet", not(feature = "std")))]
    pub fn gated_all(origin: OriginFor<T>) -> DispatchResult {
        ensure_root(origin)?;
        Ok(())
    }

    #[pallet::weight(Weight::zero())]
    pub fn root_not_gated(origin: OriginFor<T>) -> DispatchResult {
        ensure_root(origin)?;
        Ok(())
    }

    #[pallet::weight(Weight::zero())]
    #[cfg(feature = "testnet")]
    pub fn signed_testnet(origin: OriginFor<T>) -> DispatchResult {
        let _who = ensure_signed(origin)?;
        Ok(())
    }

    #[pallet::weight(Weight::zero())]
    #[cfg(not(feature = "testnet"))]
    pub fn not_testnet(origin: OriginFor<T>) -> DispatchResult {
        ensure_root(origin)?;
        Ok(())
    }

    #[pallet::weight(Weight::zero())]
    #[cfg(any(feature = "testnet", feature = "std"))]
    pub fn any_testnet(origin: OriginFor<T>) -> DispatchResult {
        ensure_root(origin)?;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let lines: Vec<_> = diags
        .iter()
        .filter(|d| d.rule_id == "SEM011")
        .map(|d| d.line)
        .collect();
    // Recall guard: ungated, signed-origin, `not(..)` and `any(..)` zero weights still fire.
    assert_eq!(lines, vec![21, 27, 34, 41], "{diags:?}");
}

#[test]
fn sem011_detects_weight_zero_in_qualified_frame_attribute() {
    let code = r#"
#[frame_support::pallet::weight(Weight::zero())]
pub fn submit(origin: OriginFor<T>) -> DispatchResult {
    let _ = origin;
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEM011"),
        "SEM011 must inspect qualified FRAME weight attributes: {diags:?}"
    );
}

#[test]
fn sem011_respects_provable_cfg_attribute_truth() {
    let active = r#"
#[cfg_attr(not(any()), pallet::weight(Weight::zero()))]
pub fn active_weight() {}
"#;
    let inactive = r#"
#[cfg_attr(any(), pallet::weight(Weight::zero()))]
pub fn inactive_weight() {}
"#;
    let feature_dependent = r#"
#[cfg_attr(feature = "runtime-benchmarks", pallet::weight(Weight::zero()))]
pub fn feature_weight() {}
"#;

    assert!(
        has_rule(&check_fixture("pallets/foo/src/lib.rs", active), "SEM011"),
        "SEM011 must inspect a provably active cfg_attr weight"
    );
    assert!(
        !has_rule(&check_fixture("pallets/foo/src/lib.rs", inactive), "SEM011"),
        "SEM011 must not inspect a provably inactive cfg_attr weight"
    );
    assert!(
        !has_rule(
            &check_fixture("pallets/foo/src/lib.rs", feature_dependent),
            "SEM011"
        ),
        "SEM011 must not guess whether a Cargo feature enables cfg_attr"
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

/// Production-only rules share one non-production masking layer. Each case
/// hides one copy of the finding and leaves a production copy that must still
/// be reported, so a mask that swallows everything also fails.
#[test]
fn production_rules_skip_non_production_cfg_but_keep_production_code() {
    let enum_def = "pub trait Verifier { fn verify_signature(_payload: &[u8]) -> bool { true } }";
    let cases = [
        (
            "crate-level cfg",
            format!("#![cfg(any(feature = \"runtime-benchmarks\", test))]\n{enum_def}"),
            0,
        ),
        (
            "test-helpers crate cfg",
            format!("#![cfg(any(feature = \"test-helpers\", test))]\n{enum_def}"),
            0,
        ),
        (
            "cfg(test) module",
            format!("#[cfg(test)]\nmod tests {{ {enum_def} }}\nmod prod {{ {enum_def} }}"),
            1,
        ),
        (
            "remote-test module",
            format!(
                "#[cfg(feature = \"remote-test\")]\nmod remote {{ {enum_def} }}\nmod prod {{ {enum_def} }}"
            ),
            1,
        ),
        (
            "integrity-test module",
            format!(
                "#[cfg(feature = \"integrity-test\")]\nmod checks {{ {enum_def} }}\nmod prod {{ {enum_def} }}"
            ),
            1,
        ),
        (
            "cfg(all(...)) module",
            format!(
                "#[cfg(all(feature = \"try-runtime\", feature = \"std\"))]\nmod checks {{ {enum_def} }}\nmod prod {{ {enum_def} }}"
            ),
            1,
        ),
        (
            "std-only module is production",
            format!("#[cfg(feature = \"std\")]\nmod native {{ {enum_def} }}"),
            1,
        ),
    ];
    for (case, code, expected) in cases {
        let diags = check_fixture("substrate/frame/example/src/lib.rs", &code);
        let count = diags.iter().filter(|d| d.rule_id == "SEM019").count();
        assert_eq!(count, expected, "{case}");
    }
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
// Config severity override
// ==========================================================================
#[test]
fn config_severity_override_works() {
    let mut config = all_rules_config();
    config
        .rules
        .severity
        .insert("SEM011".to_string(), "advisory".to_string());

    let bad = include_str!("fixtures/bad_sem011.rs");
    let diags = check_fixture_with_config("src/lib.rs", bad, &config);
    let sem011 = diags.iter().find(|d| d.rule_id == "SEM011");
    assert!(sem011.is_some(), "SEM011 should fire");
    assert_eq!(
        sem011.unwrap().severity,
        polkadot_linter::diagnostics::Severity::Advisory,
        "Severity should be overridden to Advisory"
    );
}

#[test]
fn config_rule_disable_works() {
    let mut config = polkadot_linter::config::Config::default();
    config.rules.enabled.insert("SEM011".to_string(), false);

    let bad = include_str!("fixtures/bad_sem011.rs");
    let diags = check_fixture_with_config("src/lib.rs", bad, &config);
    assert!(!has_rule(&diags, "SEM011"), "SEM011 should be disabled");
}

// ==========================================================================
// Runtime configuration rules (RTC001-RTC008)
// ==========================================================================

fn rtc_lines(diags: &[polkadot_linter::diagnostics::Diagnostic], rule_id: &str) -> Vec<usize> {
    let mut lines = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == rule_id)
        .map(|diagnostic| diagnostic.line)
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines
}

fn rtc_findings(diags: &[polkadot_linter::diagnostics::Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id.starts_with("RTC"))
        .map(|diagnostic| format!("{}:{}", diagnostic.rule_id, diagnostic.line))
        .collect()
}

/// Write a minimal runtime crate (manifest + lib.rs + extra files) to a temp dir.
fn rtc_runtime_crate(package: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temporary runtime crate should be created");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .expect("manifest should be written");
    for (path, content) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).expect("source dir should be created");
        std::fs::write(path, content).expect("source should be written");
    }
    dir
}

fn rtc_scan(
    root: &std::path::Path,
    config: polkadot_linter::config::Config,
) -> Vec<polkadot_linter::diagnostics::Diagnostic> {
    let mut engine = polkadot_linter::engine::LintEngine::new(config);
    engine.filter_rules(&["runtime-config".to_string()]);
    engine.scan(root)
}

const RTC_RUNTIME_LIB: &str = r#"
impl pallet_sudo::Config for Runtime {
    type WeightInfo = TestWeightInfo;
}

construct_runtime!(
    pub enum Runtime {
        Sudo: pallet_sudo = 15,
    }
);
"#;

#[test]
fn rtc001_detects_a_runtime_built_with_a_forked_construct_runtime() {
    // InvArch declares its runtimes with `construct_runtime_modified!`.
    let fixture = include_str!("fixtures/bad_rtc001.rs");
    let forked = fixture.replace("construct_runtime!", "construct_runtime_modified!");
    let diags = check_fixture("runtime/src/lib.rs", &forked);
    assert_eq!(
        rtc_lines(&diags, "RTC001"),
        vec![7, 12, 16, 20],
        "{diags:?}"
    );
    // Recall guard: the stock macro is unchanged, an unrelated name is not a runtime.
    let diags = check_fixture("runtime/src/lib.rs", fixture);
    assert_eq!(
        rtc_lines(&diags, "RTC001"),
        vec![7, 12, 16, 20],
        "{diags:?}"
    );
    let other = fixture.replace("construct_runtime!", "not_a_runtime!");
    let diags = check_fixture("runtime/src/lib.rs", &other);
    assert!(rtc_lines(&diags, "RTC001").is_empty(), "{diags:?}");
}

#[test]
fn rtc001_detects_placeholder_weight_info() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc001.rs"));
    assert_eq!(
        rtc_lines(&diags, "RTC001"),
        vec![7, 12, 16, 20],
        "{diags:?}"
    );
    assert!(diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "RTC001")
        .all(|diagnostic| diagnostic.severity == polkadot_linter::diagnostics::Severity::Warning));
}

#[test]
fn rtc001_allows_benchmarked_allowlisted_and_test_weights() {
    let diags = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc001.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc_rules_skip_mock_and_test_paths() {
    let bad = include_str!("fixtures/bad_rtc001.rs");
    for path in [
        "runtime/src/mock.rs",
        "runtime/tests/runtime.rs",
        "runtime/src/mock/mod.rs",
        "runtime/benches/weights.rs",
        "my-test-runtime/src/lib.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(rtc_findings(&diags).is_empty(), "{path}: {diags:?}");
    }
}

#[test]
fn rtc_rules_skip_files_outside_runtime_crates() {
    // A pallet crate's mock-like Config impl without a runtime declaration.
    let content = r#"
impl pallet_xcm::Config for Runtime {
    type WeightInfo = pallet_xcm::TestWeightInfo;
    type PriceForSiblingDelivery = ();
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", content);
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc003_detects_waived_xcm_fees() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc003.rs"),
    );
    assert_eq!(
        rtc_lines(&diags, "RTC003"),
        vec![6, 13, 17, 20, 28, 39],
        "{diags:?}"
    );
    assert!(rtc_lines(&diags, "RTC006").is_empty(), "{diags:?}");
    assert!(diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "RTC003")
        .all(|diagnostic| diagnostic.severity == polkadot_linter::diagnostics::Severity::Warning));
}

#[test]
fn rtc003_allows_fee_managers_exponential_prices_and_senderless_executors() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/good_rtc003.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc004_detects_zero_deposits() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc004.rs"));
    assert_eq!(
        rtc_lines(&diags, "RTC004"),
        vec![4, 7, 8, 9, 15, 16, 20, 24, 28, 29, 32, 38, 50],
        "{diags:?}"
    );
}

#[test]
fn rtc004_allows_non_zero_gated_and_benchmark_only_deposits() {
    let diags = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc004.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc005_detects_collective_flip_randomness() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc005.rs"));
    assert_eq!(rtc_lines(&diags, "RTC005"), vec![10], "{diags:?}");
}

#[test]
fn rtc005_allows_vrf_randomness_and_storage_prefix_bytes() {
    let diags = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc005.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc_rules_skip_dev_runtimes_unless_opted_in() {
    let krate = rtc_runtime_crate("kitchensink-runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");

    let mut config = polkadot_linter::config::Config::default();
    config.runtime_config.include_dev_runtimes = true;
    let diags = rtc_scan(krate.path(), config);
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc_rules_skip_dev_runtimes_by_whole_name_token() {
    for package in ["peaq-dev", "development-runtime"] {
        let krate = rtc_runtime_crate(package, &[("src/lib.rs", RTC_RUNTIME_LIB)]);
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert!(rtc_findings(&diags).is_empty(), "{package}: {diags:?}");
        let mut config = polkadot_linter::config::Config::default();
        config.runtime_config.include_dev_runtimes = true;
        let diags = rtc_scan(krate.path(), config);
        assert_eq!(
            rtc_findings(&diags),
            vec!["RTC001:3".to_string()],
            "{package}"
        );
    }
    for package in ["devops-x-runtime", "devnet-runtime"] {
        let krate = rtc_runtime_crate(package, &[("src/lib.rs", RTC_RUNTIME_LIB)]);
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert_eq!(
            rtc_findings(&diags),
            vec!["RTC001:3".to_string()],
            "{package}"
        );
    }
}

#[test]
fn rtc_checks_unsaved_file_against_its_crate() {
    // An editor buffer not yet on disk still belongs to the enclosing crate.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"review-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "construct_runtime!(pub enum Runtime { Sudo: pallet_sudo = 15 });",
    )
    .unwrap();
    let path = root.join("src/new_config.rs");
    assert!(!path.exists());
    let diags = check_fixture_path(
        path,
        "impl pallet_sudo::Config for Runtime { type WeightInfo = TestWeightInfo; }",
    );
    assert_eq!(rtc_lines(&diags, "RTC001"), vec![1], "{diags:?}");
}

#[test]
fn findings_ignore_test_named_ancestors_above_scan_root() {
    let sem = include_str!("fixtures/bad_sem019.rs");
    let dir = tempfile::tempdir().expect("temporary parent should be created");
    let scan = |ancestor: &str| {
        let root = dir.path().join(ancestor).join("westend-runtime");
        std::fs::create_dir_all(root.join("src")).expect("source dir should be created");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"westend-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("manifest should be written");
        std::fs::write(root.join("src/lib.rs"), RTC_RUNTIME_LIB).expect("lib should be written");
        std::fs::write(root.join("src/sem.rs"), sem).expect("source should be written");
        let engine = polkadot_linter::engine::LintEngine::new(all_rules_config());
        let mut findings = engine
            .scan(&root)
            .iter()
            .map(|d| {
                format!(
                    "{}:{}:{}",
                    d.rule_id,
                    d.file.strip_prefix(&root).unwrap().display(),
                    d.line
                )
            })
            .collect::<Vec<_>>();
        findings.sort();
        findings
    };
    let baseline = scan("ok");
    for rule in ["RTC001", "SEM019"] {
        assert!(
            baseline.iter().any(|f| f.starts_with(rule)),
            "{rule}: {baseline:?}"
        );
    }
    for ancestor in ["test", "tests"] {
        assert_eq!(scan(ancestor), baseline, "scan root under {ancestor}/");
    }
}

#[test]
fn relative_and_absolute_scan_paths_give_the_same_rtc_findings() {
    // The runtime crate canonicalises its manifest directory. A relative scan
    // path under `tests/` must still strip to a crate-relative path, or
    // `is_test_path` sees the `tests` ancestor and drops every finding.
    let dir = tempfile::tempdir().expect("temporary parent should be created");
    let krate = dir.path().join("tests/my-runtime");
    std::fs::create_dir_all(krate.join("src")).expect("source dir should be created");
    std::fs::write(
        krate.join("Cargo.toml"),
        "[package]\nname = \"my-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest should be written");
    std::fs::write(krate.join("src/lib.rs"), RTC_RUNTIME_LIB).expect("lib should be written");
    let scan = |path: &std::path::Path| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_polkadot-linter"))
            .current_dir(dir.path())
            .args([
                "--syntax-only",
                "--format",
                "json",
                "--rules",
                "runtime-config",
            ])
            .arg(path)
            .output()
            .expect("linter should run");
        let diags =
            serde_json::from_slice::<Vec<serde_json::Value>>(&output.stdout).expect("JSON output");
        diags
            .iter()
            .map(|d| format!("{}:{}", d["rule_id"].as_str().unwrap(), d["line"]))
            .collect::<Vec<_>>()
    };
    let absolute = scan(&krate);
    assert_eq!(absolute, vec!["RTC001:3".to_string()]);
    assert_eq!(scan(std::path::Path::new("tests/my-runtime")), absolute);
}

#[test]
fn narrow_scans_classify_test_and_bench_files_from_the_crate_root() {
    let dir = tempfile::tempdir().expect("temporary crate should be created");
    let krate = dir.path().join("c");
    let verifier = "pub trait Verifier {\n    fn verify_signature(_payload: &[u8]) -> bool {\n        true\n    }\n}\n";
    let helper = format!(
        "{verifier}pub fn check(xt: u32) {{\n    assert_ok!(Executive::apply_extrinsic(xt));\n}}\n"
    );
    for (file, content) in [
        (
            "Cargo.toml",
            "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/lib.rs", verifier),
        (
            "tests/main.rs",
            "mod support;\n#[test]\nfn t() { support::check(0); }\n",
        ),
        ("tests/support/mod.rs", &helper),
        ("benches/benchmarks/helper.rs", &helper),
    ] {
        let path = krate.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).expect("dir should be created");
        std::fs::write(path, content).expect("file should be written");
    }
    let engine = polkadot_linter::engine::LintEngine::new(all_rules_config());
    let rules = |target: &str| {
        let mut ids = engine
            .scan(&krate.join(target))
            .into_iter()
            .map(|d| d.rule_id)
            .filter(|id| ["TST002", "SEM019"].contains(&id.as_str()))
            .collect::<Vec<_>>();
        ids.sort();
        ids
    };
    // Test helpers get test rules only; benchmark helpers get neither test
    // nor production rules; library code gets production rules.
    assert_eq!(rules("tests"), ["TST002"]);
    assert_eq!(rules("tests/support/mod.rs"), ["TST002"]);
    assert!(rules("benches/benchmarks").is_empty());
    assert_eq!(rules(""), ["SEM019", "TST002"]);
}

#[test]
fn rtc_rules_always_skip_test_runtime_packages() {
    let krate = rtc_runtime_crate("polkadot-test-runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let mut config = polkadot_linter::config::Config::default();
    config.runtime_config.include_dev_runtimes = true;
    let diags = rtc_scan(krate.path(), config);
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc_rules_lint_production_runtime_crates_only() {
    let krate = rtc_runtime_crate("westend-runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );

    // A source file under a virtual workspace manifest is not a runtime crate.
    let dir = tempfile::tempdir().expect("temporary workspace should be created");
    std::fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("manifest should be written");
    std::fs::create_dir_all(dir.path().join("orphan/src")).expect("orphan dir");
    std::fs::write(dir.path().join("orphan/src/lib.rs"), RTC_RUNTIME_LIB).expect("orphan source");
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc006_reports_burned_xcm_fees_as_warning() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc006.rs"),
    );
    assert_eq!(rtc_lines(&diags, "RTC006"), vec![8], "{diags:?}");
    // One free price among priced ones does not make delivery free.
    assert_eq!(rtc_lines(&diags, "RTC003"), vec![26], "{diags:?}");
    assert!(diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "RTC006")
        .all(|diagnostic| diagnostic.severity == polkadot_linter::diagnostics::Severity::Warning));
}

#[test]
fn rtc006_ignores_unit_fee_managers_when_every_delivery_price_is_free() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/good_rtc006.rs"),
    );
    assert!(rtc_lines(&diags, "RTC006").is_empty(), "{diags:?}");
    assert_eq!(rtc_lines(&diags, "RTC003"), vec![6, 18], "{diags:?}");
}

fn rtc009_lockfile(packages: &[(&str, &str, Option<&str>)]) -> String {
    let mut lock = "version = 4\n".to_string();
    for (name, version, source) in packages {
        lock += &format!("\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n");
        if let Some(source) = source {
            lock += &format!("source = \"{source}\"\n");
        }
    }
    lock
}

const CRATES_IO: Option<&str> = Some("registry+https://github.com/rust-lang/crates.io-index");

#[test]
fn sem019_reports_verifier_trait_defaults_that_accept_everything() {
    let diags = check_fixture(
        "substrate/primitives/runtime/src/traits/mod.rs",
        include_str!("fixtures/bad_sem019.rs"),
    );
    assert_eq!(rtc_lines(&diags, "SEM019"), vec![6, 10], "{diags:?}");
}

#[test]
fn rtc009_reports_vulnerable_xcm_executor_releases_in_cargo_lock() {
    // fellows runtimes before 2.5.1 (28.0.1) and before the Feb 2026 bump (23.0.0, 21.0.0).
    for (version, expected) in [
        ("28.0.1", "upgrade to 28.1.0 or later"),
        ("25.0.0", "upgrade to 25.2.0 or later"),
        ("23.0.0", "upgrade to 23.0.1 or later"),
        ("19.1.3", "upgrade to 19.1.4 or later"),
        (
            "21.0.0",
            "the 21.x line has no fixed release; the nearest fixed release is 22.0.1",
        ),
        (
            "26.0.0",
            "the 26.x line has no fixed release; the nearest fixed release is 27.1.0",
        ),
        (
            "18.0.1",
            "the 18.x line has no fixed release; the nearest fixed release is 19.1.4",
        ),
    ] {
        let lock = rtc009_lockfile(&[
            ("serde", "1.0.0", CRATES_IO),
            ("staging-xcm-executor", version, CRATES_IO),
        ]);
        let diags = check_fixture("Cargo.lock", &lock);
        assert_eq!(rtc_lines(&diags, "RTC009"), vec![9], "{version}: {diags:?}");
        let message = &diags
            .iter()
            .find(|d| d.rule_id == "RTC009")
            .unwrap()
            .message;
        assert!(
            message.contains(&format!("staging-xcm-executor {version} ")),
            "{message}"
        );
        assert!(message.contains(expected), "{message}");
        assert!(message.contains("https://"), "{message}");
    }
}

#[test]
fn rtc009_ignores_fixed_unaffected_and_local_xcm_executor_versions() {
    for (version, source) in [
        ("28.1.0", CRATES_IO), // first fixed (runtimes 2.5.1)
        ("24.0.1", CRATES_IO), // InitiateTransfer fix
        ("29.0.0", CRATES_IO), // released with the dotUSD fix
        ("18.0.0", CRATES_IO), // before the InitiateTransfer regression
        ("19.0.0", CRATES_IO), // yanked, without the regression
        ("25.0.0", None),      // workspace path package (polkadot-sdk itself)
        (
            "25.0.0",
            Some("git+https://github.com/paritytech/polkadot-sdk?branch=master#0"),
        ),
        ("28.0.1-rc1", CRATES_IO),
        // A private registry may serve a patched fork with the same version.
        ("28.0.1", Some("registry+https://crates.example.com/index")),
        ("28.0.1", Some("sparse+https://crates.example.com/index/")),
    ] {
        let lock = rtc009_lockfile(&[("staging-xcm-executor", version, source)]);
        let diags = check_fixture("Cargo.lock", &lock);
        assert!(
            rtc_lines(&diags, "RTC009").is_empty(),
            "{version}: {diags:?}"
        );
    }
    // Only `Cargo.lock` files are lockfiles.
    let lock = rtc009_lockfile(&[("staging-xcm-executor", "28.0.1", CRATES_IO)]);
    assert!(rtc_lines(&check_fixture("deps.toml", &lock), "RTC009").is_empty());
}

#[test]
fn rtc009_reports_the_name_line_of_the_matching_package_block() {
    let lock = "version = 4\n\n[[package]]\nname = \"staging-xcm-executor\"\nversion = \"29.0.0\"\n\
source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\n\
version = \"28.0.1\"\nsource = \"sparse+https://index.crates.io/\"\nname = \"staging-xcm-executor\"\n";
    let diags = check_fixture("Cargo.lock", lock);
    assert_eq!(rtc_lines(&diags, "RTC009"), vec![11], "{diags:?}");
}

#[test]
fn rtc009_runs_on_cargo_lock_in_a_directory_scan() {
    let dir = tempfile::tempdir().expect("temporary workspace should be created");
    let lock = rtc009_lockfile(&[("staging-xcm-executor", "28.0.1", CRATES_IO)]);
    std::fs::write(dir.path().join("Cargo.lock"), lock).expect("lockfile should be written");
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    assert_eq!(rtc_lines(&diags, "RTC009"), vec![4], "{diags:?}");
}

#[test]
fn rtc007_detects_non_recursive_instruction_denies() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc007.rs"),
    );
    assert_eq!(
        rtc_lines(&diags, "RTC007"),
        vec![7, 13, 13, 17],
        "{diags:?}"
    );
    let good = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/good_rtc007.rs"),
    );
    assert!(rtc_findings(&good).is_empty(), "{good:?}");
}

#[test]
fn rtc008_detects_fail_open_proxy_filters() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc008.rs"));
    assert_eq!(rtc_lines(&diags, "RTC008"), vec![20, 60], "{diags:?}");
    let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages[0].contains("PolkadotXcm (pallet_xcm), ForeignAssets (pallet_assets)"),
        "{messages:?}"
    );
    assert!(
        messages[1].contains(
            "`NonTransfer` can add `Collator` proxies, which admit `CollatorSelection` calls \
             `NonTransfer` does not"
        ),
        "{messages:?}"
    );
    let good = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc008.rs"),
    );
    assert!(rtc_findings(&good).is_empty(), "{good:?}");
}

#[test]
fn rtc010_reports_authorized_aliasers_on_the_unpaid_barrier() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc010.rs"),
    );
    assert_eq!(rtc_lines(&diags, "RTC010"), vec![22, 26, 27], "{diags:?}");
    // Outside a runtime crate the same barrier is not reported.
    let diags = check_fixture(
        "pallets/xcm-mock/src/lib.rs",
        &include_str!("fixtures/bad_rtc010.rs").replace("construct_runtime!", "not_a_runtime!"),
    );
    assert!(rtc_lines(&diags, "RTC010").is_empty(), "{diags:?}");
}

#[test]
fn rtc010_reports_an_ambiguous_alias_only_if_every_definition_reads_storage() {
    let xcm = |second: &str| {
        format!(
            "construct_runtime!(pub enum Runtime {{ PolkadotXcm: pallet_xcm = 31 }});\n\
mod kusama {{\n    pub type Aliasers = (AliasChildLocation, AuthorizedAliasers<Runtime>);\n}}\n\
mod polkadot {{\n    pub type Aliasers = {second};\n}}\n\
pub type Barrier = AllowExplicitUnpaidExecutionFrom<Equals<Parent>, Aliasers>;\n"
        )
    };
    let diags = check_fixture("runtime/src/xcm_config.rs", &xcm("(AliasChildLocation,)"));
    assert!(rtc_lines(&diags, "RTC010").is_empty(), "{diags:?}");
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        &xcm("AuthorizedAliasers<Runtime>"),
    );
    assert_eq!(rtc_lines(&diags, "RTC010"), vec![8], "{diags:?}");
}

#[test]
fn rtc003_keeps_warning_with_a_minimal_user_config() {
    let config: polkadot_linter::config::Config =
        toml::from_str("[rules]\n").expect("minimal user configuration must deserialize");
    let diags = check_fixture_with_config(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc003.rs"),
        &config,
    );
    let severities = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id.starts_with("RTC"))
        .map(|diagnostic| (diagnostic.rule_id.as_str(), diagnostic.severity))
        .collect::<Vec<_>>();
    assert!(!severities.is_empty(), "{diags:?}");
    assert!(
        severities
            .iter()
            .all(|(_, severity)| *severity == polkadot_linter::diagnostics::Severity::Warning),
        "{severities:?}"
    );
}

#[test]
fn rtc_rules_recognise_a_bare_runtime_attribute() {
    let lib = r#"
use frame_support::runtime;

impl pallet_sudo::Config for Runtime {
    type WeightInfo = TestWeightInfo;
}

#[runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(RuntimeCall, RuntimeEvent)]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type Sudo = pallet_sudo;
}
"#;
    let krate = rtc_runtime_crate("bare-attr-runtime", &[("src/lib.rs", lib)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:5".to_string()],
        "{diags:?}"
    );

    // A bare `#[runtime]` on a module without `#[runtime::*]` items is not a
    // runtime declaration.
    let lib = "#[runtime]\nmod runtime { pub struct Runtime; }\nimpl pallet_sudo::Config for Runtime {\n    type WeightInfo = TestWeightInfo;\n}\n";
    let krate = rtc_runtime_crate("not-a-runtime", &[("src/lib.rs", lib)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc_rules_follow_lib_path_and_reachable_modules() {
    // construct_runtime! lives in a module re-exported from lib.rs.
    let krate = rtc_runtime_crate(
        "module-runtime",
        &[
            ("src/lib.rs", "mod runtime;\npub use runtime::*;\n"),
            ("src/runtime.rs", RTC_RUNTIME_LIB),
        ],
    );
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );

    // `[lib] path` points at a non-default library root.
    let dir = tempfile::tempdir().expect("temporary runtime crate should be created");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"custom-lib-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"runtime/lib.rs\"\n",
    )
    .expect("manifest should be written");
    std::fs::create_dir_all(dir.path().join("runtime")).expect("lib dir");
    std::fs::write(dir.path().join("runtime/lib.rs"), RTC_RUNTIME_LIB).expect("lib source");
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc_rules_match_test_package_names_by_whole_token() {
    for package in [
        "mychain-testnet-runtime",
        "contest-runtime",
        "mockingbird-runtime",
    ] {
        let krate = rtc_runtime_crate(package, &[("src/lib.rs", RTC_RUNTIME_LIB)]);
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert_eq!(
            rtc_findings(&diags),
            vec!["RTC001:3".to_string()],
            "{package}: {diags:?}"
        );
    }

    let krate = rtc_runtime_crate("xcm_mock_runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");

    // The token list is configurable: an empty list lints test runtimes too.
    let krate = rtc_runtime_crate("polkadot-test-runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let mut config = polkadot_linter::config::Config::default();
    config.runtime_config.test_runtime_tokens.clear();
    let diags = rtc_scan(krate.path(), config);
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc_rules_skip_files_behind_test_or_benchmark_module_declarations() {
    let placeholder =
        "impl pallet_sudo::Config for Runtime {\n    type WeightInfo = TestWeightInfo;\n}\n";
    let lib = "#[cfg(test)]\nmod sandbox;\n#[cfg(feature = \"runtime-benchmarks\")]\nmod bench_config;\nmod configs;\nconstruct_runtime!(pub enum Runtime { Sudo: pallet_sudo = 15 });\n";
    let krate = rtc_runtime_crate(
        "sandbox-runtime",
        &[
            ("src/lib.rs", lib),
            ("src/sandbox.rs", placeholder),
            ("src/bench_config.rs", placeholder),
            ("src/configs/mod.rs", placeholder),
        ],
    );
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    let findings = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id.starts_with("RTC"))
        .map(|diagnostic| {
            diagnostic
                .file
                .strip_prefix(krate.path())
                .unwrap_or(&diagnostic.file)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        findings,
        vec!["src/configs/mod.rs".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc_rules_lint_runtimes_switched_by_feature() {
    let decl =
        "construct_runtime!(\n    pub enum Runtime {\n        Sudo: pallet_sudo = 15,\n    }\n);\n";
    let config =
        "impl pallet_sudo::Config for Runtime {\n    type WeightInfo = TestWeightInfo;\n}\n";
    // Both declarations gated by `feature` (directly, inside `not`, or via
    // an enclosing module or file): the crate is still a runtime.
    for lib in [
        format!("{config}#[cfg(feature = \"with-evm\")]\n{decl}#[cfg(not(feature = \"with-evm\"))]\n{decl}"),
        format!("{config}#[cfg(any(feature = \"a\", test))]\nmod a {{\n{decl}}}\n#[cfg(not(any(feature = \"a\", test)))]\nmod b {{\n{decl}}}\n"),
        format!("{config}#[cfg(feature = \"a\")]\nmod a;\n#[cfg(not(feature = \"a\"))]\nmod b;\n"),
    ] {
        let krate = rtc_runtime_crate(
            "switched-runtime",
            &[
                ("src/lib.rs", lib.as_str()),
                ("src/a.rs", decl),
                ("src/b.rs", &format!("#![cfg(feature = \"x\")]\n{decl}")),
            ],
        );
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert_eq!(rtc_findings(&diags), vec!["RTC001:2".to_string()], "{lib}\n{diags:?}");
    }

    // Two ungated runtimes (an XCM simulator) stay excluded, even next to a
    // feature-gated one.
    let lib =
        format!("{config}mod a {{\n{decl}}}\nmod b {{\n{decl}}}\n#[cfg(feature = \"c\")]\n{decl}");
    let krate = rtc_runtime_crate("simulator", &[("src/lib.rs", lib.as_str())]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc_rules_treat_aliases_redefined_in_nested_modules_as_ambiguous() {
    // RTC003: the outer waiver is `Everything`; a nested `Nothing` must not
    // hide it.
    let xcm = "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n\
type WaivedLocations = Everything;\n\
impl xcm_executor::Config for XcmConfig {\n    type FeeManager = XcmFeeManagerFromComponents<WaivedLocations, ()>;\n}\n\
mod nested {\n    type WaivedLocations = Nothing;\n}\n";
    let diags = check_fixture("runtime/src/xcm_config.rs", xcm);
    assert_eq!(rtc_lines(&diags, "RTC003"), vec![4], "{diags:?}");

    // RTC004: the outer origin is signed; a nested `EnsureRoot` must not
    // exempt the zero deposit.
    let lib = "construct_runtime!(pub enum Runtime { Referenda: pallet_referenda = 1 });\n\
type SubmissionOrigin = EnsureSigned<AccountId>;\n\
impl pallet_referenda::Config for Runtime {\n    type SubmitOrigin = SubmissionOrigin;\n    type SubmissionDeposit = ConstU128<0>;\n}\n\
mod nested {\n    type SubmissionOrigin = EnsureRoot<AccountId>;\n}\n";
    let diags = check_fixture("runtime/src/lib.rs", lib);
    assert_eq!(rtc_lines(&diags, "RTC004"), vec![5], "{diags:?}");
}

#[test]
fn rtc011_reports_remove_pallet_of_a_live_pallet() {
    let bad = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc011.rs"));
    assert_eq!(rtc_lines(&bad, "RTC011"), vec![11], "{bad:?}");
    let good = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc011.rs"),
    );
    assert!(rtc_lines(&good, "RTC011").is_empty(), "{good:?}");
}

#[test]
fn sem020_reports_revive_precompile_without_delegate_guard() {
    let bad = check_fixture(
        "precompiles/src/lib.rs",
        include_str!("fixtures/bad_sem020.rs"),
    );
    assert_eq!(rtc_lines(&bad, "SEM020"), vec![3], "{bad:?}");
    let good = check_fixture(
        "precompiles/src/lib.rs",
        include_str!("fixtures/good_sem020.rs"),
    );
    assert!(rtc_lines(&good, "SEM020").is_empty(), "{good:?}");
}

/// A `#[cfg(predicate)]` runtime declaration with `pallet_xcm` and an `Extra`
/// pallet from `extra`.
fn rtc003_declaration(predicate: &str, extra: &str) -> String {
    format!(
        "#[cfg({predicate})]\nconstruct_runtime!(pub enum Runtime {{\n    System: frame_system = 0,\n    \
         PolkadotXcm: pallet_xcm = 31,\n    Extra: {extra} = 32,\n}});\n"
    )
}

#[test]
fn rtc003_downgrades_fee_manager_when_local_pallet_xcm_is_privileged_only() {
    let locked_down = r#"
construct_runtime!(pub enum Runtime {
    System: frame_system = 0,
    PolkadotXcm: pallet_xcm = 31,
});
type Blocked = Nothing;
type NoXcmOrigins = ();
impl pallet_xcm::Config for Runtime {
    type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, NoXcmOrigins>;
    type XcmExecuteFilter = Blocked;
    type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
    type XcmTeleportFilter = Nothing;
    type XcmReserveTransferFilter = Nothing;
}
impl cumulus_pallet_xcmp_queue::Config for Runtime {
    type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
}
impl xcm_executor::Config for XcmConfig {
    type XcmSender = XcmRouter;
    type MessageExporter = ();
    type FeeManager = WaiveDeliveryFees;
}
"#;
    let diags = check_fixture("runtime/src/xcm_config.rs", locked_down);
    let rtc003: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC003").collect();
    assert_eq!(
        rtc003.len(),
        2,
        "neither router price nor FeeManager is dropped: {rtc003:?}"
    );
    // Router delivery price applies to all senders (including inbound XCM) -> stays Medium.
    assert_eq!(rtc003[0].line, 16);
    assert_eq!(
        rtc003[0].confidence,
        polkadot_linter::diagnostics::Confidence::Medium
    );
    // FeeManager finding is kept but downgraded to Low confidence with an explanation.
    assert_eq!(rtc003[1].line, 21);
    assert_eq!(
        rtc003[1].confidence,
        polkadot_linter::diagnostics::Confidence::Low
    );
    assert!(
        rtc003[1].message.contains("privileged-only"),
        "{}",
        rtc003[1].message
    );

    // A pallet whose `XcmSender` is a same-module `()` alias sends nothing.
    let unit_sender = format!(
        "{locked_down}\ntype Router = ();\nimpl pallet_custom::Config for Runtime {{\n    type XcmSender = Router;\n}}\n"
    );
    let diags = check_fixture("runtime/src/xcm_config.rs", &unit_sender);
    let rtc003: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC003").collect();
    assert_eq!(
        rtc003[1].confidence,
        polkadot_linter::diagnostics::Confidence::Low,
        "{rtc003:?}"
    );

    // Opening execute via a #[cfg] alternative keeps FeeManager at Medium confidence.
    let cfg_open_execute = locked_down.replace(
        "type XcmExecuteFilter = Blocked;",
        "#[cfg(not(feature = \"x\"))]\n    type XcmExecuteFilter = Blocked;\n    #[cfg(feature = \"x\")]\n    type XcmExecuteFilter = Everything;",
    );
    let cfg_open_execute = cfg_open_execute.replace(
        "type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;",
        "type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;",
    );
    let diags = check_fixture("runtime/src/xcm_config.rs", &cfg_open_execute);
    let rtc003: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC003").collect();
    assert_eq!(rtc003.len(), 2, "{rtc003:?}");
    assert!(
        rtc003
            .iter()
            .all(|d| d.confidence == polkadot_linter::diagnostics::Confidence::Medium),
        "cfg-open execute path must keep Medium confidence: {rtc003:?}"
    );

    // Declaring another XCM-sending pallet (or a pallet with an XcmSender) keeps Medium confidence.
    for variant in [
        locked_down.replace(
            "PolkadotXcm: pallet_xcm = 31,",
            "PolkadotXcm: pallet_xcm = 31,\n    XcmTransactor: pallet_xcm_transactor = 32,",
        ),
        format!(
            "{locked_down}\nimpl pallet_custom::Config for Runtime {{\n    type XcmSender = XcmRouter;\n}}\n"
        ),
        // A `()` alias in another module does not describe this module's name.
        format!(
            "{locked_down}\nmod other {{\n    pub type Router = ();\n}}\n\
             impl pallet_custom::Config for Runtime {{\n    type XcmSender = Router;\n}}\n"
        ),
        locked_down.replace(
            "type NoXcmOrigins = ();",
            "mod other {\n    pub type NoXcmOrigins = ();\n}",
        ),
        // A `()` alias with a `#[cfg]` alternative that imports a real
        // converter under the same name does not resolve.
        locked_down.replace(
            "type NoXcmOrigins = ();",
            "#[cfg(feature = \"locked\")]\ntype NoXcmOrigins = ();\n\
             #[cfg(not(feature = \"locked\"))]\n\
             use xcm_builder::SignedToAccountId32 as NoXcmOrigins;",
        ),
        // A `pallet_xcm_transactor` in one `#[cfg]` alternative of the runtime
        // declaration sends XCM, whichever alternative comes last.
        locked_down.replace(
            "construct_runtime!(pub enum Runtime {\n    System: frame_system = 0,\n    PolkadotXcm: pallet_xcm = 31,\n});",
            &format!("{}{}", rtc003_declaration("feature = \"a\"", "pallet_xcm_transactor"), rtc003_declaration("not(feature = \"a\")", "pallet_custom")),
        ),
        locked_down.replace(
            "construct_runtime!(pub enum Runtime {\n    System: frame_system = 0,\n    PolkadotXcm: pallet_xcm = 31,\n});",
            &format!("{}{}", rtc003_declaration("not(feature = \"a\")", "pallet_custom"), rtc003_declaration("feature = \"a\"", "pallet_xcm_transactor")),
        ),
    ] {
        let diags = check_fixture("runtime/src/xcm_config.rs", &variant);
        let rtc003: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC003").collect();
        assert_eq!(rtc003.len(), 2, "{rtc003:?}");
        assert!(
            rtc003
                .iter()
                .all(|d| d.confidence == polkadot_linter::diagnostics::Confidence::Medium),
            "other outbound XCM sender must keep Medium confidence: {rtc003:?}"
        );
    }
}

#[test]
fn rtc008_value_escalations_only_filters_non_value_superset_mismatches() {
    let bad = include_str!("fixtures/bad_rtc008.rs");
    let mut cfg = all_rules_config();
    cfg.runtime_config.rtc008_value_escalations_only = Some(true);
    let confidences = |diags: &[polkadot_linter::diagnostics::Diagnostic]| {
        diags
            .iter()
            .filter(|d| d.rule_id == "RTC008")
            .map(|d| (d.line, d.confidence))
            .collect::<Vec<_>>()
    };
    use polkadot_linter::diagnostics::Confidence::{High, Medium};

    // `CollatorSelection` reserves candidacy bonds, so its superset mismatch
    // (fellows runtimes #1341) is high impact and survives the filter.
    let diags = check_fixture_with_config("runtime/src/lib.rs", bad, &cfg);
    assert_eq!(
        confidences(&diags),
        vec![(20, High), (60, High)],
        "{diags:?}"
    );

    // `Timestamp` is operational-only: reported at medium confidence by
    // default, suppressed with rtc008_value_escalations_only.
    let with_pallet = |declaration: &str, variant: &str| {
        bad.replace(
            "CollatorSelection: pallet_collator_selection = 21,",
            &format!("CollatorSelection: pallet_collator_selection = 21,\n\t\t{declaration}"),
        )
        .replace(
            "RuntimeCall::CollatorSelection(..)",
            &format!("RuntimeCall::{variant}(..)"),
        )
    };
    let timestamp = with_pallet("Timestamp: pallet_timestamp = 22,", "Timestamp");
    let diags = check_fixture("runtime/src/lib.rs", &timestamp);
    assert_eq!(
        confidences(&diags),
        vec![(21, High), (61, Medium)],
        "{diags:?}"
    );
    let diags = check_fixture_with_config("runtime/src/lib.rs", &timestamp, &cfg);
    assert_eq!(confidences(&diags), vec![(21, High)], "{diags:?}");

    // `pallet_session::set_keys` holds a `KeyDeposit` and replaces collator
    // keys, and a variant without a visible crate may be any pallet, so both
    // are high impact.
    for (declaration, variant) in [
        ("Session: pallet_session = 22,", "Session"),
        ("", "Timestamp"),
        ("", "Session"),
    ] {
        let runtime = with_pallet(declaration, variant);
        let diags = check_fixture_with_config("runtime/src/lib.rs", &runtime, &cfg);
        assert_eq!(
            confidences(&diags).last(),
            Some(&(61, High)),
            "{declaration} {variant}: {diags:?}"
        );
    }

    // Mixed-order escalation (`Timestamp | Balances`): must inspect all
    // uncovered patterns and report the high-impact witness (`Balances`).
    let mixed = timestamp.replace(
        "RuntimeCall::Timestamp(..)",
        "RuntimeCall::Timestamp(..) | RuntimeCall::Balances(..)",
    );
    let diags = check_fixture_with_config("runtime/src/lib.rs", &mixed, &cfg);
    let rtc008 = diags
        .iter()
        .filter(|d| d.rule_id == "RTC008")
        .collect::<Vec<_>>();
    assert_eq!(
        confidences(&diags),
        vec![(21, High), (61, High)],
        "{diags:?}"
    );
    assert!(
        rtc008[1].message.contains("`Balances`"),
        "must report high-impact witness over Timestamp: {}",
        rtc008[1].message
    );

    // Unknown/custom pallets (e.g. `Staking`, `Dex`) and renamed privilege
    // pallets (`Admin: pallet_sudo`) fail closed as high impact.
    for replacement in [
        "RuntimeCall::Staking(..)",
        "RuntimeCall::CustomDex(..)",
        "RuntimeCall::Admin(..)",
    ] {
        let mutated = timestamp
            .replace(
                "Balances: pallet_balances = 10,",
                "Balances: pallet_balances = 10,\n        Admin: pallet_sudo = 11,",
            )
            .replace("RuntimeCall::Timestamp(..)", replacement);
        let diags = check_fixture_with_config("runtime/src/lib.rs", &mutated, &cfg);
        assert_eq!(
            confidences(&diags).len(),
            2,
            "{replacement} must fail closed as high impact: {diags:?}"
        );
        assert_eq!(confidences(&diags)[1].1, High, "{replacement}: {diags:?}");
    }
}

/// A directory holding one `RTC_RUNTIME_LIB` runtime crate per package name.
fn rtc_runtime_crates(packages: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    for package in packages {
        let root = dir.path().join(package);
        std::fs::create_dir_all(root.join("src")).expect("source dir should be created");
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        )
        .expect("manifest should be written");
        std::fs::write(root.join("src/lib.rs"), RTC_RUNTIME_LIB).expect("source should be written");
    }
    dir
}

/// Packages with an RTC001 finding in a scan of `root`.
fn rtc001_packages(root: &std::path::Path, config: polkadot_linter::config::Config) -> Vec<String> {
    let mut packages = rtc_scan(root, config)
        .iter()
        .filter(|d| d.rule_id == "RTC001")
        .filter_map(|d| d.file.strip_prefix(root).ok()?.iter().next())
        .map(|name| name.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    packages.sort();
    packages
}

#[test]
fn rtc008_judges_a_pallet_by_every_cfg_alternative_of_its_crate() {
    use polkadot_linter::diagnostics::Confidence::High;
    let bad = include_str!("fixtures/bad_rtc008.rs");
    let filters = &bad[bad.find("impl InstanceFilter").unwrap()..];
    let filters = filters.replace(
        "RuntimeCall::CollatorSelection(..)",
        "RuntimeCall::Session(..)",
    );
    let declaration = |predicate: &str, session: &str| {
        format!(
            "#[cfg({predicate})]\nconstruct_runtime!(\n    pub enum Runtime {{\n        \
             System: frame_system = 0,\n        Balances: pallet_balances = 10,\n        \
             Session: {session} = 21,\n        PolkadotXcm: pallet_xcm = 31,\n        \
             Utility: pallet_utility = 40,\n        Assets: pallet_assets::<Instance1> = 50,\n    }}\n);\n"
        )
    };
    let collator = declaration("feature = \"collator\"", "pallet_collator_selection");
    let session = declaration("not(feature = \"collator\")", "pallet_timestamp");
    let mut cfg = all_rules_config();
    cfg.runtime_config.rtc008_value_escalations_only = Some(true);
    // `Session` is `pallet_collator_selection` in one alternative, so the
    // escalation is high impact in either declaration order.
    for runtime in [
        format!("{collator}{session}{filters}"),
        format!("{session}{collator}{filters}"),
    ] {
        for config in [all_rules_config(), cfg.clone()] {
            let diags = check_fixture_with_config("runtime/src/lib.rs", &runtime, &config);
            let rtc008 = diags
                .iter()
                .filter(|d| d.rule_id == "RTC008")
                .map(|d| d.confidence)
                .collect::<Vec<_>>();
            assert_eq!(rtc008, vec![High, High], "{diags:?}");
        }
    }
}

#[test]
fn rtc_rules_exclude_testnet_runtimes_when_configured() {
    let mut cfg = polkadot_linter::config::Config::default();
    cfg.runtime_config.exclude_testnet_runtimes = Some(true);

    let dir = rtc_runtime_crates(&[
        "mychain-testnet-runtime",
        "westend-runtime",
        "paseo-runtime",
        "polkadot-runtime",
    ]);
    assert_eq!(
        rtc001_packages(dir.path(), cfg.clone()),
        vec!["polkadot-runtime"],
        "testnet runtimes are skipped next to a production runtime"
    );
    assert_eq!(
        rtc001_packages(dir.path(), polkadot_linter::config::Config::default()).len(),
        4,
        "testnet runtimes are linted by default"
    );

    // A scan whose runtimes are all testnets lints them anyway.
    let testnets = rtc_runtime_crates(&["westend-runtime", "paseo-runtime"]);
    assert_eq!(
        rtc001_packages(testnets.path(), cfg),
        vec!["paseo-runtime", "westend-runtime"]
    );
}

#[test]
fn rtc009_downgrades_unreachable_crates_and_preserves_external_and_unrooted_errors() {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let write = |rel: &str, content: &str| {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("dir should be created");
        std::fs::write(path, content).expect("file should be written");
    };
    write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"prod-runtime\", \"dev-runtime\", \"cli-tool\"]\n",
    );
    write(
        "prod-runtime/Cargo.toml",
        "[package]\nname = \"prod-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        "prod-runtime/src/lib.rs",
        "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n",
    );
    write(
        "dev-runtime/Cargo.toml",
        "[package]\nname = \"dev-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        "dev-runtime/src/lib.rs",
        "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n",
    );
    write(
        "cli-tool/Cargo.toml",
        "[package]\nname = \"cli-tool\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write("cli-tool/src/lib.rs", "pub fn main() {}\n");

    // Case 1: Only dev-runtime and cli-tool depend on vulnerable staging-xcm-executor 28.0.1.
    // prod-runtime uses a git-patched 28.0.1 (different source), so source
    // disambiguation keeps the crates.io entry unreachable -> downgraded to Advisory/Low.
    let unreachable_lock = r#"version = 4

[[package]]
name = "prod-runtime"
version = "0.1.0"
dependencies = [
 "serde",
 "staging-xcm-executor 28.0.1 (git+https://github.com/example/polkadot-sdk)",
]

[[package]]
name = "dev-runtime"
version = "0.1.0"
dependencies = [
 "staging-xcm-executor 28.0.1 (registry+https://github.com/rust-lang/crates.io-index)",
]

[[package]]
name = "cli-tool"
version = "0.1.0"
dependencies = [
 "staging-xcm-executor 28.0.1 (registry+https://github.com/rust-lang/crates.io-index)",
]

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "staging-xcm-executor"
version = "28.0.1"
source = "git+https://github.com/example/polkadot-sdk#abc"

[[package]]
name = "staging-xcm-executor"
version = "28.0.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;
    write("Cargo.lock", unreachable_lock);
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    let rtc009: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC009").collect();
    assert_eq!(rtc009.len(), 1, "{rtc009:?}");
    assert_eq!(
        rtc009[0].severity,
        polkadot_linter::diagnostics::Severity::Advisory
    );
    assert_eq!(
        rtc009[0].confidence,
        polkadot_linter::diagnostics::Confidence::Low
    );
    assert!(
        rtc009[0].message.contains("not reachable"),
        "{}",
        rtc009[0].message
    );

    // Case 2: External git runtime in Cargo.lock pulls the crates.io executor -> Error/High.
    let with_external_runtime = format!(
        "{unreachable_lock}\n[[package]]\nname = \"mychain-runtime\"\nversion = \"0.1.0\"\n\
         source = \"git+https://github.com/example/mychain#def\"\n\
         dependencies = [\n \"staging-xcm-executor 28.0.1 (registry+https://github.com/rust-lang/crates.io-index)\",\n \
         \"substrate-wasm-builder\",\n]\n\n[[package]]\nname = \"substrate-wasm-builder\"\nversion = \"1.0.0\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
    );
    write("Cargo.lock", &with_external_runtime);
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    let rtc009: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC009").collect();
    assert_eq!(rtc009.len(), 1, "{rtc009:?}");
    assert_eq!(
        rtc009[0].severity,
        polkadot_linter::diagnostics::Severity::Error
    );
    assert_eq!(
        rtc009[0].confidence,
        polkadot_linter::diagnostics::Confidence::High
    );

    // Case 3: Workspace with only excluded (dev) runtimes falls back to full Error/High reporting.
    let dev_only = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dev_only.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"dev-runtime\"]\n",
    )
    .unwrap();
    std::fs::create_dir_all(dev_only.path().join("dev-runtime/src")).unwrap();
    std::fs::write(
        dev_only.path().join("dev-runtime/Cargo.toml"),
        "[package]\nname = \"dev-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        dev_only.path().join("dev-runtime/src/lib.rs"),
        "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n",
    )
    .unwrap();
    std::fs::write(
        dev_only.path().join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"dev-runtime\"\nversion = \"0.1.0\"\n\
         dependencies = [\"staging-xcm-executor\"]\n\n[[package]]\n\
         name = \"staging-xcm-executor\"\nversion = \"28.0.1\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n",
    )
    .unwrap();
    let diags = rtc_scan(dev_only.path(), polkadot_linter::config::Config::default());
    let rtc009: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC009").collect();
    assert_eq!(rtc009.len(), 1, "{rtc009:?}");
    assert_eq!(
        rtc009[0].severity,
        polkadot_linter::diagnostics::Severity::Error
    );
}

#[test]
fn rtc009_downgrades_withdraw_asset_when_all_reaching_runtimes_have_unit_asset_transactor() {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let write = |rel: &str, content: &str| {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("dir should be created");
        std::fs::write(path, content).expect("file should be written");
    };
    write("Cargo.toml", "[workspace]\nmembers = [\"nexus-runtime\"]\n");
    write(
        "nexus-runtime/Cargo.toml",
        "[package]\nname = \"nexus-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    // Uses both `staging_xcm_executor::Config` and a type alias to `()`.
    write(
        "nexus-runtime/src/lib.rs",
        r#"
construct_runtime!(pub enum Runtime { System: frame_system = 0 });
type LocalAssetTransactor = ();
impl staging_xcm_executor::Config for NexusXcm {
    type AssetTransactor = LocalAssetTransactor;
}
impl pallet_xcm::Config for Runtime {
    type XcmExecutor = XcmExecutor<NexusXcm>;
}
"#,
    );
    let lock = |executor_version: &str| {
        format!(
            r#"version = 4

[[package]]
name = "nexus-runtime"
version = "0.1.0"
dependencies = ["staging-xcm-executor"]

[[package]]
name = "staging-xcm-executor"
version = "{executor_version}"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#
        )
    };

    // 28.0.1 has precondition=asset_transactor -> downgraded to advisory/low.
    write("Cargo.lock", &lock("28.0.1"));
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    let rtc009: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC009").collect();
    assert_eq!(rtc009.len(), 1, "{rtc009:?}");
    assert_eq!(
        rtc009[0].severity,
        polkadot_linter::diagnostics::Severity::Advisory
    );
    assert_eq!(
        rtc009[0].confidence,
        polkadot_linter::diagnostics::Confidence::Low
    );
    assert!(
        rtc009[0].message.contains("AssetTransactor = ()"),
        "{}",
        rtc009[0].message
    );

    // Adding a #[cfg] non-unit AssetTransactor variant fails closed to Error/High.
    write(
        "nexus-runtime/src/lib.rs",
        r#"
construct_runtime!(pub enum Runtime { System: frame_system = 0 });
impl staging_xcm_executor::Config for NexusXcm {
    #[cfg(not(feature = "real"))]
    type AssetTransactor = ();
    #[cfg(feature = "real")]
    type AssetTransactor = FungibleAdapter;
}
impl pallet_xcm::Config for Runtime {
    type XcmExecutor = XcmExecutor<NexusXcm>;
}
"#,
    );
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    let rtc009: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC009").collect();
    assert_eq!(rtc009.len(), 1, "{rtc009:?}");
    assert_eq!(
        rtc009[0].severity,
        polkadot_linter::diagnostics::Severity::Error
    );

    // 19.1.2 is InitiateTransfer (precondition = -) -> NOT mitigated by AssetTransactor = ().
    write(
        "nexus-runtime/src/lib.rs",
        r#"
construct_runtime!(pub enum Runtime { System: frame_system = 0 });
impl xcm_executor::Config for XcmConfig {
    type AssetTransactor = ();
}
impl pallet_xcm::Config for Runtime {
    type XcmExecutor = XcmExecutor<XcmConfig>;
}
"#,
    );
    write("Cargo.lock", &lock("19.1.2"));
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    let rtc009: Vec<_> = diags.iter().filter(|d| d.rule_id == "RTC009").collect();
    assert_eq!(rtc009.len(), 1, "{rtc009:?}");
    assert_eq!(
        rtc009[0].severity,
        polkadot_linter::diagnostics::Severity::Error
    );
    assert_eq!(
        rtc009[0].confidence,
        polkadot_linter::diagnostics::Confidence::High
    );
}

/// Severity and confidence of every RTC009 finding in a scan of `root`.
fn rtc009_levels(
    root: &std::path::Path,
) -> Vec<(
    polkadot_linter::diagnostics::Severity,
    polkadot_linter::diagnostics::Confidence,
)> {
    rtc_scan(root, polkadot_linter::config::Config::default())
        .iter()
        .filter(|d| d.rule_id == "RTC009")
        .map(|d| (d.severity, d.confidence))
        .collect()
}

/// A workspace with one local runtime crate `package` whose `src/lib.rs` is
/// `lib`, and `lock` as its `Cargo.lock`.
fn rtc009_workspace(package: &str, lib: &str, lock: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let write = |rel: &str, content: &str| {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("dir should be created");
        std::fs::write(path, content).expect("file should be written");
    };
    write(
        "Cargo.toml",
        &format!("[workspace]\nmembers = [\"{package}\"]\n"),
    );
    write(
        &format!("{package}/Cargo.toml"),
        &format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    );
    write(&format!("{package}/src/lib.rs"), lib);
    write("Cargo.lock", lock);
    dir
}

const RTC009_RUNTIME_LIB: &str =
    "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n";

const RTC009_EXECUTOR: &str = r#"
[[package]]
name = "staging-xcm-executor"
version = "28.0.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;

const RTC009_WASM_BUILDER: &str = r#"
[[package]]
name = "substrate-wasm-builder"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;

#[test]
fn rtc009_follows_git_dependency_edges_written_without_the_commit() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    // Cargo writes a git dependency spec without the `#<commit>` that the
    // package's own `source` has.
    let lock = |dep_source: &str| {
        format!(
            r#"version = 4

[[package]]
name = "prod-runtime"
version = "0.1.0"
dependencies = ["ethereum 0.18.2 ({dep_source})"]

[[package]]
name = "ethereum"
version = "0.18.2"
source = "git+https://github.com/example/ethereum?branch=x#0123abcd"
dependencies = ["staging-xcm-executor"]

[[package]]
name = "ethereum"
version = "0.18.2"
source = "registry+https://github.com/rust-lang/crates.io-index"
{RTC009_EXECUTOR}"#
        )
    };
    for dep_source in [
        "git+https://github.com/example/ethereum?branch=x",
        // A source that matches no package keeps every same-version edge.
        "git+https://github.com/other/ethereum?branch=x",
    ] {
        let dir = rtc009_workspace("prod-runtime", RTC009_RUNTIME_LIB, &lock(dep_source));
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Error, Confidence::High)],
            "{dep_source}"
        );
    }
    // The registry `ethereum` does not reach the executor, so a dependency
    // on it alone leaves the executor unreachable.
    let dir = rtc009_workspace(
        "prod-runtime",
        RTC009_RUNTIME_LIB,
        &lock("registry+https://github.com/rust-lang/crates.io-index"),
    );
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)]
    );
    // Only the registry decoy reaches the executor. The git spec without the
    // commit selects the git package alone, so the executor is unreachable.
    let decoy = lock("git+https://github.com/example/ethereum?branch=x")
        .replace(
            "source = \"git+https://github.com/example/ethereum?branch=x#0123abcd\"\ndependencies = [\"staging-xcm-executor\"]",
            "source = \"git+https://github.com/example/ethereum?branch=x#0123abcd\"",
        )
        .replace(
            "source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"staging-xcm-executor\"",
            "source = \"registry+https://github.com/rust-lang/crates.io-index\"\ndependencies = [\"staging-xcm-executor\"]\n\n[[package]]\nname = \"staging-xcm-executor\"",
        );
    assert_eq!(
        decoy
            .matches("dependencies = [\"staging-xcm-executor\"]")
            .count(),
        1
    );
    let dir = rtc009_workspace("prod-runtime", RTC009_RUNTIME_LIB, &decoy);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)]
    );
}

#[test]
fn rtc009_external_runtime_roots_need_a_wasm_builder_dependency() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    // Runtime libraries of the `sp-`, `bp-` and `frame-try-` families depend
    // on the executor in every parachain lockfile, but they are not runtimes.
    let libraries = [
        "sp-runtime",
        "sp-runtime-interface",
        "bp-runtime",
        "frame-try-runtime",
    ]
    .iter()
    .map(|name| {
        format!(
            "\n[[package]]\nname = \"{name}\"\nversion = \"1.0.0\"\n\
             source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\
             dependencies = [\"staging-xcm-executor\"]\n"
        )
    })
    .collect::<String>();
    let lock = format!(
        "version = 4\n\n[[package]]\nname = \"prod-runtime\"\nversion = \"0.1.0\"\n\
         dependencies = [\"substrate-wasm-builder\"]\n{libraries}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}"
    );
    let dir = rtc009_workspace("prod-runtime", RTC009_RUNTIME_LIB, &lock);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)]
    );

    // A registry package that builds a runtime WASM blob is a root.
    let remote_runtime = format!(
        "{lock}\n[[package]]\nname = \"asset-hub-polkadot-runtime\"\nversion = \"1.0.0\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\
         dependencies = [\"staging-xcm-executor\", \"substrate-wasm-builder\"]\n"
    );
    std::fs::write(dir.path().join("Cargo.lock"), remote_runtime).unwrap();
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Error, Confidence::High)]
    );

    // Without a builder edge, a package is a root when it depends on
    // `frame-executive` or has a `runtime` token anywhere in its name.
    let registry = Some("registry+https://github.com/rust-lang/crates.io-index");
    let git = Some("git+https://github.com/example/chain#0123abcd");
    for (case, package) in [
        (
            "git package on frame-executive",
            rtc009_package(
                "chain",
                "1.0.0",
                git,
                &["frame-executive", "staging-xcm-executor"],
            ),
        ),
        (
            "path package on frame-executive",
            rtc009_package(
                "chain",
                "1.0.0",
                None,
                &["frame-executive", "staging-xcm-executor"],
            ),
        ),
        (
            "leading runtime token",
            rtc009_package("runtime-eden", "1.0.0", git, &["staging-xcm-executor"]),
        ),
        (
            "inner runtime token",
            rtc009_package(
                "interlay-runtime-parachain",
                "1.0.0",
                git,
                &["staging-xcm-executor"],
            ),
        ),
        (
            "runtime library outside the sp-, bp- and frame-try- families",
            rtc009_package(
                "polkadot-runtime-common",
                "1.0.0",
                registry,
                &["staging-xcm-executor"],
            ),
        ),
    ] {
        let executive = rtc009_package("frame-executive", "1.0.0", registry, &[]);
        std::fs::write(
            dir.path().join("Cargo.lock"),
            format!("{lock}{executive}{package}"),
        )
        .unwrap();
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Error, Confidence::High)],
            "{case}"
        );
    }
}

#[test]
fn rtc009_skips_local_libraries_on_frame_executive_but_not_their_dependents() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let registry = Some("registry+https://github.com/rust-lang/crates.io-index");
    let manifest = |name: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
    };
    let lock = |pallet_deps: &[&str]| {
        format!(
            "version = 4\n{}{}{}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}",
            rtc009_package("prod-runtime", "0.1.0", None, &["substrate-wasm-builder"]),
            rtc009_package("pallet-local-runtime-tools", "0.1.0", None, pallet_deps),
            rtc009_package("frame-executive", "1.0.0", registry, &[]),
        )
    };
    let dir = rtc009_crates(
        "[workspace]\nmembers = [\"prod-runtime\", \"pallet\"]\n",
        &[
            (
                "prod-runtime",
                &manifest("prod-runtime"),
                RTC009_RUNTIME_LIB,
            ),
            (
                "pallet",
                &manifest("pallet-local-runtime-tools"),
                "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    \
                 construct_runtime!(pub enum Test { System: frame_system = 0 });\n}\n",
            ),
        ],
        &lock(&["frame-executive", "staging-xcm-executor"]),
    );
    // The parser saw the pallet's source, so its `frame-executive`
    // dev-dependency and inner `runtime` token do not make it a root.
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)]
    );
    // The text check skips a pallet without runtime text, or with its mock
    // runtime only in a test path. The parser did not read it, so the
    // `frame-executive` edge makes it a root.
    let pallet = dir.path().join("pallet/src");
    for (case, lib, mock) in [
        ("no runtime text", "pub fn f() {}\n", None),
        (
            "mock runtime in a test path",
            "pub fn f() {}\n#[cfg(test)]\nmod mock;\n",
            Some("construct_runtime!(pub enum Test { System: frame_system = 0 });\n"),
        ),
    ] {
        std::fs::write(pallet.join("lib.rs"), lib).unwrap();
        if let Some(mock) = mock {
            std::fs::write(pallet.join("mock.rs"), mock).unwrap();
        }
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Error, Confidence::High)],
            "{case}"
        );
    }
    // A builder edge still does.
    std::fs::write(
        dir.path().join("Cargo.lock"),
        lock(&["staging-xcm-executor", "substrate-wasm-builder"]),
    )
    .unwrap();
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Error, Confidence::High)]
    );
}

#[test]
fn rtc009_keeps_an_external_runtime_named_like_a_local_one_as_an_external_root() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    // The local runtime sets `AssetTransactor = ()`; the git runtime of the
    // same name is other code and must not borrow that mitigation.
    let lib = "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n\
               impl xcm_executor::Config for XcmConfig {\n    type AssetTransactor = ();\n}\n\
     impl pallet_xcm::Config for Runtime {\n    type XcmExecutor = XcmExecutor<XcmConfig>;\n}\n";
    let git_runtime = "\n[[package]]\nname = \"nexus-runtime\"\nversion = \"0.1.0\"\n\
                       source = \"git+https://github.com/example/nexus#0123abcd\"\n\
                       dependencies = [\"staging-xcm-executor\", \"substrate-wasm-builder\"]\n";
    let local_runtime = "\n[[package]]\nname = \"nexus-runtime\"\nversion = \"0.1.0\"\n\
                         dependencies = [\"substrate-wasm-builder\"]\n";
    for (case, lock) in [
        (
            "local and git runtime",
            format!(
                "version = 4\n{local_runtime}{git_runtime}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}"
            ),
        ),
        (
            "git runtime only",
            format!("version = 4\n{git_runtime}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}"),
        ),
        (
            // A path package of the same name outside the workspace, at
            // another version, is other code too.
            "local and same-name path runtime of another version",
            format!(
                "version = 4\n{local_runtime}{}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}",
                rtc009_package(
                    "nexus-runtime",
                    "0.2.0",
                    None,
                    &["staging-xcm-executor", "substrate-wasm-builder"]
                )
            ),
        ),
    ] {
        let dir = rtc009_workspace("nexus-runtime", lib, &lock);
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Error, Confidence::High)],
            "{case}"
        );
    }
    // Control: the local entry alone takes the local mitigation.
    let lock = format!("version = 4\n{local_runtime}{RTC009_WASM_BUILDER}").replace(
        "dependencies = [\"substrate-wasm-builder\"]",
        "dependencies = [\"substrate-wasm-builder\", \"staging-xcm-executor\"]",
    );
    let dir = rtc009_workspace("nexus-runtime", lib, &format!("{lock}{RTC009_EXECUTOR}"));
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    let rtc009 = diags
        .iter()
        .filter(|d| d.rule_id == "RTC009")
        .collect::<Vec<_>>();
    assert_eq!(rtc009.len(), 1, "{rtc009:?}");
    assert_eq!(rtc009[0].severity, Severity::Advisory);
    assert_eq!(rtc009[0].confidence, Confidence::Low);
    assert!(
        rtc009[0].message.contains("AssetTransactor = ()"),
        "{}",
        rtc009[0].message
    );
}

#[test]
fn rtc009_resolves_the_asset_transactor_alias_only_in_the_impl_module() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let lock = format!(
        "version = 4\n\n[[package]]\nname = \"nexus-runtime\"\nversion = \"0.1.0\"\n\
         dependencies = [\"staging-xcm-executor\"]\n{RTC009_EXECUTOR}"
    );
    let construct = "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n\
                     impl pallet_xcm::Config for Runtime {\n    type XcmExecutor = XcmExecutor<NexusXcm>;\n}\n";
    // `Transactor = ()` in an unrelated module does not describe the impl's
    // `Transactor`.
    let other_module = format!(
        "{construct}mod disabled {{\n    pub type Transactor = ();\n}}\n\
         impl staging_xcm_executor::Config for NexusXcm {{\n    type AssetTransactor = Transactor;\n}}\n"
    );
    let dir = rtc009_workspace("nexus-runtime", &other_module, &lock);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Error, Confidence::High)]
    );
    // The same alias next to the impl, inside an inline module, resolves.
    let same_module = format!(
        "{}mod xcm {{\n    type Transactor = ();\n\
         impl staging_xcm_executor::Config for NexusXcm {{\n        type AssetTransactor = Transactor;\n    }}\n}}\n",
        construct.replace("<NexusXcm>", "<xcm::NexusXcm>")
    );
    std::fs::write(dir.path().join("nexus-runtime/src/lib.rs"), same_module).unwrap();
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)]
    );
}

/// A workspace with every `(directory, manifest, lib.rs)` crate, the root
/// manifest `root_manifest` and `lock` as its `Cargo.lock`.
fn rtc009_crates(
    root_manifest: &str,
    crates: &[(&str, &str, &str)],
    lock: &str,
) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let write = |rel: &str, content: &str| {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("dir should be created");
        std::fs::write(path, content).expect("file should be written");
    };
    write("Cargo.toml", root_manifest);
    for (directory, manifest, lib) in crates {
        write(&format!("{directory}/Cargo.toml"), manifest);
        write(&format!("{directory}/src/lib.rs"), lib);
    }
    write("Cargo.lock", lock);
    dir
}

/// A `[[package]]` block of a `Cargo.lock`.
fn rtc009_package(name: &str, version: &str, source: Option<&str>, deps: &[&str]) -> String {
    let source = source.map_or(String::new(), |source| format!("source = \"{source}\"\n"));
    let deps = deps
        .iter()
        .map(|dep| format!("\"{dep}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!("\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n{source}dependencies = [{deps}]\n")
}

const RTC009_UNIT_TRANSACTOR_LIB: &str =
    "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n\
     impl xcm_executor::Config for XcmConfig {\n    type AssetTransactor = ();\n}\n\
     impl pallet_xcm::Config for Runtime {\n    type XcmExecutor = XcmExecutor<XcmConfig>;\n}\n";

#[test]
fn rtc009_keeps_the_error_when_the_asset_transactor_alias_is_not_unique() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let lock = format!(
        "version = 4\n{}{RTC009_EXECUTOR}",
        rtc009_package("nexus-runtime", "0.1.0", None, &["staging-xcm-executor"])
    );
    let construct = "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n\
                     impl pallet_xcm::Config for Runtime {\n    type XcmExecutor = XcmExecutor<NexusXcm>;\n}\n";
    let executor = "impl staging_xcm_executor::Config for NexusXcm {\n    \
                    type AssetTransactor = Transactor;\n}\n";
    let lib = |aliases: &str| format!("{construct}{aliases}{executor}");
    let dir = rtc009_workspace("nexus-runtime", &lib("type Transactor = ();\n"), &lock);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)],
        "a single ungated `()` alias resolves"
    );
    let off = "#[cfg(feature = \"off\")]\ntype Transactor = ();\n#[cfg(not(feature = \"off\"))]\n";
    let on = "type Transactor = ();\n#[cfg(feature = \"on\")]\n";
    for aliases in [
        format!("{off}use real::Transactor;\n"),
        format!("{off}pub struct Transactor;\n"),
        format!("{off}pub enum Transactor {{}}\n"),
        format!("{off}use real::*;\n"),
        // The other definition is not visible to the parser.
        format!("{off}include!(\"transactor.rs\");\n"),
        format!("{on}use real::Transactor;\n"),
        format!("{on}use real::{{self as Transactor}};\n"),
        format!("{on}use real::Adapter as Transactor;\n"),
        format!("{on}pub struct Transactor;\n"),
        "type Transactor = ();\nuse real::*;\n".to_string(),
    ] {
        std::fs::write(dir.path().join("nexus-runtime/src/lib.rs"), lib(&aliases)).unwrap();
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Error, Confidence::High)],
            "{aliases}"
        );
    }
    // A generic parameter or a block item shadows the module alias.
    for shadowed in [
        "type Transactor = ();\nimpl<Transactor: TransactAsset> staging_xcm_executor::Config \
         for NexusXcm<Transactor> {\n    type AssetTransactor = Transactor;\n}\n",
        "type Transactor = ();\nfn configure() {\n    type Transactor = FungibleAdapter;\n    \
         impl staging_xcm_executor::Config for NexusXcm {\n        \
         type AssetTransactor = Transactor;\n    }\n}\n",
    ] {
        std::fs::write(
            dir.path().join("nexus-runtime/src/lib.rs"),
            format!("{construct}{shadowed}"),
        )
        .unwrap();
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Error, Confidence::High)],
            "{shadowed}"
        );
    }
}

#[test]
fn rtc009_treats_unmatched_packages_that_build_wasm_as_external_roots() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let prod = rtc009_package("prod-runtime", "0.1.0", None, &["substrate-wasm-builder"]);
    let registry = Some("registry+https://github.com/rust-lang/crates.io-index");
    let git = Some("git+https://github.com/example/chain#0123abcd");
    let unreachable = format!("version = 4\n{prod}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}");
    let dir = rtc009_workspace("prod-runtime", RTC009_RUNTIME_LIB, &unreachable);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)]
    );
    for (case, packages) in [
        (
            "builds through the polkadot-sdk umbrella",
            format!(
                "{}{}",
                rtc009_package(
                    "chain",
                    "1.0.0",
                    git,
                    &["polkadot-sdk", "staging-xcm-executor"]
                ),
                rtc009_package(
                    "polkadot-sdk",
                    "1.0.0",
                    registry,
                    &["substrate-wasm-builder"]
                ),
            ),
        ),
        (
            // The name filters exclude the helper as a root, but the search
            // goes on to the package that depends on it.
            "builds through a filtered helper crate",
            format!(
                "{}{}",
                rtc009_package(
                    "chain",
                    "1.0.0",
                    git,
                    &["test-helper", "staging-xcm-executor"]
                ),
                rtc009_package(
                    "test-helper",
                    "1.0.0",
                    registry,
                    &["substrate-wasm-builder"]
                ),
            ),
        ),
        (
            "builds through two helper crates",
            format!(
                "{}{}{}",
                rtc009_package("chain", "1.0.0", git, &["helper-a", "staging-xcm-executor"]),
                rtc009_package("helper-a", "1.0.0", registry, &["helper-b"]),
                rtc009_package("helper-b", "1.0.0", registry, &["substrate-wasm-builder"]),
            ),
        ),
        (
            // A runtime the parser does not recognise, a symlinked member or a
            // path dependency outside the workspace.
            "source-less package with a builder edge",
            rtc009_package(
                "chain",
                "1.0.0",
                None,
                &["staging-xcm-executor", "substrate-wasm-builder"],
            ),
        ),
        (
            // Cargo drops a disabled optional builder dependency of a package
            // outside the workspace.
            "runtime-named git package without a builder edge",
            rtc009_package("chain-runtime", "1.0.0", git, &["staging-xcm-executor"]),
        ),
    ] {
        std::fs::write(
            dir.path().join("Cargo.lock"),
            format!("{unreachable}{packages}"),
        )
        .unwrap();
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Error, Confidence::High)],
            "{case}"
        );
    }
}

#[test]
fn rtc009_keeps_the_error_when_a_local_runtime_is_not_matched_exactly() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let manifest = |name: &str, version: &str| {
        format!("[package]\nname = \"{name}\"\n{version}\nedition = \"2021\"\n")
    };
    let members = "[workspace]\nmembers = [\"a-runtime\", \"b-chain\"]\n";
    let a = rtc009_package("a-runtime", "0.1.0", None, &[]);

    // A stale lockfile: `b-chain` is 0.2.0 in its manifest, so the lock does
    // not show what it reaches.
    let lock = format!(
        "version = 4\n{a}{}{RTC009_EXECUTOR}",
        rtc009_package("b-chain", "0.1.0", None, &["staging-xcm-executor"])
    );
    let a_manifest = manifest("a-runtime", "version = \"0.1.0\"");
    for (case, b_version) in [
        ("b-chain reaches the executor", "version = \"0.1.0\""),
        ("stale lockfile", "version = \"0.2.0\""),
    ] {
        let b_manifest = manifest("b-chain", b_version);
        let crates = [
            ("a-runtime", a_manifest.as_str(), RTC009_RUNTIME_LIB),
            ("b-chain", b_manifest.as_str(), RTC009_RUNTIME_LIB),
        ];
        let dir = rtc009_crates(members, &crates, &lock);
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Error, Confidence::High)],
            "{case}"
        );
    }

    // `version.workspace = true` resolves from `[workspace.package]`, so the
    // unit transactor of `nexus-runtime` 0.2.0 does not describe the
    // same-name path package 0.1.0 outside the workspace.
    let workspace = "[workspace]\nmembers = [\"nexus-runtime\"]\n\
                     [workspace.package]\nversion = \"0.2.0\"\n";
    let inherited = manifest("nexus-runtime", "version.workspace = true");
    let crates = [(
        "nexus-runtime",
        inherited.as_str(),
        RTC009_UNIT_TRANSACTOR_LIB,
    )];
    let local = rtc009_package("nexus-runtime", "0.2.0", None, &["substrate-wasm-builder"]);
    let outside = rtc009_package(
        "nexus-runtime",
        "0.1.0",
        None,
        &["staging-xcm-executor", "substrate-wasm-builder"],
    );
    let lock = format!("version = 4\n{local}{outside}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}");
    let dir = rtc009_crates(workspace, &crates, &lock);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Error, Confidence::High)],
        "inherited version"
    );
    // The resolved version picks the local entry out of two.
    let reaching = rtc009_package(
        "nexus-runtime",
        "0.2.0",
        None,
        &["staging-xcm-executor", "substrate-wasm-builder"],
    );
    let idle = rtc009_package("nexus-runtime", "0.1.0", None, &[]);
    let lock = format!("version = 4\n{reaching}{idle}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}");
    let dir = rtc009_crates(workspace, &crates, &lock);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)],
        "inherited version, local entry"
    );
    // Without `[workspace.package]` the version is unknown: one source-less
    // entry of the name matches, two match nothing.
    let workspace = "[workspace]\nmembers = [\"nexus-runtime\"]\n";
    let only = rtc009_package(
        "nexus-runtime",
        "0.1.0",
        None,
        &["staging-xcm-executor", "substrate-wasm-builder"],
    );
    let lock = format!("version = 4\n{only}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}");
    let dir = rtc009_crates(workspace, &crates, &lock);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)],
        "unknown version, one entry"
    );
    let lock = format!("version = 4\n{local}{outside}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}");
    let dir = rtc009_crates(workspace, &crates, &lock);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Error, Confidence::High)],
        "unknown version, two entries"
    );

    // Two parsed crates match one entry: the unit transactor of one does not
    // describe the entry, even when the filters exclude the other.
    let workspace = "[workspace]\nmembers = [\"nexus-runtime\"]\n";
    let nexus = manifest("nexus-runtime", "version = \"0.1.0\"");
    let crates = [
        ("nexus-runtime", nexus.as_str(), RTC009_UNIT_TRANSACTOR_LIB),
        ("nexus-dev", nexus.as_str(), RTC009_RUNTIME_LIB),
    ];
    let lock = format!(
        "version = 4\n{}{RTC009_EXECUTOR}",
        rtc009_package("nexus-runtime", "0.1.0", None, &["staging-xcm-executor"])
    );
    let dir = rtc009_crates(workspace, &crates, &lock);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Error, Confidence::High)],
        "two parsed crates"
    );
    let dir = rtc009_crates(workspace, &crates[..1], &lock);
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)],
        "one parsed crate"
    );
}

/// Packages with an RTC001 finding in a scan of `roots` with
/// `exclude_testnet_runtimes = true` and the include and exclude globs.
fn rtc001_testnet_scan(
    base: &std::path::Path,
    roots: &[&str],
    include: &[&str],
    exclude: &[&str],
) -> Vec<String> {
    let mut cfg = polkadot_linter::config::Config::default();
    cfg.runtime_config.exclude_testnet_runtimes = Some(true);
    cfg.general
        .exclude
        .extend(exclude.iter().map(ToString::to_string));
    let mut engine = polkadot_linter::engine::LintEngine::new(cfg);
    engine.filter_rules(&["runtime-config".to_string()]);
    if !include.is_empty() {
        let include = include.iter().map(ToString::to_string).collect::<Vec<_>>();
        engine.set_include_patterns(&include).unwrap();
    }
    let roots = roots.iter().map(|root| base.join(root)).collect::<Vec<_>>();
    let mut packages = engine
        .scan_paths(&roots, false, &[])
        .iter()
        .filter(|d| d.rule_id == "RTC001")
        .filter_map(|d| {
            let rel = d.file.strip_prefix(base).ok()?;
            let crate_dir = rel
                .ancestors()
                .find(|dir| base.join(dir).join("Cargo.toml").is_file())?;
            Some(crate_dir.file_name()?.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    packages.sort();
    packages.dedup();
    packages
}

#[test]
fn testnet_fallback_counts_only_the_runtimes_the_scan_lints() {
    let dir = rtc_runtime_crates(&["paseo-runtime", "polkadot-runtime", "westend-runtime"]);
    let base = dir.path();
    // A production runtime under `tests/` is test code, which the scan does
    // not lint.
    let fixture = base.join("tests/fixtures/kusama-runtime");
    std::fs::create_dir_all(fixture.join("src")).unwrap();
    std::fs::write(
        fixture.join("Cargo.toml"),
        "[package]\nname = \"kusama-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(fixture.join("src/lib.rs"), RTC_RUNTIME_LIB).unwrap();
    std::fs::remove_dir_all(base.join("polkadot-runtime")).unwrap();
    assert_eq!(
        rtc001_testnet_scan(base, &["."], &[], &[]),
        vec!["paseo-runtime", "westend-runtime"],
        "a runtime under tests/"
    );

    let dir = rtc_runtime_crates(&["paseo-runtime", "polkadot-runtime", "westend-runtime"]);
    let base = dir.path();
    assert_eq!(
        rtc001_testnet_scan(base, &["."], &[], &[]),
        vec!["polkadot-runtime"],
        "control: a production runtime next to the testnets"
    );
    assert_eq!(
        rtc001_testnet_scan(base, &["."], &[], &["polkadot-runtime/**"]),
        vec!["paseo-runtime", "westend-runtime"],
        "[general] exclude"
    );
    assert_eq!(
        rtc001_testnet_scan(base, &["."], &["westend-runtime/**"], &[]),
        vec!["westend-runtime"],
        "--include"
    );
    // One decision for every root: the production runtime in one root
    // skips the testnets in the other, as a scan of their parent does.
    assert_eq!(
        rtc001_testnet_scan(base, &["polkadot-runtime", "paseo-runtime"], &[], &[]),
        vec!["polkadot-runtime"],
        "two roots"
    );
    assert_eq!(
        rtc001_testnet_scan(base, &["paseo-runtime", "polkadot-runtime"], &[], &[]),
        vec!["polkadot-runtime"],
        "two roots, testnet first"
    );
    assert_eq!(
        rtc001_testnet_scan(base, &["paseo-runtime", "westend-runtime"], &[], &[]),
        vec!["paseo-runtime", "westend-runtime"],
        "two testnet roots"
    );
}

#[test]
fn testnet_fallback_applies_to_a_lockfile_scan_target() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let lock = format!(
        "version = 4\n{}{}{RTC009_EXECUTOR}",
        rtc009_package("paseo-runtime", "0.1.0", None, &["staging-xcm-executor"]),
        rtc009_package(
            "other-runtime",
            "1.0.0",
            Some("registry+https://github.com/rust-lang/crates.io-index"),
            &[]
        ),
    );
    let dir = rtc009_workspace("paseo-runtime", RTC009_RUNTIME_LIB, &lock);
    let mut cfg = polkadot_linter::config::Config::default();
    cfg.runtime_config.exclude_testnet_runtimes = Some(true);
    let mut engine = polkadot_linter::engine::LintEngine::new(cfg);
    engine.filter_rules(&["RTC009".to_string()]);
    for root in [dir.path().to_path_buf(), dir.path().join("Cargo.lock")] {
        let levels = engine
            .scan(&root)
            .iter()
            .map(|d| (d.severity, d.confidence))
            .collect::<Vec<_>>();
        assert_eq!(
            levels,
            vec![(Severity::Error, Confidence::High)],
            "{}",
            root.display()
        );
    }
}

#[test]
fn testnet_fallback_skips_other_workspaces_in_a_relative_lockfile_scan() {
    use polkadot_linter::diagnostics::Severity;
    let registry = Some("registry+https://github.com/rust-lang/crates.io-index");
    // The executor is unreachable from `paseo-runtime`, so it is advisory
    // when the testnet is linted and an error when it is skipped.
    let lock = format!(
        "version = 4\n{}{}{RTC009_EXECUTOR}",
        rtc009_package("paseo-runtime", "0.1.0", None, &["sp-io"]),
        rtc009_package("sp-io", "1.0.0", registry, &[]),
    );
    let dir = rtc009_workspace("paseo-runtime", RTC009_RUNTIME_LIB, &lock);
    // A production runtime of another workspace has its own lockfile.
    let other = dir.path().join("other");
    std::fs::create_dir_all(other.join("polkadot-runtime/src")).unwrap();
    std::fs::write(
        other.join("Cargo.toml"),
        "[workspace]\nmembers = [\"polkadot-runtime\"]\n",
    )
    .unwrap();
    std::fs::write(
        other.join("polkadot-runtime/Cargo.toml"),
        "[package]\nname = \"polkadot-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        other.join("polkadot-runtime/src/lib.rs"),
        RTC009_RUNTIME_LIB,
    )
    .unwrap();
    let config = dir.path().join("linter.toml");
    std::fs::write(
        &config,
        "[runtime_config]\nexclude_testnet_runtimes = true\n",
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_polkadot-linter"))
        .current_dir(dir.path())
        .args(["--syntax-only", "--format", "json", "--rules", "RTC009"])
        .arg("--config")
        .arg(&config)
        .arg("Cargo.lock")
        .output()
        .expect("linter should run");
    let diags =
        serde_json::from_slice::<Vec<polkadot_linter::diagnostics::Diagnostic>>(&output.stdout)
            .expect("JSON output");
    assert_eq!(
        diags.iter().map(|d| d.severity).collect::<Vec<_>>(),
        vec![Severity::Advisory],
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn rtc009_reads_frame_support_runtime_declarations() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let registry = Some("registry+https://github.com/rust-lang/crates.io-index");
    // The executor is unreachable from `nexus`, so it is advisory only when
    // the parser reads `nexus` as a runtime.
    let lock = format!(
        "version = 4\n{}{}{RTC009_EXECUTOR}",
        rtc009_package("nexus", "0.1.0", None, &["sp-io"]),
        rtc009_package("sp-io", "1.0.0", registry, &[]),
    );
    let body = "\nmod runtime {\n    #[runtime::pallet_index(0)]\n    \
                pub type System = frame_system;\n}\n";
    for attr in [
        "#[frame_support::runtime]",
        "#[frame_support::runtime(legacy_ordering)]",
        "#[frame_support::runtime (legacy_ordering)]",
    ] {
        let dir = rtc009_workspace("nexus", &format!("{attr}{body}"), &lock);
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Advisory, Confidence::Low)],
            "{attr}"
        );
    }
}

#[test]
fn rtc009_counts_only_crates_of_this_workspace_and_version_as_parsed() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let manifest = |name: &str, version: &str, extra: &str| {
        format!(
            "[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n{extra}"
        )
    };
    // A library the parser reads: its only runtime is test code.
    let helper_lib = "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    \
                      construct_runtime!(pub enum Test { System: frame_system = 0 });\n}\n";
    let lock = |helper_version: &str| {
        format!(
            "version = 4\n{}{}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}",
            rtc009_package("prod-runtime", "0.1.0", None, &["substrate-wasm-builder"]),
            rtc009_package("helper", helper_version, None, &["staging-xcm-executor"]),
        )
    };
    let levels = |helper_manifest: &str, helper_version: &str| {
        let dir = rtc009_crates(
            "[workspace]\nmembers = [\"prod-runtime\", \"helper\"]\n",
            &[
                (
                    "prod-runtime",
                    &manifest("prod-runtime", "0.1.0", ""),
                    RTC009_RUNTIME_LIB,
                ),
                ("helper", helper_manifest, helper_lib),
            ],
            &lock(helper_version),
        );
        rtc009_levels(dir.path())
    };
    assert_eq!(
        levels(&manifest("helper", "0.1.0", ""), "0.1.0"),
        vec![(Severity::Advisory, Confidence::Low)],
        "control: the parsed library is not a root"
    );
    // The lockfile entry is other code, which the parser did not read.
    assert_eq!(
        levels(&manifest("helper", "0.1.0", ""), "0.2.0"),
        vec![(Severity::Error, Confidence::High)],
        "another version"
    );
    assert_eq!(
        levels(&manifest("helper", "0.1.0", "[workspace]\n"), "0.1.0"),
        vec![(Severity::Error, Confidence::High)],
        "a crate of another workspace"
    );
}

/// A workspace whose `nexus-runtime` has the executor `lib`, with
/// `staging-xcm-executor` 28.0.1 reachable only from it.
fn rtc009_executor_workspace(lib: &str) -> tempfile::TempDir {
    let lock = format!(
        "version = 4\n{}{RTC009_EXECUTOR}",
        rtc009_package("nexus-runtime", "0.1.0", None, &["staging-xcm-executor"])
    );
    rtc009_workspace("nexus-runtime", lib, &lock)
}

#[test]
fn rtc009_applies_the_unit_transactor_mitigation_only_to_executors_pallet_xcm_names() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let construct = "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n";
    let pallet_xcm = |executor: &str| {
        format!(
            "impl pallet_xcm::Config for Runtime {{\n    \
             type XcmExecutor = xcm_executor::XcmExecutor<{executor}>;\n}}\n"
        )
    };
    let advisory = vec![(Severity::Advisory, Confidence::Low)];
    let error = vec![(Severity::Error, Confidence::High)];
    for (case, items, expected) in [
        (
            "trait imported with `use`",
            format!(
                "use xcm_executor::Config;\nimpl Config for NexusXcm {{\n    \
                 type AssetTransactor = ();\n}}\n{}",
                pallet_xcm("NexusXcm")
            ),
            &advisory,
        ),
        (
            "trait imported under another name",
            format!(
                "use staging_xcm_executor::{{Config as ExecutorConfig}};\n\
                 impl ExecutorConfig for NexusXcm {{\n    type AssetTransactor = ();\n}}\n{}",
                pallet_xcm("NexusXcm")
            ),
            &advisory,
        ),
        (
            "a second executor under a renamed trait moves assets",
            format!(
                "use staging_xcm_executor::Config as ExecutorConfig;\n\
                 impl ExecutorConfig for NexusXcm {{\n    type AssetTransactor = ();\n}}\n\
                 impl ExecutorConfig for OtherXcm {{\n    type AssetTransactor = FungibleAdapter;\n}}\n{}",
                pallet_xcm("NexusXcm")
            ),
            &error,
        ),
        (
            // A glob import hides the trait path, so only the
            // `AssetTransactor` item shows it may be an executor.
            "an impl through a glob import moves assets",
            format!(
                "impl xcm_executor::Config for NexusXcm {{\n    type AssetTransactor = ();\n}}\n\
                 mod other {{\n    use xcm_executor::*;\n    impl Config for OtherXcm {{\n        \
                 type AssetTransactor = FungibleAdapter;\n    }}\n}}\n{}",
                pallet_xcm("NexusXcm")
            ),
            &error,
        ),
        (
            "an impl of another trait for `XcmConfig`",
            format!(
                "impl pallet_custom::Config for XcmConfig {{\n    type AssetTransactor = ();\n}}\n{}",
                pallet_xcm("XcmConfig")
            ),
            &error,
        ),
        (
            "pallet_xcm names an executor the parser cannot see",
            format!(
                "impl xcm_executor::Config for NexusXcm {{\n    type AssetTransactor = ();\n}}\n\
                 include!(\"executor.rs\");\n{}",
                pallet_xcm("HiddenXcm")
            ),
            &error,
        ),
        (
            "another XcmExecutor names an executor the parser cannot see",
            format!(
                "impl xcm_executor::Config for NexusXcm {{\n    type AssetTransactor = ();\n}}\n{}\
                 impl pallet_message_queue::Config for Runtime {{\n    \
                 type MessageProcessor = ProcessXcmMessage<AggregateMessageOrigin, \
                 XcmExecutor<HiddenXcm>, RuntimeCall>;\n}}\n",
                pallet_xcm("NexusXcm")
            ),
            &error,
        ),
        (
            "executor imported from its module",
            format!(
                "mod xcm_config {{\n    impl xcm_executor::Config for NexusXcm {{\n        \
                 type AssetTransactor = ();\n    }}\n}}\nuse xcm_config::NexusXcm;\n{}",
                pallet_xcm("NexusXcm")
            ),
            &advisory,
        ),
        (
            "XcmExecutor imported under another name",
            "use xcm_executor::XcmExecutor as XcmExec;\n\
             impl xcm_executor::Config for NexusXcm {\n    type AssetTransactor = ();\n}\n\
             impl pallet_xcm::Config for Runtime {\n    type XcmExecutor = XcmExec<NexusXcm>;\n}\n"
                .to_string(),
            &advisory,
        ),
        (
            "pallet_xcm names an executor of the same name in another module",
            format!(
                "mod inert {{\n    impl xcm_executor::Config for XcmConfig {{\n        \
                 type AssetTransactor = ();\n    }}\n}}\n\
                 mod active {{\n    include!(\"active.rs\");\n}}\n{}",
                pallet_xcm("active::XcmConfig")
            ),
            &error,
        ),
        (
            "pallet_xcm names an executor of an extern crate",
            format!(
                "impl xcm_executor::Config for XcmConfig {{\n    type AssetTransactor = ();\n}}\n{}",
                pallet_xcm("common_xcm::XcmConfig")
            ),
            &error,
        ),
        (
            "a renamed XcmExecutor names an executor the parser cannot see",
            format!(
                "impl xcm_executor::Config for NexusXcm {{\n    type AssetTransactor = ();\n}}\n{}\
                 use xcm_executor::XcmExecutor as Executor;\n\
                 type MessageProcessor = ProcessXcmMessage<Origin, Executor<HiddenXcm>, RuntimeCall>;\n",
                pallet_xcm("NexusXcm")
            ),
            &error,
        ),
        (
            "a turbofish XcmExecutor in an expression",
            format!(
                "impl xcm_executor::Config for NexusXcm {{\n    type AssetTransactor = ();\n}}\n{}\
                 fn execute() {{\n    XcmExecutor::<HiddenXcm>::prepare_and_execute();\n}}\n",
                pallet_xcm("NexusXcm")
            ),
            &error,
        ),
        (
            // A name with several imports resolves only when each one does.
            "the trait name has an import of another Config",
            format!(
                "#[cfg(feature = \"a\")]\nuse xcm_executor::Config;\n\
                 #[cfg(not(feature = \"a\"))]\nuse pallet_custom::Config;\n\
                 impl Config for NexusXcm {{\n    type AssetTransactor = ();\n}}\n{}",
                pallet_xcm("NexusXcm")
            ),
            &error,
        ),
        (
            "an executor impl without an AssetTransactor",
            format!(
                "impl xcm_executor::Config for NexusXcm {{\n    type Barrier = ();\n}}\n{}",
                pallet_xcm("NexusXcm")
            ),
            &error,
        ),
        (
            "no pallet_xcm",
            "impl xcm_executor::Config for NexusXcm {\n    type AssetTransactor = ();\n}\n"
                .to_string(),
            &error,
        ),
        (
            "pallet_xcm without an XcmExecutor",
            "impl xcm_executor::Config for NexusXcm {\n    type AssetTransactor = ();\n}\n\
             impl pallet_xcm::Config for Runtime {}\n"
                .to_string(),
            &error,
        ),
    ] {
        let dir = rtc009_executor_workspace(&format!("{construct}{items}"));
        assert_eq!(&rtc009_levels(dir.path()), expected, "{case}");
    }
}

#[test]
fn rtc009_treats_cfg_attr_and_raw_identifiers_like_cfg_and_plain_names() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let lib = |items: &str| {
        format!(
            "construct_runtime!(pub enum Runtime {{ System: frame_system = 0 }});\n{items}\
             impl pallet_xcm::Config for Runtime {{\n    type XcmExecutor = XcmExecutor<NexusXcm>;\n}}\n"
        )
    };
    let executor = "impl staging_xcm_executor::Config for NexusXcm {\n    \
                    type AssetTransactor = Transactor;\n}\n";
    let dir = rtc009_executor_workspace(&lib(&format!("type Transactor = ();\n{executor}")));
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)],
        "control"
    );
    let raw_use = executor.replace("= Transactor;", "= r#Transactor;");
    std::fs::write(
        dir.path().join("nexus-runtime/src/lib.rs"),
        lib(&format!("type Transactor = ();\n{raw_use}")),
    )
    .unwrap();
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)],
        "`r#Transactor` names the `Transactor` alias"
    );
    std::fs::write(
        dir.path().join("nexus-runtime/src/lib.rs"),
        lib(&format!("type Transactor = ();\n{executor}")),
    )
    .unwrap();
    for items in [
        // `cfg_attr` can remove the alias, like `cfg`.
        format!(
            "#[cfg_attr(feature = \"real\", cfg(any()))]\ntype Transactor = ();\n\
             #[cfg(feature = \"real\")]\ninclude!(\"real_transactor.rs\");\n{executor}"
        ),
        // `r#Transactor` is the same name as `Transactor`.
        format!("type Transactor = ();\nuse real::r#Transactor;\n{executor}"),
        "type Transactor = ();\nimpl<r#Transactor: TransactAsset> staging_xcm_executor::Config \
         for NexusXcm<Transactor> {\n    type AssetTransactor = Transactor;\n}\n"
            .to_string(),
    ] {
        std::fs::write(dir.path().join("nexus-runtime/src/lib.rs"), lib(&items)).unwrap();
        assert_eq!(
            rtc009_levels(dir.path()),
            vec![(Severity::Error, Confidence::High)],
            "{items}"
        );
    }
}

#[test]
fn rtc003_keeps_the_fee_manager_confidence_for_cfg_attr_and_qualified_senders() {
    use polkadot_linter::diagnostics::Confidence;
    let runtime = |pallets: &str, blocked: &str| {
        format!(
            "construct_runtime!(pub enum Runtime {{\n    System: frame_system = 0,\n    \
             PolkadotXcm: pallet_xcm = 31,\n{pallets}}});\n{blocked}type Blocked = Nothing;\n\
             impl pallet_xcm::Config for Runtime {{\n    \
             type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;\n    \
             type XcmExecuteFilter = Blocked;\n    type XcmTeleportFilter = Nothing;\n    \
             type XcmReserveTransferFilter = Nothing;\n}}\n\
             impl xcm_executor::Config for XcmConfig {{\n    type XcmSender = XcmRouter;\n    \
             type MessageExporter = Exporter;\n    type FeeManager = WaiveDeliveryFees;\n}}\n"
        )
    };
    // The line and confidence of every RTC003 finding.
    let fee_manager = |source: &str| {
        check_fixture("runtime/src/lib.rs", source)
            .iter()
            .filter(|d| d.rule_id == "RTC003")
            .map(|d| (d.line, d.confidence))
            .collect::<Vec<_>>()
    };
    // The RTC003 finding on the `FeeManager` line with `confidence`.
    let finding = |source: &str, confidence: Confidence| {
        let line = source
            .lines()
            .position(|line| line.contains("type FeeManager"))
            .unwrap()
            + 1;
        vec![(line, confidence)]
    };
    let control = runtime("", "");
    assert_eq!(
        fee_manager(&control),
        finding(&control, Confidence::Low),
        "control"
    );
    for (case, source) in [
        (
            "cfg_attr on the filter alias",
            runtime("", "#[cfg_attr(feature = \"open\", cfg(any()))]\n"),
        ),
        (
            "path-qualified XCM sender pallet",
            runtime(
                "    XcmTransactor: my_pallets::pallet_xcm_transactor = 32,\n",
                "",
            ),
        ),
    ] {
        assert_eq!(
            fee_manager(&source),
            finding(&source, Confidence::Medium),
            "{case}"
        );
    }
    // `#[frame_support::runtime]` with a `+ Call` pallet type.
    let frame_runtime = runtime("", "").replace(
        "construct_runtime!(pub enum Runtime {\n    System: frame_system = 0,\n    \
         PolkadotXcm: pallet_xcm = 31,\n});",
        "#[frame_support::runtime]\nmod runtime {\n    #[runtime::pallet_index(0)]\n    \
         pub type System = frame_system;\n    #[runtime::pallet_index(31)]\n    \
         pub type PolkadotXcm = pallet_xcm;\n    #[runtime::pallet_index(32)]\n    \
         pub type XTokens = orml_xtokens + Call;\n}",
    );
    assert!(frame_runtime.contains("orml_xtokens"), "{frame_runtime}");
    assert_eq!(
        fee_manager(&frame_runtime),
        finding(&frame_runtime, Confidence::Medium)
    );
}

#[test]
fn rtc008_takes_the_crate_of_a_frame_support_runtime_pallet_type_before_the_plus() {
    use polkadot_linter::diagnostics::Confidence::{High, Medium};
    let bad = include_str!("fixtures/bad_rtc008.rs");
    let filters = &bad[bad.find("impl InstanceFilter").unwrap()..];
    let filters = filters.replace(
        "RuntimeCall::CollatorSelection(..)",
        "RuntimeCall::Timestamp(..)",
    );
    let runtime = |timestamp: &str| {
        format!(
            "#[frame_support::runtime]\nmod runtime {{\n    #[runtime::pallet_index(0)]\n    \
             pub type System = frame_system;\n    #[runtime::pallet_index(10)]\n    \
             pub type Balances = pallet_balances;\n{timestamp}    #[runtime::pallet_index(31)]\n    \
             pub type PolkadotXcm = pallet_xcm;\n    #[runtime::pallet_index(40)]\n    \
             pub type Utility = pallet_utility;\n    #[runtime::pallet_index(50)]\n    \
             pub type Assets = pallet_assets::Pallet<Runtime, Instance1>;\n}}\n{filters}"
        )
    };
    let confidences = |timestamp: &str| {
        check_fixture("runtime/src/lib.rs", &runtime(timestamp))
            .iter()
            .filter(|d| d.rule_id == "RTC008")
            .map(|d| d.confidence)
            .collect::<Vec<_>>()
    };
    let index = "    #[runtime::pallet_index(21)]\n";
    assert_eq!(
        confidences(&format!(
            "{index}    pub type Timestamp = pallet_timestamp + Call;\n"
        )),
        vec![High, Medium]
    );
    // One cfg alternative without a visible crate may be any pallet.
    assert_eq!(
        confidences(&format!(
            "    #[cfg(feature = \"a\")]\n{index}    pub type Timestamp = pallet_timestamp;\n    \
             #[cfg(not(feature = \"a\"))]\n{index}    \
             pub type Timestamp = <Pallets as Select>::Timestamp;\n"
        )),
        vec![High, High]
    );
}

#[test]
fn rtc009_lints_a_standalone_runtime_that_a_parent_workspace_excludes() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let dir = rtc009_crates(
        "[workspace]\nmembers = [\"other\"]\nexclude = [\"my-chain\"]\n",
        &[
            (
                "other",
                "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                "",
            ),
            (
                "my-chain",
                "[package]\nname = \"my-chain\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                RTC009_RUNTIME_LIB,
            ),
        ],
        "",
    );
    std::fs::remove_file(dir.path().join("Cargo.lock")).unwrap();
    std::fs::write(
        dir.path().join("my-chain/Cargo.lock"),
        format!(
            "version = 4\n{}{}{RTC009_EXECUTOR}",
            rtc009_package("my-chain", "0.1.0", None, &["staging-xcm-executor"]),
            rtc009_package(
                "other-runtime",
                "1.0.0",
                Some("registry+https://github.com/rust-lang/crates.io-index"),
                &[]
            ),
        ),
    )
    .unwrap();
    assert_eq!(
        rtc009_levels(&dir.path().join("my-chain")),
        vec![(Severity::Error, Confidence::High)]
    );
}

#[test]
fn rtc009_matches_runtimes_by_workspace_version_and_name_filters() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let registry = Some("registry+https://github.com/rust-lang/crates.io-index");
    let manifest = |name: &str, version: &str| {
        format!("[package]\nname = \"{name}\"\n{version}\nedition = \"2021\"\n")
    };

    // A runtime of a nested workspace has its own lockfile, so it does not
    // describe the outer lock's source-less entry of the same name and version.
    let prod = rtc009_package("prod-runtime", "0.1.0", None, &["substrate-wasm-builder"]);
    let outside = rtc009_package(
        "nexus-runtime",
        "0.1.0",
        None,
        &["staging-xcm-executor", "substrate-wasm-builder"],
    );
    let nexus = manifest("nexus-runtime", "version = \"0.1.0\"");
    let prod_manifest = manifest("prod-runtime", "version = \"0.1.0\"");
    let dir = rtc009_crates(
        "[workspace]\nmembers = [\"prod-runtime\"]\nexclude = [\"inner\"]\n",
        &[
            ("prod-runtime", prod_manifest.as_str(), RTC009_RUNTIME_LIB),
            (
                "inner/nexus-runtime",
                nexus.as_str(),
                RTC009_UNIT_TRANSACTOR_LIB,
            ),
        ],
        &format!("version = 4\n{prod}{outside}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}"),
    );
    std::fs::write(
        dir.path().join("inner/Cargo.toml"),
        "[workspace]\nmembers = [\"nexus-runtime\"]\n",
    )
    .unwrap();
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Error, Confidence::High)],
        "runtime of another workspace"
    );

    // An unresolved version does not pick the first of several entries, so
    // the unit transactor does not describe the entry that reaches the
    // executor.
    let dir = rtc009_crates(
        "[workspace]\nmembers = [\"nexus\"]\n",
        &[(
            "nexus",
            manifest("nexus", "version.workspace = true").as_str(),
            RTC009_UNIT_TRANSACTOR_LIB,
        )],
        &format!(
            "version = 4\n{}{}{RTC009_EXECUTOR}",
            rtc009_package("nexus", "0.1.0", None, &["staging-xcm-executor"]),
            rtc009_package("nexus", "0.2.0", None, &[]),
        ),
    );
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Error, Confidence::High)],
        "unresolved version"
    );

    // The test, dev and testnet name filters apply to external roots.
    let lock = |name: &str| {
        format!(
            "version = 4\n{prod}{}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}",
            rtc009_package(
                name,
                "1.0.0",
                registry,
                &["staging-xcm-executor", "substrate-wasm-builder"]
            )
        )
    };
    let dir = rtc009_workspace(
        "prod-runtime",
        RTC009_RUNTIME_LIB,
        &lock("polkadot-test-runtime"),
    );
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)],
        "test runtime"
    );
    std::fs::write(dir.path().join("Cargo.lock"), lock("westend-runtime")).unwrap();
    let mut cfg = polkadot_linter::config::Config::default();
    cfg.runtime_config.exclude_testnet_runtimes = Some(true);
    let levels = rtc_scan(dir.path(), cfg)
        .iter()
        .filter(|d| d.rule_id == "RTC009")
        .map(|d| (d.severity, d.confidence))
        .collect::<Vec<_>>();
    assert_eq!(
        levels,
        vec![(Severity::Advisory, Confidence::Low)],
        "testnet runtime"
    );
}

#[test]
fn rtc009_treats_a_source_less_package_the_parser_did_not_read_as_a_root() {
    use polkadot_linter::diagnostics::{Confidence, Severity};
    let manifest = |name: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
    };
    let lock = format!(
        "version = 4\n{}{}{RTC009_EXECUTOR}{RTC009_WASM_BUILDER}",
        rtc009_package("prod-runtime", "0.1.0", None, &["substrate-wasm-builder"]),
        rtc009_package("real-chain", "0.1.0", None, &["staging-xcm-executor"]),
    );
    // The crate walk skips hidden directories, so it never reads `real-chain`.
    let dir = rtc009_crates(
        "[workspace]\nmembers = [\"prod-runtime\", \".hidden/real\"]\n",
        &[
            (
                "prod-runtime",
                &manifest("prod-runtime"),
                RTC009_RUNTIME_LIB,
            ),
            (".hidden/real", &manifest("real-chain"), RTC009_RUNTIME_LIB),
        ],
        &lock,
    );
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Error, Confidence::High)]
    );
    // Control: the same member where the walk reads it, as a library.
    let dir = rtc009_crates(
        "[workspace]\nmembers = [\"prod-runtime\", \"real\"]\n",
        &[
            (
                "prod-runtime",
                &manifest("prod-runtime"),
                RTC009_RUNTIME_LIB,
            ),
            ("real", &manifest("real-chain"), "pub fn f() {}\n"),
        ],
        &lock,
    );
    assert_eq!(
        rtc009_levels(dir.path()),
        vec![(Severity::Advisory, Confidence::Low)]
    );
}
