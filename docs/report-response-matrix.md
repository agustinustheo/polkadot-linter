# Report response matrix

Source reports:

- `research-comment.md`: benchmark research against `paritytech/polkadot-sdk`
  at `b18fb34a8ae348df5866e4b718d82871d744e60d`
- `audit-report-comment.md`: manually verified SDK findings from that benchmark

Current branch:

- worktree branch: `fix/implementation-bugs-false-positives`
- pinned SDK submodule commit: `b18fb34a8ae348df5866e4b718d82871d744e60d`
- focused CI syntax benchmark: 13 validated `SEC018` findings
- unrestricted syntax benchmark: 348 findings at the pinned SDK commit; run
  `scripts/benchmark-unrestricted-sec-rules.sh` with `config/default.toml`
  to reproduce this noise metric. It is not an audit-authoritative result.
- unrestricted SEC scan: 348 findings

Validation evidence used for this matrix:

- `cargo +1.93.0 test`: 297 tests passed
- `cargo +1.93.0 clippy --all-targets -- -D warnings`: passed
- `cargo +1.93.0 build`: passed
- `scripts/check-rustc-hard-rules.sh`: `SEC001` syntax path 0 findings,
  rustc path 1 finding; `SEC002` syntax path 4 findings, rustc path 3
  findings; `SEC003` syntax path 8 findings, rustc path 8 findings; `SEC008`
  syntax path 10 findings, rustc path 4 findings; `SEC009` syntax path 5
  findings, rustc path 2 findings; `SEC011` syntax path 1 finding, rustc path
  8 findings; `SEC012` syntax path 6 findings, rustc path 3 findings; `SEC013`
  syntax path 0 findings, rustc path 1 finding; `SEC017` syntax path 0
  findings, rustc path 1 finding; `SEC018` syntax path 0 findings, rustc path
  3 findings; rustc driver rule filter `SEC008,SEC009` emitted exactly those 6
  findings and no other rule IDs
- `scripts/check-rustc-sdk-smoke.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-multisig` checked successfully through the public `polkadot-linter`
  CLI using automatic Cargo manifest discovery and default compiler-backed routing, with
  7 package-local, deduplicated rustc-backed `SEC001`/`SEC008` diagnostics
  captured in the public JSON output after syntax findings for those migrated
  rule IDs were demoted; baseline
  `benchmarks/polkadot-sdk-rustc-multisig-sec001-sec008-baseline.tsv` matched
- `scripts/check-rustc-sdk-sec002.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-multisig` syntax emitted 0 findings while rustc resolves the public
  `cancel_as_multi` `debug_assert!` at line 548; baseline
  `benchmarks/polkadot-sdk-rustc-multisig-sec002-baseline.tsv` is checked in CI
- `scripts/check-rustc-sdk-sec003.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-xcm` checked successfully through the public `polkadot-linter` CLI
  using automatic Cargo manifest discovery and default compiler-backed routing; the
  current syntax rule reports 0 package-local `SEC003` findings while the
  rustc-backed rule reports 1 associated-projection decode finding at
  `polkadot/xcm/pallet-xcm/src/lib.rs:4111`; baseline
  `benchmarks/polkadot-sdk-rustc-pallet-xcm-sec003-baseline.tsv` matched
- `scripts/check-rustc-sdk-sec009.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-collective` checked successfully through the public
  `polkadot-linter` CLI using automatic Cargo manifest discovery and default
  compiler-backed routing; the current syntax rule reports 5 package-local
  `SEC009` findings while the final routed output contains the 2 rustc-backed
  resolved-integer findings; baseline
  `benchmarks/polkadot-sdk-rustc-collective-sec009-baseline.tsv` matched
- `scripts/check-rustc-sdk-sec018.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-contracts` checked successfully through automatic Cargo manifest
  discovery; the compiler-backed route reports the audited `data` input in
  `contracts::call` at `substrate/frame/contracts/src/lib.rs:944` and excludes
  the deprecated compatibility dispatchable; baseline
  `benchmarks/polkadot-sdk-rustc-contracts-sec018-baseline.tsv` matched
- `scripts/check-rustc-sdk-sec013.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-session` checked successfully through automatic Cargo manifest
  discovery; syntax emitted 4 findings while the compiler-backed storage-value
  model emitted the 3 validated values and excluded the unbounded `KeyOwner`
  key; baseline `benchmarks/polkadot-sdk-rustc-session-sec013-baseline.tsv`
  matched
- `scripts/check-rustc-sdk-sec017.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-root-offences` checked through automatic Cargo manifest discovery;
  syntax emitted 0 findings while the compiler-backed event model reported the
  unbounded `OffenceCreated::offenders` payload at line 116; baseline
  `benchmarks/polkadot-sdk-rustc-root-offences-sec017-baseline.tsv` matched
- `scripts/check-rustc-sdk-sec008.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-multisig` checked through automatic Cargo manifest discovery; syntax
  emitted 0 findings while the compiler-backed path emitted 2 resolved
  reachable `.expect()` paths; baseline
  `benchmarks/polkadot-sdk-rustc-multisig-sec008-baseline.tsv` matched
- `scripts/check-rustc-sdk-sec011.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-multisig` migration coverage: syntax emitted 0 package findings while
  rustc resolves the `OnRuntimeUpgrade` `Calls::drain` at line 56; baseline
  `benchmarks/polkadot-sdk-rustc-multisig-sec011-baseline.tsv` is checked in CI
- `scripts/check-rustc-sdk-sec012.sh .repos/polkadot-sdk .benchmarks`:
  `pallet-oracle` checked through compiler-backed Cargo analysis; syntax emitted
  0 package-local findings while rustc resolved the `RawValues` storage alias
  and emitted the unbounded `clear_prefix` finding at line 440; baseline
  `benchmarks/polkadot-sdk-rustc-oracle-sec012-baseline.tsv` matched
- `scripts/benchmark-sec-rules.sh .repos/polkadot-sdk .benchmarks`: 13
  focused `SEC018` findings
- `scripts/check-sec-benchmark-baseline.sh
  .benchmarks/sec-rules-20260711T172345Z.json`: baseline matched
- unrestricted scan output:
  `/tmp/polkadot-linter-sec-after-default-rustc-routing-unrestricted.json`

## Research report concerns

| Report concern | Current status | Evidence / response |
| --- | --- | --- |
| Full SEC run produced 5,563 findings across 15 rules. | Fixed for focused CI benchmark; still true that unrestricted SEC remains noisy. | The focused syntax benchmark is intentionally limited to the validated `SEC018` baseline and emits 13 findings. The unrestricted syntax scan now emits 348 findings, not 5,563, but that unrestricted mode is still not audit-grade. |
| `SEC004` and `SEC005` produced zero findings, so usefulness could not be judged from the benchmark. | Still true. | No current SDK benchmark evidence establishes audit precision for these rules. They are not part of the minimum compiler-backed migration set unless later evidence identifies them as semantically hard. |
| `SEC001` had 104 findings with 60% sampled FP rate. | Partially superseded by rustc-driver increment; not fully migrated. | Phase 1 added privileged-origin and bounded-input handling. Current unrestricted count is 7. The rustc path resolves aliased `Vec` inputs only for source-linked FRAME dispatchables, excluding public helpers; `scripts/check-rustc-sdk-smoke.sh` preserves the five validated multisig dispatchable findings. Remaining trust depends on resolved origin, parameter bounds, and input-flow evidence. |
| `SEC001` false positives on `ensure_root` or privileged-origin extrinsics. | Partially superseded by rustc-driver increment; not fully migrated. | The rustc rule now resolves `frame_system::ensure_root` and FRAME `EnsureOrigin::{ensure_origin, ensure_origin_or_root}` calls tied to named privileged configuration origins, while retaining unprivileged dispatchable inputs. Alias, delegated, and arbitrary configured-origin semantics still need compiler-backed modeling. |
| `SEC002` had 457 findings with 77% sampled FP rate. | Partially superseded by rustc-driver increment; not fully migrated. | Current unrestricted count is 70. Many exact SDK invariant clusters are suppressed with tests. `scripts/check-rustc-hard-rules.sh` proves the rustc path reports expanded active and nested-macro `debug_assert!` calls, skips cfg-disabled source, retains a directly-called private helper, and drops an unreachable private helper. For FRAME attribute expansions that collapse the inner HIR span, the rustc-selected reachable function supplies a direct macro-source fallback; comments and strings do not qualify. `scripts/check-rustc-sdk-sec002.sh` adds pinned SDK coverage for the public multisig `cancel_as_multi` assertion where syntax emits 0 findings. Indirect-call and control-flow evidence remain incomplete. |
| `SEC003` had 571 findings with 97% sampled FP rate. | Partially superseded by rustc-driver increment; not fully migrated. | `polkadot-linter-rustc` now reports decode calls using resolved return/receiver types, handles associated projections such as `<T as Config>::RuntimeCall`, and requires propagated input evidence from entry parameters through locals, local aliases and assignments passed to direct local helpers, match patterns, `using_encoded` closures, and the XCM `OnResponse` callback. `scripts/check-rustc-hard-rules.sh` proves it catches aliased, match-derived, directly-called private helper, local-alias-to-private-helper, local-assignment-to-private-helper, and `OnResponse` runtime decodes while skipping an internal buffer and an unreachable private helper. `scripts/check-rustc-sdk-sec003.sh` proves SDK coverage on `pallet-xcm`: syntax reports 0 findings while rustc reports the associated-projection decode at line 4111. Indirect-call taint and broader SDK package coverage remain. |
| `SEC003` fired on non-recursive internal types and non-user-controlled decode calls. | Partially superseded by rustc-driver increment; still incomplete. | The rustc fixture skips `MigrationState::decode` because the resolved type is not a recursive runtime target and skips `RuntimeCall::decode` over an internal constant buffer. It removes taint after an unconditional local overwrite, conservatively retains taint after a conditional overwrite, drops an unreachable private helper, and retains a private helper that receives tainted entry-point input. The SDK check filters macro-generated pallet attribute decode noise. Indirect-call and broader path-sensitive input taint remain future compiler-backed/dataflow work. |
| `SEC006` had 7 findings with 86% sampled FP rate. | Partially fixed; residual low volume remains. | Current unrestricted count is 1. Existing tests cover checked/discarded repatriate patterns. No compiler-backed migration is currently required by the report, but any remaining finding still needs manual validation. |
| `SEC007` had 28 findings with 89% sampled FP rate. | Partially fixed; residual low volume remains. | Current unrestricted count is 2. Tests cover propagated errors, intentional ignores, and `try_mutate` false positives. |
| `SEC008` had 3,524 findings with 94% sampled FP rate and was the biggest noise source. | Partially superseded by rustc-driver increment; not fully migrated. | Current unrestricted count is 45. Stabilization removed benchmark/test/genesis/runtime-benchmark noise and documented invariant traps. The rustc path suppresses `Result<T, Infallible>::unwrap()`, private helper-only `expect` calls, and unrelated trait implementation bodies while retaining public fallible paths and direct local helper calls. `scripts/check-rustc-sdk-sec008.sh` proves 2 compiler-only `pallet-multisig` findings where syntax reports 0; the hard-rule fixture proves a directly-called private helper is retained while an unreachable one is not. Indirect reachability, macro-expanded cfg, and control-flow evidence remain future work. |
| `SEC008` false positives in `genesis_build`, benchmarks, `runtime-benchmarks`, and type-provably infallible conversions. | Partially superseded by rustc-driver increment; still incomplete. | Tests cover benchmark/runtime-benchmark/genesis/helper paths. The rustc fixture now covers one type-provably infallible conversion via `Result<T, Infallible>`, locals constructed as `Ok`/`Some`, and resolved `Option`/`Result` values proven present after a terminating guard; an overwrite with an unknown fallible result remains reported. Broader panic reachability and debug assertion handling still require compiler-backed control-flow and cfg evidence. |
| `SEC009` had 706 findings with 79% sampled FP rate. | Partially superseded by rustc-driver increment; not fully migrated. | `polkadot-linter-rustc` reports `SEC009` from HIR/typeck for resolved integer operands reachable from public or hook fallible entry points, including direct local helpers. `scripts/check-rustc-hard-rules.sh` proves overloaded `Add`, an `a - b` subtraction guarded by `if a >= b`, an early-return `ensure!(a >= b, ...)` guard, and an unreachable private helper are removed while a helper reached by a fallible entry point is retained. `scripts/check-rustc-sdk-sec009.sh` proves SDK package precision on `pallet-collective`: syntax reports 5 findings, while default compiler-backed routing emits the 2 rustc-backed resolved-integer findings and drops generic/operator and macro-generated noise. Indirect reachability and broader path-sensitive control flow remain future work. |
| `SEC010` had 16 findings with 88% sampled FP rate. | Still true unless later scoped evidence proves otherwise. | Current unrestricted scan has no `SEC010` findings after stabilization, but there is no compiler-backed proof for hook transactional semantics. |
| `SEC011` had 4 findings with 100% sampled FP rate. | Partially superseded by rustc-driver increment; not fully migrated. | Current unrestricted count is 1. `scripts/check-rustc-hard-rules.sh` proves the rustc path resolves associated-call owner types, reports `StorageMap::iter()` through direct private helpers plus resolved `Hooks`, `OnRuntimeUpgrade`, `UncheckedOnRuntimeUpgrade`, and the SDK `polkadot_sdk_frame` facade callback paths, skips an unreachable private helper, and removes a syntax-only `Domain::iter()` false positive. `scripts/check-rustc-sdk-sec011.sh` adds pinned SDK evidence: syntax reports 0 findings while rustc detects the multisig migration `Calls::drain`. Broader dispatchable/hook coverage still needs the full compiler-backed FRAME model. |
| `SEC012` had 14 findings with 64% sampled FP rate. | Partially superseded by rustc-driver increment; not fully migrated. | Current unrestricted count is 8. `scripts/check-rustc-hard-rules.sh` proves the rustc path resolves `clear_prefix` owner types, reports unbounded limits through a direct private helper, skips an unreachable private helper, and removes a syntax-only `Domain::clear_prefix` false positive. `scripts/check-rustc-sdk-sec012.sh` proves compiler-backed SDK coverage for the `pallet-oracle` `RawValues` callback at line 440, while syntax emits 0 package-local findings. |
| `SEC013` had 55 findings with 60% sampled FP rate. | Partially superseded by rustc-driver increment; not fully migrated. | Current unrestricted count is 45. The rustc path resolves source-linked `#[pallet::storage]` aliases and inspects storage values rather than collection-like keys. `scripts/check-rustc-sdk-sec013.sh` proves improved SDK precision on `pallet-session`: 3 stored unbounded values are retained while the syntax-only `KeyOwner` key false positive is removed. |
| `SEC013` false positives on bounded storage wrappers. | Partially superseded by rustc-driver increment; not fully migrated. | Syntax tests cover bounded wrappers and capacity-limited docs. The rustc fixture now also skips a bounded storage alias after type resolution. Remaining risk is full FRAME storage expansion and SDK-scale proof. |
| `SEC014` had 14 findings with 100% sampled FP rate. | Fixed for current SDK unrestricted scan. | Current unrestricted scan has no `SEC014` findings. Existing tests cover account-id keys and documented internal numeric layouts. |
| `SEC015` had 12 findings with 75% sampled FP rate. | Fixed for current SDK unrestricted scan. | Current unrestricted scan has no `SEC015` findings. Existing tests cover verified root branches and bypass patterns. |
| `SEC016` had 30 findings with 83% sampled FP rate. | Partially fixed; residual low volume remains. | Current unrestricted count is 2. Tests cover `VersionedMigration`, reconciliation, permanent migrations, and FRAME migration helpers. |
| `SEC016` false positives for `VersionedMigration<N, M, ...>`. | Fixed with tests for the known pattern. | Stabilization added versioned migration handling and regression coverage. Remaining `SEC016` findings need manual review or stronger semantic modeling. |
| `SEC017` had 21 findings with 57% sampled FP rate. | Partially superseded by rustc-driver increment; not fully migrated. | Current unrestricted count is 12. The rustc path resolves source-linked FRAME event payloads, including aliases to `Vec`, while skipping bounded wrappers and ordinary enums. `scripts/check-rustc-sdk-sec017.sh` proves SDK coverage on `pallet-root-offences`: syntax reports 0 findings while rustc reports the unbounded `OffenceCreated::offenders` payload at line 116. Event input-flow and weight-accounting evidence remain needed for audit grade. |
| Recommendation: run only `SEC001`, `SEC012`, `SEC013`, `SEC017` diff-scoped with a cap. | Superseded by current stabilization direction, not by final implementation yet. | The branch instead uses a focused validated `SEC018` benchmark and keeps unrestricted scans as stabilization evidence. The final goal is a compiler-backed linter, not a capped syntax-only integration. |
| Recommendation: improve existing rule implementations. | Partially implemented. | Phase 1 added narrow, evidence-backed fixes and regression tests, reducing the unrestricted scan from the stale 5,563-result report to 348 current findings. |
| Recommendation: develop new rules for weight annotations missing user-controlled input sizes. | Implemented as `SEC018`, with a rustc-backed macro-recovery increment; upstream findings are not fixed here. | `SEC018` is now the focused CI benchmark rule. The validated SDK baseline contains 13 findings, including the report's audit findings. `scripts/check-rustc-hard-rules.sh` proves the rustc path resolves an aliased unbounded input, and `scripts/check-rustc-sdk-sec018.sh` proves macro-consumed `#[pallet::weight]` source metadata is paired with the resolved `contracts::call` input while excluding the deprecated compatibility call. |
| Recommendation: rewrite as a focused security linter. | In progress through compiler-backed migration, not complete. | A `rustc_driver` entry point now exists with typed fixture-backed increments for `SEC001`, `SEC002`, `SEC003`, `SEC008`, `SEC009`, `SEC011`, `SEC012`, `SEC013`, `SEC017`, and `SEC018`. The public CLI now discovers a single project manifest and invokes that driver by default for migrated SEC rules, demoting syntax findings for those rule IDs. The semantically hard rules still need broader migration and SDK benchmark proof. |

## Audit report findings

| Audit finding | Current status | Evidence / response |
| --- | --- | --- |
| `contracts::call` weight does not account for `data` length. | Out of scope for this repository; upstream `polkadot-sdk` fix. Represented by linter output. | The linter now tracks this class as `SEC018`. The focused benchmark baseline includes `substrate/frame/contracts/src/lib.rs` findings. This repo should not patch SDK code. |
| `multisig::as_multi_threshold_1` weight does not scale with signatories count. | Out of scope for this repository; upstream `polkadot-sdk` fix. Represented by linter output. | The validated `SEC018` baseline includes `substrate/frame/multisig/src/lib.rs`. This repository can detect/report it but does not change SDK weights. |
| `society::found_society` performs O(n) hash without proportional weight. | Out of scope for this repository; upstream `polkadot-sdk` fix. Represented by linter output. | The validated `SEC018` baseline includes `substrate/frame/society/src/lib.rs`. This remains an informational upstream issue. |

## Current conclusion

The immediate benchmark-noise problem is mitigated for CI because the focused
syntax baseline explicitly uses `--no-rustc` and emits only the validated
`SEC018` findings. The old 5,563-finding benchmark is stale.

The unrestricted rule set still emits 348 findings. That is evidence that Phase
1 stabilization is not a substitute for Phase 2. The remaining hard classes are
raw arithmetic, decode-depth, panic/debug-assert reachability,
SDK-scale weight/input-accounting dataflow, and unbounded input/storage
analysis.

Only the first rustc-driver increments for `SEC001`, `SEC002`, `SEC003`,
`SEC008`, `SEC009`, `SEC011`, `SEC012`, `SEC013`, `SEC017`, and `SEC018` have
been implemented. The stable CLI now has CI-covered Cargo integration that uses
automatic manifest-backed routing for migrated SEC rules and demotes syntax
findings for those rule IDs. The full compiler-backed transition remains
incomplete until the hard rules above have broader SDK benchmark proof,
interprocedural dataflow/control-flow evidence where needed, and migration of
the remaining syntax-authoritative rules.
