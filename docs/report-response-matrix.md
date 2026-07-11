# Report response matrix

Source reports:

- `research-comment.md`: benchmark research against `paritytech/polkadot-sdk`
  at `b18fb34a8ae348df5866e4b718d82871d744e60d`
- `audit-report-comment.md`: manually verified SDK findings from that benchmark

Current branch:

- worktree branch: `fix/implementation-bugs-false-positives`
- pinned SDK submodule commit: `b18fb34a8ae348df5866e4b718d82871d744e60d`
- focused CI benchmark: 13 validated `SEC018` findings
- unrestricted SEC scan: 348 findings

Validation evidence used for this matrix:

- `cargo +1.93.0 test`: 297 tests passed
- `cargo +1.93.0 clippy --all-targets -- -D warnings`: passed
- `cargo +1.93.0 build`: passed
- `scripts/check-rustc-sec009.sh`: syntax path 2 findings, rustc path 1 finding
- `scripts/benchmark-sec-rules.sh .repos/polkadot-sdk .benchmarks`: 13
  focused `SEC018` findings
- `scripts/check-sec-benchmark-baseline.sh
  .benchmarks/sec-rules-20260711T121656Z.json`: baseline matched
- unrestricted scan output:
  `/tmp/polkadot-linter-sec-after-rustc-driver-default-unrestricted.json`

## Research report concerns

| Report concern | Current status | Evidence / response |
| --- | --- | --- |
| Full SEC run produced 5,563 findings across 15 rules. | Fixed for default CI benchmark; still true that unrestricted SEC remains noisy. | The default benchmark is now intentionally focused on the validated `SEC018` baseline and emits 13 findings. The unrestricted scan now emits 348 findings, not 5,563, but that unrestricted mode is still not audit-grade. |
| `SEC004` and `SEC005` produced zero findings, so usefulness could not be judged from the benchmark. | Still true. | No current SDK benchmark evidence establishes audit precision for these rules. They are not part of the minimum compiler-backed migration set unless later evidence identifies them as semantically hard. |
| `SEC001` had 104 findings with 60% sampled FP rate. | Partially fixed; still true as an unrestricted audit-grade concern. | Phase 1 added privileged-origin and bounded-input handling. Current unrestricted count is 7. Remaining trust depends on resolved origin, parameter bounds, and input-flow evidence, so this belongs with compiler-backed unbounded input analysis. |
| `SEC001` false positives on `ensure_root` or privileged-origin extrinsics. | Partially fixed with tests; still needs semantic origin resolution for audit grade. | Stabilization added narrow privileged-origin and bounded-`Vec` cases with regression tests. Syntax matching cannot fully prove origin semantics across aliases/helpers. |
| `SEC002` had 457 findings with 77% sampled FP rate. | Partially fixed; still true as a hard rule. | Current unrestricted count is 70. Many exact SDK invariant clusters are suppressed with tests, but production reachability and debug-assert safety require compiler cfg expansion and control-flow context. |
| `SEC003` had 571 findings with 97% sampled FP rate. | Still true; requires compiler-backed migration. | Current focused benchmark does not run this rule. Accurate decode-depth analysis needs resolved decoded types, recursive structure, and user-controlled input evidence. |
| `SEC003` fired on non-recursive internal types and non-user-controlled decode calls. | Still true as a syntax-level limitation. | The Phase 1 audit explicitly lists decode-depth analysis as not reliably fixable with token-level suppressions. |
| `SEC006` had 7 findings with 86% sampled FP rate. | Partially fixed; residual low volume remains. | Current unrestricted count is 1. Existing tests cover checked/discarded repatriate patterns. No compiler-backed migration is currently required by the report, but any remaining finding still needs manual validation. |
| `SEC007` had 28 findings with 89% sampled FP rate. | Partially fixed; residual low volume remains. | Current unrestricted count is 2. Tests cover propagated errors, intentional ignores, and `try_mutate` false positives. |
| `SEC008` had 3,524 findings with 94% sampled FP rate and was the biggest noise source. | Partially fixed; still true as a hard rule. | Current unrestricted count is 45. Stabilization removed benchmark/test/genesis/runtime-benchmark noise and documented invariant traps, but panic reachability still needs macro-expanded cfg and control-flow evidence. |
| `SEC008` false positives in `genesis_build`, benchmarks, `runtime-benchmarks`, and type-provably infallible conversions. | Partially fixed with tests; type-provable cases still require compiler-backed analysis. | Tests cover benchmark/runtime-benchmark/genesis/helper paths and several bounded/infallible-looking conversions. Full proof of infallible conversions needs resolved types and dataflow. |
| `SEC009` had 706 findings with 79% sampled FP rate. | Partially superseded by first rustc-driver increment; not fully migrated. | `polkadot-linter-rustc` now reports `SEC009` from HIR/typeck for integer operands only. `scripts/check-rustc-sec009.sh` proves overloaded `Add` is removed on a fixture. The default unrestricted syntax path still emits 142 `SEC009` findings, so the rule is not yet fully compiler-backed. |
| `SEC010` had 16 findings with 88% sampled FP rate. | Still true unless later scoped evidence proves otherwise. | Current unrestricted scan has no `SEC010` findings after stabilization, but there is no compiler-backed proof for hook transactional semantics. |
| `SEC011` had 4 findings with 100% sampled FP rate. | Partially fixed; residual low volume remains. | Current unrestricted count is 1. Accurate storage-iteration risk still depends on resolved storage API semantics and bounds. |
| `SEC012` had 14 findings with 64% sampled FP rate. | Partially fixed; still not default audit-grade. | Current unrestricted count is 8. The default benchmark does not include `SEC012`; it remains disabled from the focused CI baseline because the report found material noise. |
| `SEC013` had 55 findings with 60% sampled FP rate. | Partially fixed; still requires compiler-backed storage modeling. | Current unrestricted count is 45. A rustdoc-backed prototype exists, but real FRAME macro output does not preserve enough storage alias/value detail for full SDK migration. Final precision requires rustc/Clippy-grade macro-expanded storage analysis. |
| `SEC013` false positives on bounded storage wrappers. | Partially fixed with tests; not complete. | Syntax tests cover bounded wrappers and capacity-limited docs. The remaining issue is resolved type aliases and FRAME storage expansion. |
| `SEC014` had 14 findings with 100% sampled FP rate. | Fixed for current SDK unrestricted scan. | Current unrestricted scan has no `SEC014` findings. Existing tests cover account-id keys and documented internal numeric layouts. |
| `SEC015` had 12 findings with 75% sampled FP rate. | Fixed for current SDK unrestricted scan. | Current unrestricted scan has no `SEC015` findings. Existing tests cover verified root branches and bypass patterns. |
| `SEC016` had 30 findings with 83% sampled FP rate. | Partially fixed; residual low volume remains. | Current unrestricted count is 2. Tests cover `VersionedMigration`, reconciliation, permanent migrations, and FRAME migration helpers. |
| `SEC016` false positives for `VersionedMigration<N, M, ...>`. | Fixed with tests for the known pattern. | Stabilization added versioned migration handling and regression coverage. Remaining `SEC016` findings need manual review or stronger semantic modeling. |
| `SEC017` had 21 findings with 57% sampled FP rate. | Partially fixed; still not default audit-grade. | Current unrestricted count is 12. Tests cover bounded, weight-accounted, and derived event payloads, but event payload safety still depends on input-flow evidence. |
| Recommendation: run only `SEC001`, `SEC012`, `SEC013`, `SEC017` diff-scoped with a cap. | Superseded by current stabilization direction, not by final implementation yet. | The branch instead uses a focused validated `SEC018` benchmark and keeps unrestricted scans as stabilization evidence. The final goal is a compiler-backed linter, not a capped syntax-only integration. |
| Recommendation: improve existing rule implementations. | Partially implemented. | Phase 1 added narrow, evidence-backed fixes and regression tests, reducing the unrestricted scan from the stale 5,563-result report to 348 current findings. |
| Recommendation: develop new rules for weight annotations missing user-controlled input sizes. | Implemented as `SEC018`, but upstream findings are not fixed here. | `SEC018` is now the focused CI benchmark rule. The validated SDK baseline contains 13 findings, including the report's audit findings. |
| Recommendation: rewrite as a focused security linter. | In progress through compiler-backed migration, not complete. | A `rustc_driver` entry point now exists, but only the first `SEC009` typed fixture is implemented. The semantically hard rules still need full migration and SDK benchmark proof. |

## Audit report findings

| Audit finding | Current status | Evidence / response |
| --- | --- | --- |
| `contracts::call` weight does not account for `data` length. | Out of scope for this repository; upstream `polkadot-sdk` fix. Represented by linter output. | The linter now tracks this class as `SEC018`. The focused benchmark baseline includes `substrate/frame/contracts/src/lib.rs` findings. This repo should not patch SDK code. |
| `multisig::as_multi_threshold_1` weight does not scale with signatories count. | Out of scope for this repository; upstream `polkadot-sdk` fix. Represented by linter output. | The validated `SEC018` baseline includes `substrate/frame/multisig/src/lib.rs`. This repository can detect/report it but does not change SDK weights. |
| `society::found_society` performs O(n) hash without proportional weight. | Out of scope for this repository; upstream `polkadot-sdk` fix. Represented by linter output. | The validated `SEC018` baseline includes `substrate/frame/society/src/lib.rs`. This remains an informational upstream issue. |

## Current conclusion

The immediate benchmark-noise problem is mitigated for CI because the default
benchmark now emits only the validated `SEC018` baseline. The old 5,563-finding
benchmark is stale.

The unrestricted rule set still emits 348 findings. That is evidence that Phase
1 stabilization is not a substitute for Phase 2. The remaining hard classes are
raw arithmetic, decode-depth, panic/debug-assert reachability,
weight/input-accounting dataflow, and unbounded input/storage analysis.

Only the first rustc-driver increment for `SEC009` has been implemented. The
full compiler-backed transition remains incomplete until the hard rules above
run through the compiler-backed pipeline with SDK benchmark proof and CI
coverage.
