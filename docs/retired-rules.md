# Retired rules: what we tried, why it failed, where to catch it instead

polkadot-linter tested one idea: can static analysis find real bugs in Polkadot SDK code? We wrote
64 rules, ran them on real projects and judged each finding by hand. The bar: a finding counts only
if an engineer would fix it at reasonable cost. True but not worth fixing counts as a miss.
Measurements: two public audits sampled 12-15 findings per rule on 10 parachain repos and polkadot-sdk
(`benchmarks/precision/2026-11-external`, `2026-11-run-audit`; these used a looser label, so their
"useful" counts are upper bounds). An acceptance test judged all 39 findings on
`paritytech/individuality-community` ("IC") with the strict label. A private benchmark ran every rule
at the commits security auditors reviewed, against 56 bugs they reported. Per-rule numbers are in
`benchmarks/precision/retired-rules-evidence.tsv`.
The 33 rules below failed, and were deleted from the code. This page is what is left of them. None caught any of the 56 auditor bugs for the right reason. On IC, 31 of
the 39 findings came from four of them, and none was worth fixing.

## Benchmarks

### BEN001: weight function without a benchmark
- **Tried:** flag `WeightInfo` functions with no benchmark of the same name.
- **Evidence:** external 4 of 12 useful, 4 wrong. Auditor bugs: one related hit, not for the right reason.
- **Why it failed:** benchmarks live in many places: the pallet, a companion crate, the runtime
  crate, a helper-built closure. The rule had to guess where. Every missed place became a wrong
  finding. It also flagged weight functions that nothing calls.
- **Catch it instead:** RTC002 (kept) checks that each runtime pallet is in `define_benchmarks!`.
  Run `impl_benchmark_test_suite!` tests in CI. In review, regenerated `weights.rs` must match the benchmarks.
- **Worth rebuilding?** A smaller "missing benchmark" rule could be worth it if it can reliably find
  where benchmarks live (for example from the compiler's expanded `SelectedBenchmark` list) and only reports what it can prove.

### BEN002: benchmark without a verify block
- **Tried:** flag benchmarks with no assertion after the measured call.
- **Evidence:** external 10 of 12, run audit 15 of 23 under the loose label. IC: 0 of 9 worth fixing, 1 wrong.
  In auditor-scoped files: 0 of 10 sampled actionable.
- **Why it failed:** the pattern is real and common, but rarely matters. A missing postcondition
  does not change the weight. Hook and micro-benchmarks have nothing to assert. Assertions inside
  the measured block were missed.
- **Catch it instead:** benchmark tests (`cargo test --features runtime-benchmarks`) already run every
  benchmark. Ask for a `verify` in review only when the benchmark could silently take the cheap path.

### BEN003: extrinsic without a benchmark
- **Tried:** flag dispatchables that no benchmark measures.
- **Evidence:** external 6 of 12, run audit 9 of 15. IC: 0 of 12 worth fixing, 9 wrong.
  Auditor bugs: three related hits, none for the right reason.
- **Why it failed:** weights reach a benchmark in indirect ways: weight helper functions, reused
  `WeightInfo` functions, closures built by setup helpers, metered step budgets. Syntax cannot follow
  all of them. The rest were mostly root-only calls, where a hand-written weight is a fair choice.
- **Catch it instead:** weight review: every signed call's `#[pallet::weight]` should name a generated
  `WeightInfo` function. RTC001/RTC002 (kept) catch the runtime-level form: placeholder or missing weights.
- **Worth rebuilding?** Same as BEN001: a narrow, precise version might pay off once benchmark
  discovery is solved; the syntax-only version could not do that.

## Tests and mocks

### TST001: prefer `assert_noop!`
- **Tried:** flag `assert!(x.is_err())` followed by `unwrap_err()` in tests.
- **Evidence:** external 0 of 12, 7 wrong.
- **Why it failed:** many results were transaction-validity or XCM errors, not dispatch results, so
  `assert_noop!` does not apply. The rest needed the error value later. It is a style rule.
- **Catch it instead:** code review and team style guide.

### TST004: missing `Pays::Yes` error-path test
- **Tried:** when a call refunds fees with `Pays::No`, require a test that the error path pays.
- **Evidence:** external 6 of 7, then run audit 0 of 15 (2 wrong). In auditor-scoped files: 0 of 10 sampled actionable.
- **Why it failed:** errors returned with `?` already pay by default. No SDK pallet writes this test.
  The rule only checked that some test file contains two words. Two findings were not even extrinsics.
- **Catch it instead:** unit tests where a refund is conditional. Review the refund condition, not the test file.

### TST005: assertions on implementation details
- **Tried:** flag test assertions on fields named `inner`, `state`, `cache`, or starting with `_`.
- **Evidence:** 10 of 10 sampled findings were wrong on 10 parachain repositories (October audit).
- **Why it failed:** a field name does not tell you whether it is private. It flagged public state and `.inner()` results.
- **Catch it instead:** test review.

### TST006: storage-mutating extrinsic without an event
- **Tried:** flag dispatchables that write storage but call no `deposit_event`.
- **Evidence:** external 5 of 12, run audit 17 of 23 under the loose label. IC: 0 of 8 worth fixing, 1 wrong.
- **Why it failed:** events are often emitted by a helper the call invokes. Most remaining hits were
  root-only single-value setters, already visible through Sudo, Scheduler or Referenda events.
  A missing event is real but rarely worth a change.
- **Catch it instead:** review checklist ("does the UI or indexer need to see this?") and
  `System::assert_last_event` in unit tests.

### MOK001: excessive mock setup
- **Tried:** flag tests with a high ratio of mock lines to assertions.
- **Evidence:** 10 of 10 sampled findings were wrong (October audit). In auditor-scoped files: 1 of 1 not actionable.
- **Why it failed:** it counted test-harness calls as mock setup. A ratio is not a defect.
- **Catch it instead:** test review.

## Panics and arithmetic

### SEC002: `debug_assert!` reachable from runtime entry points
- **Tried:** flag `debug_assert!` on paths reached from calls and hooks.
- **Evidence:** external 1 of 12.
- **Why it failed:** nearly all were deliberate precondition checks in math helpers. They are
  compiled out of release builds by design. The one real case was `debug_assert!(false)` in an error branch.
- **Catch it instead:** use FRAME's `defensive!` for "should never happen" branches. It logs in
  release and panics in tests. Review any `debug_assert!(false)`.

### SEC008: `unwrap`/`expect` in production runtime code
- **Tried:** flag `unwrap()` and `expect()` reachable from runtime entry points.
- **Evidence:** external 0 of 12, 8 wrong. 275 findings on 8 runtimes. Two auditor bugs of this kind
  could not be evaluated because the code did not build.
- **Why it failed:** a safe unwrap and an unsafe one look the same. Safety comes from a proof
  ("qed"), a constant input, or genesis-only code. The rule fired on genesis presets and `qed` code it claimed to skip.
- **Catch it instead:** fuzz the runtime with `substrate-runtime-fuzzer`; run `try-runtime`.
  Teams that want a hard ban can enable `clippy::unwrap_used` and `clippy::expect_used` per pallet crate.
  Prefer `defensive_unwrap_or` and similar helpers.

### SEC009: raw integer arithmetic in fallible functions
- **Tried:** flag `+ - * / %` on integers inside functions that return `Result`.
- **Evidence:** external 2 of 12, 7 wrong. Raw arithmetic is a real, recurring auditor class, but this
  rule caught none of it. The cases sat outside its scope (non-integer operands, functions that do not
  return `Result`) or in code that did not build.
- **Why it failed:** small bounded sums (`len + 8`, `cursor + args * 32`) look exactly like risky ones.
  Whether a value can overflow depends on ranges set at runtime or by economics.
- **Catch it instead:** opt-in `clippy::arithmetic_side_effects` on math-heavy pallets;
  `overflow-checks = true` in the release profile; property tests and fuzzing on the math crate.

### VAL002: division by an unproven divisor
- **Tried:** flag division by a config value, a storage value or a length.
- **Evidence:** run audit 0 of 10; in auditor-scoped files 0 of 2 actionable. It caught no auditor bug.
- **Why it failed:** every hit divided by a `Get` config constant. A zero there is a misconfiguration,
  not a value an attacker controls.
- **Catch it instead:** assert that config constants are non-zero in the pallet's `integrity_test` hook.
  `clippy::integer_division` exists if a team wants a strict ban.

## Storage, migrations and decoding

### SEC001: unbounded `Vec` in extrinsic parameters
- **Tried:** flag call parameters of type `Vec` with no length check.
- **Evidence:** external 1 of 4, 3 wrong.
- **Why it failed:** the wrong ones were gated by a privileged origin that only the runtime config
  shows. Signed callers already pay a per-byte length fee for their input.
- **Catch it instead:** prefer `BoundedVec` in call parameters. Benchmark with a length component
  (`Linear<0, MAX>`) when the body loops over the input.

### SEC003: decoding recursive types without a depth limit
- **Tried:** taint caller input and flag `decode` of `RuntimeCall` or extrinsics without a depth limit.
- **Evidence:** external 0 of 10: 6 wrong, 4 unclear. Unbounded decode depth is a real auditor class,
  but the rule missed the one case it could analyse and could not build the other.
- **Why it failed:** it read Ethereum ABI decoding as SCALE decoding. In the real bugs, bytes were
  stored by one call and decoded later, so taint from call parameters never reached them.
- **Catch it instead:** review checklist: every `RuntimeCall::decode` uses `decode_with_depth_limit`.
  Fuzz with `substrate-runtime-fuzzer`, which builds nested calls.

### SEC013: unbounded collections in storage
- **Tried:** flag storage values that contain `Vec` or `BTreeMap`.
- **Evidence:** run audit 0 of 15. No findings in auditor-scoped files.
- **Why it failed:** all 50 findings were in pallets marked `#[pallet::without_storage_info]`. FRAME
  already rejects values without `MaxEncodedLen` everywhere else. The rule only restated a deliberate opt-out.
- **Catch it instead:** FRAME's own check. In review, ask why any new `without_storage_info` or `#[pallet::unbounded]` is needed.

### SEC014: `Identity` hasher on common key types
- **Tried:** flag `Identity` hashers on `u32`, `u64`, `Balance` or `BlockNumber` keys.
- **Evidence:** run audit 0 of 6. No findings in auditor-scoped files.
- **Why it failed:** the risk exists only when an attacker chooses the key. Every key found was an
  internal sequential index. The key's type does not tell you who chooses it.
- **Catch it instead:** review checklist: `Identity` only for keys the runtime assigns.

### SEC016: migration writes storage without a version check
- **Tried:** flag `on_runtime_upgrade` bodies that write storage but never read `StorageVersion`.
- **Evidence:** external 0 of 9.
- **Why it failed:** teams guard re-runs in other ways: a done-flag, a state check, or a write that
  is safe to repeat. The rule could not tell these from a missing guard.
- **Catch it instead:** write migrations as `UncheckedOnRuntimeUpgrade` wrapped in `VersionedMigration`.
  Run `try-runtime on-runtime-upgrade`, which also checks that a migration is safe to run twice.

### SEC017: unbounded `Vec` in pallet events
- **Tried:** flag event fields of type `Vec`.
- **Evidence:** external 3 of 8, run audit 0 of 15.
- **Why it failed:** every sampled `Vec` was bounded elsewhere: a config maximum, a `BoundedVec`
  input, a validator set. A third came from a demo pallet.
- **Catch it instead:** review: an event field is as bounded as the data it copies.

## Weights

### SEC005: expensive operations in weight expressions
- **Tried:** flag storage reads, `get_dispatch_info` and encode/decode inside `#[pallet::weight]`.
- **Evidence:** external 0 of 8.
- **Why it failed:** every hit was `call.get_dispatch_info()`, the standard way wrapper calls
  (utility, proxy, sudo) add the inner call's weight.
- **Catch it instead:** review: weight expressions should not read storage. Benchmarks show the cost.

### SEC018: weight ignores unbounded input
- **Tried:** flag calls with a `Vec` parameter whose weight does not use its length.
- **Evidence:** external 1 of 1, run audit 2 of 15, 2 wrong.
- **Why it failed:** the per-byte length fee already prices the input. The real risk is O(n) work in
  the body, which the rule never checked. It also missed bounds checked in the body.
- **Catch it instead:** benchmark loops with a component and use it in the weight. Review any loop over a call parameter.

### SEM005: parameterless weight function times a count
- **Tried:** flag `WeightInfo::f().saturating_mul(n)`.
- **Evidence:** external 1 of 12.
- **Why it failed:** per-item weight times a count is a valid linear over-estimate. Some "counts"
  were constants or fee multipliers, not components.
- **Catch it instead:** weight review; benchmark with `Linear` components when the cost is non-linear.

### SEM006: `DbWeight` estimates ignore proof size
- **Tried:** flag `T::DbWeight::get().reads(..)/writes(..)` outside generated weight files.
- **Evidence:** external 3 of 12. In auditor-scoped files, 3 of 10 sampled actionable.
- **Why it failed:** most hits sat beside an explicit proof-size budget, in hook accounting, or in template
  code. Whether the missing proof size matters depends on how much the call reads.
- **Catch it instead:** benchmarks, which measure proof size. In review, reject hand-written `DbWeight`
  weights on production calls.

## Validation order

### VAL001: heavy storage reads before cheap validation
- **Tried:** flag a storage read on a line before an `ensure!` in the same function.
- **Evidence:** external 2 of 5, 3 wrong; an earlier audit found about 83% wrong.
- **Why it failed:** it matched text. The "cheap" guard often read storage too, or the read chose a
  branch that returned early. The gain is small: the caller pays the declared weight either way.
- **Catch it instead:** review, for calls where early rejection saves real work.

### VAL003: storage writes before validation
- **Tried:** flag a storage write followed by a `?` or `ensure!` in the same function.
- **Evidence:** external 1 of 12.
- **Why it failed:** FRAME runs every dispatchable in a storage layer. If the call returns an error,
  its writes are rolled back. The rule restated a hazard FRAME already removes.
- **Catch it instead:** the risk remains only outside dispatchables (hooks, helpers called from hooks).
  Unit-test the error paths of hooks. Use `with_storage_layer` or `#[transactional]` there.

## Discarded results

### SEC007: `let _ =` discards a `Result`
- **Tried:** flag `let _ = expr;` when `expr` is a `Result` with a real error type.
- **Evidence:** external 5 of 12, run audit 2 of 15. In auditor-scoped files: 0 of 2 actionable.
- **Why it failed:** `let _ =` is already an explicit choice, often with a comment. It also skipped
  closure bodies, and FRAME wraps every dispatchable in a closure, so it never saw dispatchable code.
  The real hits were one narrow shape: a discarded `Result` holding funds.
- **Catch it instead:** the compiler's `#[must_use]` warning catches silent discards. SEC024 (kept, off
  by default) covers the funds-holding shape. `clippy::let_underscore_must_use` exists for a strict ban.

## Deprecations and enums

### SEM008: deprecated `sp_std` paths
- **Tried:** flag every `sp_std::` path.
- **Evidence:** external 0 of 12; 1,699 findings, 58% of all output at the time. In auditor-scoped files, 0 of 10 actionable.
- **Why it failed:** it is a dependency cleanup, not a defect. Many hits were in generated weight files.
- **Catch it instead:** remove `sp-std` from `Cargo.toml`. The compiler then lists every use. Fix generated
  files in the benchmark template.

### SEM013: custom invalidity enum without `#[repr(u8)]`
- **Tried:** flag `*Invalidity` enums without `#[repr(u8)]`, because they are cast to a `u8` error code.
- **Evidence:** IC 0 of 2 worth fixing. No findings on any other corpus.
- **Why it failed:** with explicit, in-range discriminants the attribute changes nothing. It fired anyway.
- **Catch it instead:** `clippy::cast_enum_truncation` catches the real failure, a value that does not
  fit in a `u8`. Review: pin discriminants on enums that leave the runtime.

## Style conventions

These six encoded Parity SDK house style. They were turned off by default in October 2026 and are
now deleted. On the 10 parachain repositories behind `benchmarks/precision/2026-11-external`, each rule
had 10 sampled findings and 0 were worth fixing: code outside the SDK does not follow the SDK's
conventions, and none of them is a defect. Style is better enforced by clippy or a team style guide
than by a bug-finding linter.

### SEM002: `collect()` without a turbofish
- **Tried:** flag `let xs: Vec<_> = iter.collect();` and ask for `iter.collect::<Vec<_>>()`.
- **Evidence:** 0 of 10 worth fixing.
- **Why it failed:** both forms compile to the same code. It is a preference, not a bug.
- **Catch it instead:** team style guide.

### SEM003: `.iter()` in `for` loops
- **Tried:** flag `for x in v.iter()` and ask for `for x in &v`.
- **Evidence:** 0 of 10 worth fixing.
- **Why it failed:** both forms desugar to the same code.
- **Catch it instead:** `clippy::explicit_iter_loop` (pedantic) for teams that want it.

### SEM004: wildcard imports
- **Tried:** flag `use module::*;` outside tests, with exemptions such as `use super::*`.
- **Evidence:** 0 of 10 worth fixing.
- **Why it failed:** glob imports are common and deliberate in FRAME code (preludes, `pallet_prelude::*`).
  The cost is readability, not correctness.
- **Catch it instead:** `clippy::wildcard_imports` (pedantic), which has its own allow-list for preludes.

### SEM007: deprecated `RuntimeDebug`
- **Tried:** flag `#[derive(RuntimeDebug)]` and suggest `Debug`.
- **Evidence:** 10 of 10 sampled findings were wrong.
- **Why it failed:** `RuntimeDebug` is deprecated only from `sp-debug-derive` 15.0.0. Every flagged
  repository locked 14.0.0, where it is not deprecated, so the advice was wrong for them.
- **Catch it instead:** the compiler's deprecation warning, once a repository moves to a release that
  deprecates it.

### SEM012: `#[allow(dead_code)]` in pallet code
- **Tried:** flag `allow(dead_code)` outside tests and generated weight files.
- **Evidence:** 0 of 10 worth fixing.
- **Why it failed:** the attribute is already a visible, deliberate choice. Whether the code should go
  is a judgment the linter cannot make.
- **Catch it instead:** reviewers.

### TST003: imports inside test bodies
- **Tried:** flag `use` items inside test closures and nested blocks.
- **Evidence:** 0 of 10 worth fixing.
- **Why it failed:** import placement in tests is a style choice with no effect on what the test checks.
- **Catch it instead:** team style guide.

## What worked

The rules that kept finding real bugs check one concrete fact that a reviewer can confirm in seconds.
- **Runtime configuration (RTC001-RTC008).** Placeholder weights (`()`, `TestWeightInfo`), free XCM
  delivery prices, zero deposits, insecure randomness, a deny barrier not applied to nested XCM, and
  proxy filters that fail open. They caught most of the auditor bugs that any rule caught. The 3 IC findings worth fixing are all RTC.
- **RTC009, known-vulnerable crate versions.** It reads `Cargo.lock` and compares versions to a list of
  real Polkadot incidents. A version is a fact; there is nothing to guess.
- **A few incident-based compiler rules** (for example SEC012 unbounded `clear_prefix`, SEC020
  discarded verification verdict, SEC021 inverted guard, SEM019 verifier defaulting to `true`). Each copies
  the exact shape of a known bug, so a hit is rarely a coincidence.
Rules failed when the answer depended on things outside the code: who controls an input, how large it
can get, whether a check happens elsewhere, or whether a fix is worth the cost. Those are for review,
benchmarks, fuzzing and `try-runtime`, not a linter.
