# Historical rustdoc Prototype

This document records the original rustdoc JSON prototype. It is not a public
CLI backend: `SEC013` and every other public SEC rule are now implemented by
the `rustc_driver` pipeline documented in
[`rustc-driver-analysis.md`](rustc-driver-analysis.md).

## Why rustdoc JSON

The original scanner was `syn`-based. It is fast and useful for local patterns,
but it cannot see macro-expanded FRAME items, resolved paths, trait
implementations, or type aliases. rustdoc JSON is produced by the compiler
pipeline, so it gives the linter a reproducible bridge to resolved item/type
data without embedding `rustc_private` in the main binary.

The production implementation uses the driver because the security rules need
full body-level HIR and type information.

## Historical Prototype

`SEC013` originally had a rustdoc-backed prototype. It inspected resolved type
aliases and reported FRAME storage aliases whose value argument contained an
unbounded collection such as `Vec` or `BTreeMap`.

The active compiler-backed migration work now lives in the rustc-driver path.
The rustdoc code remains testable background material only; it does not emit
public diagnostics and must not be reintroduced as a second SEC013 authority.

The prototype intentionally skips:

- bounded wrappers such as `BoundedVec`, `WeakBoundedVec`, and
  `BoundedBTreeMap`
- aliases carrying `#[pallet::unbounded]`
- docs that explicitly describe a capacity-limited value

The rustc driver supersedes both the former `syn` rule and this prototype.

## Current Status

The `--rustdoc-json` and `--rustdoc-source-root` CLI options were removed when
SEC013 moved to the rustc driver. Regression tests for the parser are retained
as research coverage, while the CLI and CI use only rustc-backed SEC013 output.
