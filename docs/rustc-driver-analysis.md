# Rustc-driver analysis

`polkadot-linter-rustc` is the compiler-backed analysis entry point. It is
gated behind the `rustc-driver` feature because it depends on nightly
`rustc_private` APIs plus the `rustc-dev` and `llvm-tools-preview`
components.

## Toolchain

The supported toolchain for the current driver is:

```sh
nightly-2025-06-10
```

Install the required components with:

```sh
rustup component add rustc-dev --toolchain nightly-2025-06-10
rustup component add llvm-tools-preview --toolchain nightly-2025-06-10
```

The default syntax-based CLI still builds on the stable project toolchain. The
compiler-backed driver is built and checked separately:

```sh
cargo +nightly-2025-06-10 build --features rustc-driver --bin polkadot-linter-rustc
```

## Typed hard-rule checks

The driver currently includes typed checks for:

- `SEC001`: unbounded public inputs. The rustc-backed implementation reads
  resolved function parameter types and reports only FRAME dispatchables whose
  input resolves to `Vec<T>`, including type aliases that are invisible to the
  syntax-only rule. FRAME consumes `#[pallet::call_index]`, so dispatchable
  identity is recovered from the source span associated with the rustc-resolved
  function; public helper methods are excluded.
- `SEC002`: debug assertions in production code. The rustc-backed
  implementation identifies `debug_assert!` through macro expansion ancestry,
  so cfg-disabled source that never reaches expanded HIR is not reported. It
  limits analysis to public entry points, resolved FRAME `Hooks`,
  `OnRuntimeUpgrade`, and `UncheckedOnRuntimeUpgrade` callbacks,
  `ChangeMembers::change_members_sorted`, and their direct local callees;
  unrelated public trait implementations are not treated as runtime entry
  points. Indirect calls and path-sensitive control flow remain out of scope.
- `SEC003`: unsafe recursive decode calls. The rustc-backed implementation
  reads resolved call return types and decode receiver types, so aliases to
  `RuntimeCall`, `UncheckedExtrinsic`, or `OpaqueExtrinsic` are handled by type
  resolution instead of source text matching. It propagates input evidence from
  entry-point parameters through local bindings, match-arm bindings,
  `using_encoded(|mut bytes| ...)` closure inputs, and direct resolved local
  calls, while filtering macro-generated attribute-line spans.
- `SEC008`: panic-capable unwrap/expect calls. The rustc-backed implementation
  reads the resolved receiver type and skips `Result<T, Infallible>` unwraps,
  where the error path is statically uninhabited. It analyzes public and hook
  entry points and direct calls to local helpers rather than private helper-only
  bodies. Indirect calls and path-sensitive control flow remain out of scope.
- `SEC009`: raw arithmetic in fallible functions. The rustc-backed
  implementation reads HIR and type-checking results, then reports binary `+`,
  `-`, `*`, `/`, and `%` only when both operands resolve to integer types inside
  a function reachable from a public or hook `Result` entry point, including
  direct local helper calls. Indirect calls and path-sensitive control flow
  remain out of scope.
- `SEC011`: storage iteration in callable paths. The rustc-backed
  implementation resolves the owner type of associated `iter()`/`drain()` calls
  and reports only known FRAME storage collection owners such as `StorageMap`,
  `StorageDoubleMap`, `StorageNMap`, `CountedStorageMap`, and `StorageValue`.
  It follows public entry points, resolved FRAME hook and migration callbacks
  including `Hooks::on_runtime_upgrade`, `OnRuntimeUpgrade`, and
  `UncheckedOnRuntimeUpgrade`, and direct local helper calls. For storage
  aliases, it falls back to the resolved associated-method owner path without
  forcing projection expansion, accepting both the canonical `frame_support`
  crate path and the SDK's `frame` crate alias.
- `SEC012`: unbounded `clear_prefix`. The rustc-backed implementation resolves
  the owner type of associated `clear_prefix` calls and reports unbounded limits
  such as `None` and `Some(u32::MAX)` only when the owner is a FRAME storage
  collection reachable from a public or hook entry point, including direct
  local helper calls.
- `SEC013`: unbounded storage aliases. The rustc-backed implementation reads
  resolved storage alias types and examines the storage value generic rather
  than collection-like key generics. FRAME consumes `#[pallet::storage]`, so
  source-span recovery identifies the storage declaration while rustc provides
  the resolved owner and value type. Payloads hidden behind a `Vec<T>` alias
  are reported while bounded wrappers and unbounded keys are skipped.
- `SEC017`: unbounded event payloads. The rustc-backed implementation visits
  source-linked FRAME event enums and reads resolved field types, so aliases to
  `Vec<T>` are reported while bounded wrappers such as `BoundedVec` and
  ordinary enums are skipped. FRAME consumes `#[pallet::event]`, so the event
  marker is recovered from the enum's compiler source span.
- `SEC018`: missing weight accounting for unbounded inputs. The rustc-backed
  implementation pairs resolved function parameter types with the nearest
  source `#[pallet::weight(...)]` annotation. FRAME consumes this custom
  annotation during macro expansion, so source-span recovery is required while
  rustc remains the authority for the dispatchable and its input types. Aliased
  `Vec<T>` inputs are reported when the weight expression does not reference
  the parameter length or encoded size; deprecated compatibility dispatchables
  are excluded.

This removes syntax-level false negatives for aliased unbounded inputs and
aliased recursive decode targets, storage payloads, and event payloads, plus
syntax-level false positives for cfg-disabled debug assertions,
type-provably infallible unwraps, overloaded arithmetic, ordinary non-storage
`iter()` calls, ordinary non-storage `clear_prefix` calls, and bounded
weight-input cases. Source spelling alone is no longer the authority for these
checks.

Run the reproducible precision check with:

```sh
scripts/check-rustc-hard-rules.sh
```

That fixture intentionally compares the existing syntax rule against the typed
driver:

- for `SEC001`, the syntax path misses an aliased `Vec` dispatchable parameter,
  while the rustc-driver path resolves the alias and reports it
- for `SEC002`, the syntax path reports both cfg-disabled and active
  `debug_assert!` calls, while the rustc-driver path reports only the expanded
  active assertion
- for `SEC003`, the syntax path misses `AliasCall::decode`, while the
  rustc-driver path resolves the alias and reports it
- for `SEC008`, the syntax path reports both `Result<T, Infallible>::unwrap()`
  and a truly fallible `expect`, while the rustc-driver path reports only the
  reachable fallible path
- for `SEC009`, the syntax path reports both integer arithmetic and overloaded
  `Add`, while the rustc-driver path reports only integer arithmetic
- for `SEC011`, the syntax path reports an ordinary `Domain::iter()` call in a
  dispatchable-shaped body, while the rustc-driver path resolves owner types and
  reports only `StorageMap::iter()`
- for `SEC012`, the syntax path reports both a resolved storage
  `clear_prefix` call and an ordinary `Domain::clear_prefix` call, while the
  rustc-driver path reports only the storage call with an unbounded limit
- for `SEC013`, the syntax path misses a `#[pallet::storage]` alias whose value
  type is another alias to `Vec`, while the rustc-driver path resolves the value
  alias and reports it while skipping a bounded storage alias
- for `SEC017`, the syntax path misses an event payload behind a `Payload`
  alias, while the rustc-driver path resolves the alias and reports it while
  skipping a `BoundedVec` event payload
- for `SEC018`, the syntax path misses a weight annotation whose unaccounted
  parameter is hidden behind a `Payload` alias, while the rustc-driver path
  resolves the alias and reports it while skipping bounded inputs

The stable `polkadot-linter` CLI now treats the migrated SEC rules as
compiler-backed by default when scan paths resolve to one Cargo project. It
discovers the nearest `Cargo.toml`, uses `nightly-2025-06-10` unless overridden,
and supplies that toolchain's compiler-library directory to the rustc wrapper.
For selected migrated rules, syntax findings are demoted and removed from the
final output, then the rustc-backed diagnostics are emitted in the normal
public diagnostic format. Use `--no-rustc` only to request a legacy syntax-only
scan:

```sh
polkadot-linter \
  --format json \
  --rules SEC001,SEC008 \
  --rustc-package pallet-multisig \
  --rustc-lib \
  --rustc-no-default-features \
  --rustc-driver target/debug/polkadot-linter-rustc \
  --rustc-toolchain nightly-2025-06-10 \
  --rustc-source-filter substrate/frame/multisig/src/lib.rs \
  .repos/polkadot-sdk/substrate/frame/multisig
```

Internally, `polkadot-linter` runs Cargo with `polkadot-linter-rustc` as
`RUSTC_WORKSPACE_WRAPPER`, parses the driver's JSONL output, and converts it
back into the public diagnostic format. When `--compiler-backed-rules` is not
specified, the CLI expands the requested rule filters to the migrated
compiler-backed SEC rules (`SEC001`, `SEC002`, `SEC003`, `SEC008`, `SEC009`,
`SEC011`, `SEC012`, `SEC013`, `SEC017`, and `SEC018`). In wrapper mode, Cargo
passes the real rustc path as the first argument; the driver preserves that invocation,
continues compilation after analysis, and appends linter diagnostics to the
JSONL file named by `POLKADOT_LINTER_RUSTC_JSONL`. Diagnostics are sorted and
deduplicated before JSON/JSONL output. `POLKADOT_LINTER_RUSTC_FILE_CONTAINS`
can be set to a comma-separated list of file substrings to capture only
package-local benchmark output while Cargo still compiles dependencies
normally. `POLKADOT_LINTER_RUSTC_RULES` can be set to a comma-separated list
of rule families or IDs, such as `SEC` or `SEC008,SEC009`, so migrated rules
can be benchmarked and wired independently.

Run the SDK smoke check with:

```sh
scripts/check-rustc-sdk-smoke.sh .repos/polkadot-sdk .benchmarks
```

That script builds `polkadot-linter-rustc`, invokes the stable
`polkadot-linter` CLI without `--rustc-cargo-manifest`, `--no-syntax`, or
`--compiler-backed-rules`, and verifies package-local
compiler-backed findings are captured from the pinned SDK
`pallet-multisig` package. The raw smoke artifact is filtered to the
`substrate/frame/multisig/src/lib.rs` package file and currently contains 7
deduplicated public linter diagnostics from the explicitly selected `SEC001`
and `SEC008` rustc-backed rules. The summary is checked against
`benchmarks/polkadot-sdk-rustc-multisig-sec001-sec008-baseline.tsv`. This is
the first end-to-end SDK Cargo integration baseline through the public CLI for
the rustc-backed pipeline; it is not yet the final SDK-scale benchmark proof
for every hard rule.

Run the SDK `SEC003` coverage check with:

```sh
scripts/check-rustc-sdk-sec003.sh .repos/polkadot-sdk .benchmarks
```

That script compares the current syntax rule against the compiler-backed rule
on pinned SDK `pallet-xcm`. The syntax rule reports 0 package-local `SEC003`
findings, while the rustc-backed rule reports the associated projection decode
at `polkadot/xcm/pallet-xcm/src/lib.rs:4111` after walking the closure body and
resolving `<T as Config>::RuntimeCall`. The rustc-backed summary is checked
against `benchmarks/polkadot-sdk-rustc-pallet-xcm-sec003-baseline.tsv`, and
the public CLI run relies on default compiler-backed routing rather than
`--no-syntax`.

Run the SDK `SEC009` precision check with:

```sh
scripts/check-rustc-sdk-sec009.sh .repos/polkadot-sdk .benchmarks
```

That script compares the current syntax rule against the compiler-backed rule
on pinned SDK `pallet-collective`. The syntax rule reports 5 package-local
`SEC009` findings in `substrate/frame/collective/src/lib.rs`; the rustc-backed
rule reports 2 findings after using resolved integer operand types, ignoring
macro-generated attribute spans, and deduplicating nested arithmetic to one
finding per affected source line. The rustc-backed summary is checked against
`benchmarks/polkadot-sdk-rustc-collective-sec009-baseline.tsv`, and the public
CLI run relies on default compiler-backed routing rather than `--no-syntax`.

Run the SDK `SEC018` macro-recovery check with:

```sh
scripts/check-rustc-sdk-sec018.sh .repos/polkadot-sdk .benchmarks
```

That script checks `pallet-contracts` through the public CLI with automatic
manifest discovery. Both the syntax baseline and rustc route report the
audited `contracts::call` input at line 944, but the compiler-backed result
uses the resolved `Vec<u8>` parameter and source span attached to the
macro-expanded dispatchable. It is checked against
`benchmarks/polkadot-sdk-rustc-contracts-sec018-baseline.tsv`.

Run the SDK `SEC013` storage-value precision check with:

```sh
scripts/check-rustc-sdk-sec013.sh .repos/polkadot-sdk .benchmarks
```

The pinned `pallet-session` package provides three unbounded stored `Vec`
values. The syntax path reports an additional false positive because
`KeyOwner` has a `Vec<u8>` key; the compiler-backed rule resolves the
`StorageMap` value generic and emits only the three stored values. The output
is checked against `benchmarks/polkadot-sdk-rustc-session-sec013-baseline.tsv`.

Run the SDK `SEC017` event coverage check with:

```sh
scripts/check-rustc-sdk-sec017.sh .repos/polkadot-sdk .benchmarks
```

The syntax path finds no `SEC017` result in `pallet-root-offences`, while the
compiler-backed rule resolves the FRAME event payload and reports
`OffenceCreated::offenders` at line 116. The output is checked against
`benchmarks/polkadot-sdk-rustc-root-offences-sec017-baseline.tsv`.

Run the SDK `SEC008` panic coverage check with:

```sh
scripts/check-rustc-sdk-sec008.sh .repos/polkadot-sdk .benchmarks
```

The stabilized syntax rule emits zero `SEC008` diagnostics for
`pallet-multisig`, while the compiler-backed route resolves and reports two
reachable `.expect()` paths. The output is checked against
`benchmarks/polkadot-sdk-rustc-multisig-sec008-baseline.tsv`.

Run the SDK `SEC012` clear-prefix coverage check with:

```sh
scripts/check-rustc-sdk-sec012.sh .repos/polkadot-sdk .benchmarks
```

The syntax path emits no package-local `SEC012` result for `pallet-oracle`,
while the compiler-backed route resolves `RawValues::<T, I>::clear_prefix` and
reports the unbounded deletion limit at line 440. The output is checked against
`benchmarks/polkadot-sdk-rustc-oracle-sec012-baseline.tsv`.

The CI workflow runs the hard-rule fixture, the multisig SDK smoke baseline,
the `pallet-xcm` `SEC003` SDK coverage baseline, and the collective `SEC009`
SDK precision baseline, the multisig `SEC011` migration coverage baseline, and
the contracts `SEC018` macro-recovery baseline
and session `SEC013` storage-value and root-offences `SEC017` event baselines
and multisig `SEC008` panic and oracle `SEC012` clear-prefix baselines after
the default stable build.
