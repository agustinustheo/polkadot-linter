# Review Remediation

This page tracks the remediation work begun from the 2026-07-14 full-project
review. A rule is not considered audit-grade merely because its fixture passes:
every listed fix has a regression test that reproduces the reviewed shape.

## Resolved In This Tranche

| Review finding | Resolution | Evidence |
| --- | --- | --- |
| VAL001 stops at multiline signatures | Keep scanning until the function body has opened and closed; support restricted visibility functions. | `val001_detects_multiline_dispatchable_signatures` |
| `#[cfg(test)]` masks the rest of a file | Recognize additional block item forms and do not leave a pending mask after an inline attribute/item. | `sec008_does_not_mask_production_after_cfg_test_const_fn` |
| Negated test cfg conditions mask production code | Parse cfg predicates and mask only conditions whose every enabled branch is non-production; `cfg(not(any(test, ...)))` remains linted. | `sec008_does_not_mask_production_after_negated_test_cfg`, `sec008_masks_test_cfg_when_test_is_conjoined_with_other_conditions` |
| Implicit FRAME call indices are omitted | Treat every method in `#[pallet::call]` as a dispatchable; `call_index` is optional metadata. | `sec001_checks_dispatchables_without_explicit_call_indices` |
| BEN003 omits implicit-index FRAME dispatchables | Apply the same call-impl membership model to the benchmark coverage rule. | `ben003_detects_implicit_index_dispatchables` |
| Impl-level FRAME weight providers are ignored | Recognize qualified `#[pallet::call(weight = ...)]` declarations and model their generated per-dispatchable accounting. | `sec001_recognizes_impl_level_call_weight_providers` |
| Compiler scans silently return zero on a warm target | Add a unique Cargo fingerprint per scan and require the driver to create an invocation marker. | Repeated warm-target compiler pipeline run |
| Missing driver discards syntax findings | Retain and emit syntax results, with a clear compiler-analysis-skipped message. `--no-syntax` still fails because it requested compiler-only output. | CLI behavior in `main.rs` |
| TRM001 panics after non-ASCII source | Return byte offsets from line-comment detection before slicing the UTF-8 source line. | `trm001_handles_non_ascii_before_inline_comments` |
| SEC004/SEC005 miss normal `pub mod pallet` nesting | Recursively collect weight attributes from item and impl functions. | `sec004_checks_weight_attributes_nested_in_pallet_modules` |
| SEC015 treats `if !is_root` as root-only | Only grant root context for a positive root proof, including conjunctive conditions. | `sec015_reports_bypass_inside_negated_root_flag_branch` |
| SEC012 reads the cursor as a `StorageMap` deletion limit | Select the limit by argument count: first for `StorageMap`, second for `StorageDoubleMap`. | `sec012_allows_bounded_single_key_clear_prefix` |
| BEN002 mishandles v1 benchmark macros and parameterized attributes | Track macro boundaries, include trailing `verify` blocks, and accept `#[benchmark(...)]`. | `ben002_handles_v1_benchmarks_with_individual_verify_blocks`, `ben002_recognizes_parameterized_benchmark_attributes` |
| SEC009 skips arithmetic in `if` conditions | Visit the condition before applying branch-specific subtraction proofs. | `sec009_detects_raw_arithmetic_in_if_conditions` |
| SEC014 misses FRAME numeric type aliases | Recognize `BalanceOf` and `BlockNumberFor` in identity-key analysis. | `sec014_detects_identity_hasher_on_frame_type_alias_keys` |
| SEC013 trusts “no maximum capacity” docs | Treat explicit no-maximum wording as evidence that a storage collection remains unbounded. | Source-rule regression suite |
| Driver JSONL records can interleave | Serialize diagnostics and their newlines into one buffer and append them with one `write_all` call. | Driver build under `rustc-driver` |
| `config/default.toml` diverges from runtime defaults | Deserialize the bundled TOML as the `Config` default, so the documented file is the effective default source. | `bundled_default_configuration_matches_config_default_toml` |
| Explicit missing config silently falls back | Treat omitted config and explicit `--config` separately; an explicit path must load successfully. | `explicit_missing_config_is_an_error` |
| Invalid include/exclude globs are ignored | Validate configured patterns and make CLI pattern compilation fallible before scanning. | `validation_rejects_invalid_globs_and_severities` |
| Family severity settings are dead | Apply family severity to VAL, TST, MOK, BEN, and TRM rules; per-rule overrides remain higher precedence. | `per_rule_severity_overrides_family_severity` |
| TST001 emits duplicate diagnostics | Remove the unreachable text pass and inspect assertion macro tokens for macro-contained `unwrap_err` calls. | `tst001_reports_one_diagnostic_for_one_manual_error_check`, `tst001_detects_unwrap_err_inside_assert_macro` |
| BEN001 reports trait and impl declarations repeatedly | Deduplicate weight functions by name before matching benchmarks; end the text fallback when the WeightInfo block closes. | BEN001 rule suite |
| BEN003 fallback only allows four lines for an extrinsic signature | Allow a bounded 32-line attribute/signature window. | BEN003 rule suite |
| BEN003 ignores qualified FRAME pallet attributes | Match the `pallet::call` and `pallet::call_index` suffixes, preserving fully qualified FRAME forms. | `ben003_detects_qualified_pallet_call_attributes` |
| Semantic FRAME-attribute rules ignore qualified paths | Match multi-segment FRAME attribute suffixes while retaining exact matching for one-segment language attributes. | `sem011_detects_weight_zero_in_qualified_frame_attribute` |
| Compiler-backed weight rules cannot distinguish active from inactive `cfg_attr` attributes | Expand attributes with rustc's active Cargo configuration before parsing the authored attribute source, so only the selected configuration contributes weight evidence. | `check-rustc-weight-attributes.sh` |
| Source rules miss feature-controlled `cfg_attr` FRAME attributes | Resolve `cfg_attr` using the same default, disabled-default, and explicit Cargo feature selection as the CLI/compiler pipeline; keep custom cfg predicates unresolved instead of guessing. | `source_analysis_uses_selected_cargo_features_for_cfg_attrs`, `check-source-cfg-attributes.sh` |
| MOK001 ignores configured `new_test_ext` and counts callback arguments | Respect every configured mock pattern and inspect only call targets/receivers, not arbitrary call arguments. | MOK001 rule suite |
| TST004 uses a separate test-file heuristic | Reuse the engine’s test and benchmark target classification. | TST004 rule suite |
| Local builds can select an unsupported Rust version | Pin the same Rust 1.93.0 toolchain used by CI for ordinary Cargo commands. | `rust-toolchain.toml` |
| SEM009 compares only storage owners | Require exact normalized argument equality between `contains_key` and `remove`/`take`. | `sem009_does_not_match_different_storage_keys` |
| Input bounds are matched by substrings | Use AST-local `ensure!` length bounds and typed conversions; retain special fixed-size transformations only when the parameter is an actual call argument. | `sec001_recognizes_cast_length_bounds_for_the_correct_parameter`, `sec001_does_not_treat_similar_identifiers_as_input_bounds`, SEC018 bound-handling suite |
| `ensure_signed_or_root` is unknown | Classify it as mixed access because signed callers remain possible. | `sec001_treats_signed_or_root_as_callable_by_signed_origins` |
| VAL001 reads comments, strings, and `Get` constants as storage access | Strip inline non-code before pattern matching and exclude `T::...::get()` type-level constants without removing storage-alias `::get()` coverage. | `val001_ignores_comments_strings_and_get_constants` |
| VAL003 misses same-line write/validation order | Compare write and validation spans, including standalone macro statements on the same source line. | `val003_detects_validation_after_write_on_the_same_line` |
| SEC017 chooses an earlier or identifier-internal `let` | Select the latest identifier-bound `let`/`let mut` before the capacity assignment. | `sec017_allows_vec_event_payloads_derived_from_weighted_inputs` |
| SEC010 treats any matching function name as a hook and misses qualified transactional attributes | Analyze only `Hooks` trait impls and match the terminal `transactional` attribute segment. | `sec010_accepts_qualified_transactional_attributes`, `sec010_ignores_non_hooks_methods_with_hook_names` |
| Source stripping mishandles block comments, character literals, and raw strings | Use a stateful Rust-literal/comment sanitizer for full-source consumers. | `ben002_does_not_treat_block_comments_as_verification`, `sem008_ignores_comments_and_literals_without_hiding_following_code` |
| SEC003 misses qualified trait decode paths | Inspect the qualified self type of decode calls as well as the visible path. | `sec003_detects_qualified_runtime_call_decode_without_limit` |
| SEC004 duplicates nested arithmetic and flags literal-only calculations | Deduplicate weight arithmetic by source line and ignore expressions whose operands are literal constants. | `sec004_reports_nested_runtime_arithmetic_once_per_line`, `sec004_allows_literal_weight_arithmetic` |
| SEC015 lets any earlier root guard whitelist later bypasses | Track strict root guards in traversal order and only within the current block or closure scope. | `sec015_does_not_whitelist_bypass_after_sibling_root_guard`, `sec015_does_not_whitelist_bypass_in_closure_after_root_guard` |
| SEM014 has narrow line windows for submission and log targets | Associate wrapped signed or unsigned submissions with the full nearby log macro and deduplicate reports by log site. | `sem014_handles_wrapped_unsigned_submission_and_long_log_target` |
| SARIF omits diagnostic end lines | Serialize `endLine` whenever the diagnostic contains an end span. | `sarif_preserves_end_line_when_available` |
| SEM002 skips nested local bindings | Recurse from every local binding so closure and nested initializer locals are inspected. | `sem002_detects_typed_collect_inside_closure` |
| SEM013 treats every `*Invalidity` enum as a custom transaction invalidity code | Restrict the convention to the documented `CustomInvalidity` and `CustomValidity` enum names. | `sem013_ignores_unrelated_invalidity_enum_names` |
| TRM001 skips identifiers on lines containing strings or comments | Check sanitized code identifiers alongside configured string and comment text. | `trm001_checks_identifiers_on_lines_with_strings_and_comments` |
| Test and benchmark path detection assumes POSIX separators | Normalize path separators before applying target-file heuristics. | `target_classification_accepts_windows_path_separators` |
| Config documents ignored severity, setup, and benchmark options | Remove no-op schema entries and reject unknown configuration keys instead of silently accepting them. | `project_configuration_uses_the_supported_schema`, `deserialization_rejects_unknown_configuration_options` |
| TST005 treats `1.0` float literals as tuple-field assertions | Remove the `\\.0\\b` fallback pattern as it cannot distinguish literals from tuple-field access. | `partial_test_smell_config_does_not_match_float_literals` |
| Overlapping CLI scan paths produce repeated, unsorted diagnostics | Normalize the combined syntax and compiler result set by full location/rule identity before rendering output. | `normalizes_order_and_removes_overlapping_scan_duplicates` |
| Dynamic-library path joining can erase the caller's library search path | Retain the existing environment value when path joining cannot safely prepend the toolchain directory. | `preserves_existing_library_path_when_joining_fails` |
| Comma-containing compiler source paths split into multiple filters | Serialize file filters as JSON across the linter-driver boundary; the driver retains legacy environment parsing for older invocations. | `encodes_file_filters_without_delimiter_ambiguity`, `check-rustc-hard-rules.sh` |
| Source-rule corpus baseline predates the review fixes | Regenerate the pinned `substrate/frame` baseline after validating the intentional BEN001/BEN002/BEN003, SEM002, TST, and VAL001 count changes against the pinned SDK commit. | `check-source-rule-corpus.sh .repos/polkadot-sdk` |
| Authority and best-practice docs describe unimplemented rules as enforced | Derive the authority matrix from the public registry and mark unsupported rule IDs as scope boundaries rather than coverage. | `docs/rule-authority-matrix.md`, `docs/best-practices.md` |

## Still Open

The review is not fully resolved. The next implementation work should address:

1. Rule-specific compiler-driver regression coverage for every public
   compiler-backed rule. The current pinned-SDK gate covers the hard security
   rules and `VAL002`; a source-rule test alone does not prove production
   behavior where rustc is authoritative.

Run the core checks with:

```bash
cargo +1.93.0 test
cargo +1.93.0 clippy --all-targets -- -D warnings
cargo +nightly-2025-09-01 build --features rustc-driver --bin polkadot-linter-driver
```
