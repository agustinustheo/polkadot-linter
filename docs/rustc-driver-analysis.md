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

## SEC009 typed arithmetic check

The first migrated driver check is `SEC009` for raw arithmetic in fallible
functions. The rustc-backed implementation reads HIR and type-checking results,
then reports binary `+`, `-`, `*`, `/`, and `%` only when both operands resolve
to integer types inside a function returning `Result`.

This removes syntax-level false positives for overloaded arithmetic, because
operator syntax alone is no longer treated as integer arithmetic.

Run the reproducible precision check with:

```sh
scripts/check-rustc-sec009.sh
```

That fixture intentionally compares the existing syntax rule against the typed
driver:

- syntax path reports both integer arithmetic and overloaded `Add`
- rustc-driver path reports only integer arithmetic

The CI workflow runs this script after the default stable build.
