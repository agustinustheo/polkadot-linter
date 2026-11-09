# Retired rules

We wrote 64 rules. We measured each rule on real projects and judged each
finding by hand. A finding counts only if an engineer would fix it at a
reasonable cost. The 33 rules below failed this test, so we deleted them. The
table tells you how to catch each problem instead.

All 33 rules were deleted in 0.2.0
([#36](https://github.com/agustinustheo/polkadot-linter/pull/36)). Rules
marked † also had an old syntax copy. That copy was deleted earlier, in
[#30](https://github.com/agustinustheo/polkadot-linter/pull/30). Per-rule
numbers are in
[`benchmarks/precision/retired-rules-evidence.tsv`](https://github.com/agustinustheo/polkadot-linter/blob/main/benchmarks/precision/retired-rules-evidence.tsv).

| Rule | What it checked | Why we removed it | Catch it instead |
| --- | --- | --- | --- |
| BEN001 | A weight function with no benchmark | It could not find benchmarks in companion crates or helpers | RTC002; `impl_benchmark_test_suite!` tests in CI |
| BEN002 | A benchmark with no `verify` block | Real, but almost never worth a fix (IC: 0 of 9) | Benchmark tests (`cargo test --features runtime-benchmarks`) |
| BEN003 | A call that no benchmark measures | It could not follow weight helpers (IC: 9 of 12 wrong) | RTC001, RTC002; manual code review of `#[pallet::weight]` |
| TST001 | `assert!(x.is_err())` in place of `assert_noop!` | Style only; many results were not dispatch errors | Team style guide |
| TST003 | `use` inside a test body | Style only (0 of 10 worth a fix) | Team style guide |
| TST004 | No test that the error path pays fees | Errors already pay by default (0 of 15) | Unit tests; manual code review |
| TST005 | Asserts on fields such as `inner` or `state` | A field name does not show if it is private (10 of 10 wrong) | Manual code review |
| TST006 | A call that writes storage but emits no event | Events come from helpers; seldom worth a fix (IC: 0 of 8) | `System::assert_last_event` in unit tests; manual code review |
| MOK001 | Too much mock setup in a test | It counted harness calls as mocks (10 of 10 wrong) | Manual code review |
| SEC001 † | An unbounded `Vec` call parameter | Privileged origins and length fees already limit it | `BoundedVec`; benchmarks with a length component |
| SEC002 † | `debug_assert!` on runtime paths | Almost all were correct precondition checks | FRAME `defensive!`; manual code review |
| SEC003 † | Decoding of recursive types with no depth limit | It read ABI decoding as SCALE; it missed stored bytes | Fuzzing (`substrate-runtime-fuzzer`); manual or LLM-assisted review for `decode_with_depth_limit` |
| SEC005 † | Expensive work in a weight expression | Every hit was the correct `get_dispatch_info()` | Benchmarks; manual code review |
| SEC007 † | `let _ =` drops a `Result` | `let _ =` is already a deliberate choice | Compiler `unused_must_use` warning for bare discards; `clippy::let_underscore_must_use`; SEC024 |
| SEC008 † | `unwrap`/`expect` in runtime code | A safe unwrap and an unsafe one look the same (0 of 12) | `clippy::unwrap_used`, `clippy::expect_used`; fuzzing; try-runtime |
| SEC009 † | Raw `+ - * / %` in fallible functions | Small bounded sums look like risky ones (2 of 12) | `clippy::arithmetic_side_effects`; `overflow-checks = true`; fuzzing |
| SEC013 † | `Vec` or `BTreeMap` in storage | All hits were deliberate `without_storage_info` pallets | FRAME `MaxEncodedLen` check; manual code review |
| SEC014 † | `Identity` hasher on number keys | All keys were internal indexes, not user input | Manual code review |
| SEC016 † | A migration with no storage-version check | Teams guard re-runs in other ways (0 of 9) | `VersionedMigration`; `try-runtime on-runtime-upgrade` |
| SEC017 † | A `Vec` in an event | Every `Vec` was bounded somewhere else | Manual code review |
| SEC018 † | A weight that ignores input length | Length fees already price input; it missed loop cost | Benchmarks with a length component |
| SEM002 | `collect()` with no turbofish | Style only (0 of 10) | Team style guide |
| SEM003 | `for x in v.iter()` | Style only (0 of 10) | `clippy::explicit_iter_loop` |
| SEM004 | `use module::*;` | Normal in FRAME preludes (0 of 10) | `clippy::wildcard_imports` |
| SEM005 | `WeightInfo::f().saturating_mul(n)` | A valid linear upper estimate (1 of 12) | Benchmarks with `Linear` components |
| SEM006 † | `DbWeight` estimates with no proof size | Usually next to an explicit proof-size budget (3 of 12) | Benchmarks; manual code review |
| SEM007 | `#[derive(RuntimeDebug)]` | Not deprecated in the versions teams use (10 of 10 wrong) | Compiler deprecation warning |
| SEM008 | `sp_std::` paths | A dependency cleanup, not a bug (0 of 12; 58% of all output) | Remove `sp-std`; compiler errors list each use |
| SEM012 | `#[allow(dead_code)]` in pallets | Already a visible, deliberate choice (0 of 10) | Manual code review |
| SEM013 | An `*Invalidity` enum with no `#[repr(u8)]` | No effect with explicit discriminants (IC: 0 of 2) | `clippy::cast_enum_truncation` |
| VAL001 | A storage read before a cheap `ensure!` | Text match; small gain (about 83% wrong) | Manual code review |
| VAL002 † | Division by a value that can be zero | Every hit divided by a config constant (0 of 10) | `integrity_test` asserts that constants are not zero |
| VAL003 † | A storage write before validation | FRAME rolls back a failed call's writes (1 of 12) | `with_storage_layer` or `#[transactional]` in hooks; unit tests |

"IC" is the acceptance test on `paritytech/individuality-community`. Other
numbers are useful findings out of sampled findings, from the
[external and run audits](measurement.md).

## Notes by theme

**Benchmarks (BEN).** Benchmarks are in many places: the pallet, a companion
crate, the runtime, a helper closure. Syntax cannot find all of them. A smaller
rule could work if it reads the compiler's list of benchmarks.

**Tests and mocks (TST, MOK).** These rules checked test style or counted
words. None found a test that hides a bug. TST002 is the test rule we kept.

**Panics and arithmetic (SEC002, SEC008, SEC009, VAL002).** A safe `unwrap` or
sum looks the same as an unsafe one. Safety depends on a proof, a range or an
economic limit. Use clippy lints per crate, fuzzing and try-runtime.

**Storage, migrations and decoding (SEC001, SEC003, SEC013, SEC014, SEC016,
SEC017).** The risk depends on who controls the input. The code does not show
this. Unbounded decode depth is a real bug class. Review each
`RuntimeCall::decode` for `decode_with_depth_limit`, and fuzz nested calls.

**Weights (SEC005, SEC018, SEM005, SEM006).** Benchmarks measure weight and
proof size better than a static rule. Review loops over call parameters.

**Validation order (VAL001, VAL003).** FRAME runs each call in a storage layer.
A failed call rolls back its writes. The risk is only in hooks.

**Discarded results (SEC007).** The real cases were one shape: a dropped
`Result` that holds funds. SEC024 checks that shape.

**Style (SEM002, SEM003, SEM004, SEM007, SEM012, TST003).** These rules
followed Parity SDK house style. On 10 parachain repositories, 0 of 10 sampled
findings per rule were worth a fix. Style belongs in clippy or a team style
guide.

## What worked

The rules we kept check one concrete fact. A reviewer can confirm it in
seconds. Examples: runtime configuration (RTC), known vulnerable versions
(RTC009), and compiler rules that copy a known bug (SEC012, SEC019-SEC023,
SEM019). Rules failed when the answer depended on facts outside the code: who
controls an input, how large it can get, or if a fix is worth the cost. Use
review, benchmarks, fuzzing and try-runtime for those questions.
