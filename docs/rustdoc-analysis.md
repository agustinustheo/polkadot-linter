# rustdoc-backed Analysis

The first typed-analysis backend consumes rustdoc JSON emitted by `rustdoc`.
This keeps the default linter fast and dependency-light while giving selected
rules access to compiler-resolved item and type structure.

## Why rustdoc JSON

The current scanner is `syn`-based. It is fast and useful for local patterns,
but it cannot see macro-expanded FRAME items, resolved paths, trait
implementations, or type aliases. rustdoc JSON is produced by the compiler
pipeline, so it gives the linter a reproducible bridge to resolved item/type
data without embedding `rustc_private` in the main binary.

This is an intermediate Phase 2 backend. Rules that need full body-level HIR or
MIR data may still need a Clippy-style driver later.

## Current Prototype

`SEC013` has a rustdoc-backed prototype. It inspects resolved type aliases and
reports FRAME storage aliases whose value argument contains an unbounded
collection such as `Vec` or `BTreeMap`.

The prototype intentionally skips:

- bounded wrappers such as `BoundedVec`, `WeakBoundedVec`, and
  `BoundedBTreeMap`
- aliases carrying `#[pallet::unbounded]`
- docs that explicitly describe a capacity-limited value

The existing `syn` rule remains unchanged. The rustdoc path only runs when
`--rustdoc-json` is supplied.

## Usage

Generate rustdoc JSON for a crate with the pinned nightly toolchain:

```bash
cargo +nightly-2025-06-10 rustdoc \
  --manifest-path path/to/Cargo.toml \
  --lib \
  -- -Z unstable-options --output-format json
```

Then run the linter with the generated JSON:

```bash
cargo +1.93.0 run -- path/to/source \
  --rules SEC013 \
  --rustdoc-json path/to/target/doc/crate_name.json \
  --rustdoc-source-root path/to/source
```

## Migration Plan

Keep the fast scanner for rules that are mostly syntactic and already stable.
Move rules when typed evidence can remove meaningful ambiguity:

| Rule | Target backend | Required evidence |
| --- | --- | --- |
| `SEC013` | rustdoc JSON first | resolved storage aliases and value type arguments |
| `SEC003` | rustc/Clippy | decoded type, recursive structure, input taint |
| `SEC009` | rustc/Clippy | operator type, trait resolution, bounds dataflow |
| `SEC008`/`SEC002` | rustc/Clippy | cfg expansion, reachability, panic/debug assertion context |
| `SEC018` | rustc/Clippy plus FRAME model | dispatchable params, weight expression dataflow, helper calls |

Each migrated rule needs:

- focused unit tests with near misses
- corpus benchmarks against the pinned SDK checkout
- a normalized diff proving fewer false positives or stronger evidence with no
  unexpected new diagnostics
- CI coverage for the backend entry point

## Compatibility

The default CLI behavior does not change. `--rustdoc-json` is opt-in and can be
rolled out per rule. This lets CI keep the current benchmark stable while adding
typed benchmarks gradually.
