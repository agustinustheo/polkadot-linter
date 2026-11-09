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

#[test]
fn sem014_handles_wrapped_unsigned_submission_and_long_log_target() {
    let code = r#"
fn submit() {
    let transaction = SubmitTransaction::<T>::new(payload);
    transaction
        .submit_unsigned_transaction()
        .expect("submission succeeds");
    log::warn!(
        target: LOG_TARGET,
        "unable to submit transaction with a deliberately long and detailed diagnostic message",
    );
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEM014"),
        "SEM014 should associate rustfmt-wrapped unsigned submission with its full log macro: {diags:?}"
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
    type WeightInfo = ();
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
fn rtc001_reports_substrate_weight_as_advisory_when_enabled() {
    let mut config = polkadot_linter::config::Config::default();
    config.runtime_config.flag_substrate_weight = true;
    let diags = check_fixture_with_config(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc001.rs"),
        &config,
    );
    let findings = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "RTC001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 1, "{diags:?}");
    assert_eq!(findings[0].line, 19);
    assert_eq!(
        findings[0].severity,
        polkadot_linter::diagnostics::Severity::Advisory
    );
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
fn rtc002_detects_local_weights_missing_from_define_benchmarks() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc002.rs"));
    assert_eq!(rtc_lines(&diags, "RTC002"), vec![10, 14, 20], "{diags:?}");
}

#[test]
fn rtc002_accepts_benchmarked_pallets() {
    let diags = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc002.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc002_reads_define_benchmarks_from_sibling_files_and_requires_it() {
    let configs = r#"
impl pallet_balances::Config for Runtime {
    type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
}
impl pallet_sudo::Config for Runtime {
    type WeightInfo = weights::pallet_sudo::WeightInfo<Runtime>;
}
"#;
    let lib = "mod configs;\nmod benchmarks;\nconstruct_runtime!(pub enum Runtime { Balances: pallet_balances = 10 });\n";
    let benches = "frame_benchmarking::define_benchmarks!(\n    [pallet_balances, Balances]\n);\n";
    let krate = rtc_runtime_crate(
        "example-runtime",
        &[
            ("src/lib.rs", lib),
            ("src/configs/mod.rs", configs),
            ("src/benchmarks.rs", benches),
        ],
    );
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC002:6".to_string()],
        "{diags:?}"
    );

    // Without any define_benchmarks! the precondition is not met.
    let krate = rtc_runtime_crate(
        "example-runtime",
        &[("src/lib.rs", lib), ("src/configs/mod.rs", configs)],
    );
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
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
        .all(|diagnostic| diagnostic.severity == polkadot_linter::diagnostics::Severity::Error));
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
    assert_eq!(rtc_lines(&diags, "RTC005"), vec![10, 16, 17], "{diags:?}");
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
        "impl pallet_sudo::Config for Runtime { type WeightInfo = (); }",
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
fn rtc003_stays_error_with_a_minimal_user_config() {
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
            .all(|(_, severity)| *severity == polkadot_linter::diagnostics::Severity::Error),
        "{severities:?}"
    );
}

#[test]
fn rtc_rules_recognise_a_bare_runtime_attribute() {
    let lib = r#"
use frame_support::runtime;

impl pallet_sudo::Config for Runtime {
    type WeightInfo = ();
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
    let lib = "#[runtime]\nmod runtime { pub struct Runtime; }\nimpl pallet_sudo::Config for Runtime {\n    type WeightInfo = ();\n}\n";
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
fn rtc002_ignores_define_benchmarks_in_test_modules() {
    let lib = r#"
impl pallet_sudo::Config for Runtime {
    type WeightInfo = weights::pallet_sudo::WeightInfo<Runtime>;
}

construct_runtime!(pub enum Runtime { Sudo: pallet_sudo = 15 });

#[cfg(feature = "runtime-benchmarks")]
mod benches {
    frame_benchmarking::define_benchmarks!([pallet_balances, Balances]);
}

#[cfg(test)]
mod tests {
    frame_benchmarking::define_benchmarks!([pallet_sudo, Sudo]);
}
"#;
    let krate = rtc_runtime_crate("bench-runtime", &[("src/lib.rs", lib)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC002:3".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc_rules_skip_files_behind_test_or_benchmark_module_declarations() {
    let placeholder = "impl pallet_sudo::Config for Runtime {\n    type WeightInfo = ();\n}\n";
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
    let config = "impl pallet_sudo::Config for Runtime {\n    type WeightInfo = ();\n}\n";
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
fn rtc002_matches_benchmark_paths_by_whole_segments() {
    let lib = "construct_runtime!(pub enum Runtime { Balances: pallet_balances = 10 });\n\
impl custom_pallet_balances::Config for Runtime {\n    type WeightInfo = weights::custom_pallet_balances::WeightInfo<Runtime>;\n}\n\
impl parachains_paras::Config for Runtime {\n    type WeightInfo = weights::polkadot_runtime_parachains_paras::WeightInfo<Runtime>;\n}\n\
impl disputes::Config for Runtime {\n    type WeightInfo = weights::disputes::WeightInfo<Runtime>;\n}\n\
define_benchmarks!(\n    [pallet_balances, Balances]\n    [polkadot_runtime_parachains::paras, Paras]\n    [polkadot_runtime_parachains::disputes, Disputes]\n);\n";
    let krate = rtc_runtime_crate("segment-runtime", &[("src/lib.rs", lib)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(rtc_lines(&diags, "RTC002"), vec![3], "{diags:?}");
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

// ==========================================================================
// Noise narrowing (round 2): one negative case per mechanism, plus recall guards
// ==========================================================================

/// Writes `files` (relative path, content) under a fresh temp dir.
fn write_tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    dir
}

#[test]
fn rtc001_skips_parity_test_networks_only() {
    for (package, fires) in [
        ("rococo-runtime", false),
        ("glutton-westend-runtime", false),
        ("next-people-paseo-runtime", false),
        ("westend-runtime", true),
        ("parachain-template-runtime", true),
    ] {
        let krate = rtc_runtime_crate(package, &[("src/lib.rs", RTC_RUNTIME_LIB)]);
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert_eq!(
            rtc_findings(&diags) == vec!["RTC001:3".to_string()],
            fires,
            "{package}: {diags:?}"
        );
    }
}

#[test]
fn rtc001_skips_pallets_without_calls() {
    let runtime = r#"
impl pallet_hooks::Config for Runtime {
    type WeightInfo = ();
    type ExtensionsWeightInfo = pallet_hooks::TestWeightInfo;
}
impl pallet_calls::Config for Runtime {
    type WeightInfo = ();
}

construct_runtime!(
    pub enum Runtime {
        Hooks: pallet_hooks = 1,
        Calls: pallet_calls = 2,
    }
);
"#;
    let dir = write_tree(&[
        (
            "runtime/Cargo.toml",
            "[package]\nname = \"chain-runtime\"\nversion = \"0.1.0\"\n\n[dependencies]\npallet-hooks = { path = \"../hooks\" }\npallet-calls = { path = \"../calls\" }\n",
        ),
        ("runtime/src/lib.rs", runtime),
        ("hooks/src/lib.rs", "#[pallet::hooks]\nimpl<T: Config> Hooks for Pallet<T> {}\n"),
        ("calls/src/lib.rs", "#[pallet::call]\nimpl<T: Config> Pallet<T> {}\n"),
    ]);
    let diags = rtc_scan(
        &dir.path().join("runtime"),
        polkadot_linter::config::Config::default(),
    );
    // The no-calls skip covers `()` only, not `TestWeightInfo`.
    let mut findings = rtc_findings(&diags);
    findings.sort();
    assert_eq!(findings, vec!["RTC001:4", "RTC001:7"], "{diags:?}");
}
