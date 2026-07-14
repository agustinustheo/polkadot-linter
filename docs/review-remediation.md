# Review Remediation

This page tracks the remediation work begun from the 2026-07-14 full-project
review. A rule is not considered audit-grade merely because its fixture passes:
every listed fix has a regression test that reproduces the reviewed shape.

## Resolved In This Tranche

| Review finding | Resolution | Evidence |
| --- | --- | --- |
| VAL001 stops at multiline signatures | Keep scanning until the function body has opened and closed; support restricted visibility functions. | `val001_detects_multiline_dispatchable_signatures` |
| `#[cfg(test)]` masks the rest of a file | Recognize additional block item forms and do not leave a pending mask after an inline attribute/item. | `sec008_does_not_mask_production_after_cfg_test_const_fn` |
| Implicit FRAME call indices are omitted | Treat every method in `#[pallet::call]` as a dispatchable; `call_index` is optional metadata. | `sec001_checks_dispatchables_without_explicit_call_indices` |
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

## Still Open

The review is not fully resolved. The next implementation work should address:

1. Config options that are declared but ignored, and the divergence between
   `config/default.toml` and `Config::default`.
2. Remaining source-rule precision issues, particularly bounds/dataflow matching,
   string/comment stripping, root-guard dominance, and retired parser rules.
3. Rule-specific compiler-driver regression coverage for public compiler-backed
   SEC/VAL diagnostics. A source-rule test alone does not prove production behavior
   for a rule whose public authority is rustc.

Run the core checks with:

```bash
cargo +1.93.0 test
cargo +1.93.0 clippy --all-targets -- -D warnings
cargo +nightly-2025-09-01 build --features rustc-driver --bin polkadot-linter-driver
```
