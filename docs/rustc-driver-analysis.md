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

This removes syntax-level false negatives for aliased unbounded inputs and
aliased recursive decode targets, plus syntax-level false positives for
cfg-disabled debug assertions, type-provably infallible unwraps, and overloaded
arithmetic. Source spelling alone is no longer the authority for these checks.

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

The CI workflow runs this script after the default stable build.
