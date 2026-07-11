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
  resolved function parameter types and reports public callables whose input
  resolves to `Vec<T>`, including type aliases that are invisible to the
  syntax-only rule.
- `SEC002`: debug assertions in production code. The rustc-backed
  implementation identifies `debug_assert!` through macro expansion ancestry,
  so cfg-disabled source that never reaches expanded HIR is not reported.
- `SEC003`: unsafe recursive decode calls. The rustc-backed implementation
  reads resolved call return types and decode receiver types, so aliases to
  `RuntimeCall`, `UncheckedExtrinsic`, or `OpaqueExtrinsic` are handled by type
  resolution instead of source text matching.
- `SEC008`: panic-capable unwrap/expect calls. The rustc-backed implementation
  reads the resolved receiver type and skips `Result<T, Infallible>` unwraps,
  where the error path is statically uninhabited.
- `SEC009`: raw arithmetic in fallible functions. The rustc-backed
  implementation reads HIR and type-checking results, then reports binary `+`,
  `-`, `*`, `/`, and `%` only when both operands resolve to integer types inside
  a function returning `Result`.
- `SEC011`: storage iteration in callable paths. The rustc-backed
  implementation resolves the owner type of associated `iter()`/`drain()` calls
  and reports only known FRAME storage collection owners such as `StorageMap`,
  `StorageDoubleMap`, `StorageNMap`, `CountedStorageMap`, and `StorageValue`.
- `SEC012`: unbounded `clear_prefix`. The rustc-backed implementation resolves
  the owner type of associated `clear_prefix` calls and reports unbounded limits
  such as `None` and `Some(u32::MAX)` only when the owner is a FRAME storage
  collection.
- `SEC013`: unbounded storage aliases. The rustc-backed implementation reads
  `#[pallet::storage]` type aliases through HIR attributes and resolved alias
  types, so storage values whose payload is hidden behind a `Vec<T>` alias are
  reported while bounded wrappers are skipped.
- `SEC017`: unbounded event payloads. The rustc-backed implementation visits
  event-like enums and reads resolved field types, so aliases to `Vec<T>` are
  reported while bounded wrappers such as `BoundedVec` are skipped.
- `SEC018`: missing weight accounting for unbounded inputs. The rustc-backed
  implementation reads `#[pallet::weight(...)]` attributes plus resolved
  function parameter types, so aliased `Vec<T>` inputs are reported when the
  weight expression does not reference the parameter length or encoded size.

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

The stable `polkadot-linter` CLI can now invoke the compiler-backed path for a
Cargo package and emit normal linter diagnostics:

```sh
polkadot-linter \
  --no-syntax \
  --format json \
  --compiler-backed-rules SEC001,SEC008 \
  --rustc-cargo-manifest .repos/polkadot-sdk/Cargo.toml \
  --rustc-package pallet-multisig \
  --rustc-lib \
  --rustc-no-default-features \
  --rustc-driver target/debug/polkadot-linter-rustc \
  --rustc-toolchain nightly-2025-06-10 \
  --rustc-source-filter substrate/frame/multisig/src/lib.rs
```

Internally, `polkadot-linter` runs Cargo with `polkadot-linter-rustc` as
`RUSTC_WORKSPACE_WRAPPER`, parses the driver's JSONL output, and converts it
back into the public diagnostic format. In wrapper mode, Cargo passes the real
rustc path as the first argument; the driver preserves that invocation,
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
`polkadot-linter` CLI with `--rustc-cargo-manifest`, and verifies
package-local compiler-backed findings are captured from the pinned SDK
`pallet-multisig` package. The raw smoke artifact is filtered to the
`substrate/frame/multisig/src/lib.rs` package file and currently contains 10
deduplicated public linter diagnostics from the explicitly selected `SEC001`
and `SEC008` rustc-backed rules. The summary is checked against
`benchmarks/polkadot-sdk-rustc-multisig-sec001-sec008-baseline.tsv`. This is
the first end-to-end SDK Cargo integration baseline through the public CLI for
the rustc-backed pipeline; it is not yet the final SDK-scale benchmark proof
for every hard rule.

The CI workflow runs both scripts after the default stable build.
