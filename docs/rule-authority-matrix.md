# Rule Authority Matrix

This matrix is derived from the public rule registry in
`src/rules/mod.rs`. It describes what the released CLI actually runs, rather
than planned or experimental rules.

Authority is separate from default enablement: `config/default.toml` turns off
`RTC004`, `SEC004` and `SEC024` on precision and review evidence
(`benchmarks/precision/2026-11-external`,
`benchmarks/precision/2026-11-run-audit`,
`benchmarks/precision/2026-11-individuality-community`). Rules that were mostly
wrong or not worth fixing were deleted; see
[What we tried and removed](retired-rules.md). The pinned-SDK
checks below run with `config/all-rules.toml`, so every rule stays covered.

## Rustc-backed authority

The public CLI routes these rules through the compiler driver whenever it
scans a compilable Cargo project. The parser implementations with matching
security IDs are retired and are not registered in the public rule set.

| Rules | Evidence used |
| --- | --- |
| `SEM009`, `SEM010`, `SEM016` | Resolved paths, operators, storage methods, and trait implementations. |
| `SEC004`, `SEC006`, `SEC010`-`SEC012`, `SEC015`, `SEC019`-`SEC024` | Resolved types and paths, macro/cfg-expanded code, trait/operator resolution, and control-flow/dataflow evidence as required by each rule. |

The focused driver checks in `scripts/check-rustc-*.sh` cover the released
compiler path. `scripts/check-sdk-benchmarks.sh` maintains pinned SDK
baselines for every compiler-backed rule: reviewed findings for the security
rules, plus zero-finding false-positive controls for the compiler-backed SEM
rules.

For source-authoritative FRAME attributes, the CLI applies active `cfg_attr`
values from Cargo metadata using the same default, `--no-default-features`, and
`--features` selection passed to compiler-backed analysis. Custom cfg values
that Cargo metadata cannot establish remain unresolved rather than guessed.

## Source authority

These rules remain parser/source-based because their current contract depends
on authored layout, comments, spelling, or FRAME attributes that macro expansion does not preserve. A rustc implementation is
not an upgrade unless it can provide stronger benchmarked evidence; it must
not replace a more accurate source implementation merely for uniformity.

| Rules | Why source remains authoritative | Next enabling work, if any |
| --- | --- | --- |
| `SEM011`, `SEM014`, `SEM015`, `SEM019` | These are authored FRAME-attribute, logging and trait-default conventions. | Only migrate where a compiler implementation demonstrates improved precision. |
| `TST002` | Test assertion shape is a source-level convention. | Test-target compilation plus source-span/effect analysis. |
| `RTC001`-`RTC008`, `RTC010` | Runtime configuration is authored type bindings, `parameter_types!` bodies, and `construct_runtime!`/`define_benchmarks!` macro input; the evidence is the authored binding, and macro expansion erases `define_benchmarks!` membership. Precision is pinned by `scripts/check-source-runtime-corpus.sh` and `benchmarks/labels/RTC.tsv`. | Type resolution of aliased fee managers, waived locations, delivery prices and origins across files (same-file `type` aliases are resolved). |
| `RTC009` | The evidence is the resolved version in `Cargo.lock`, checked against a table of published releases; no source or compiler analysis is involved. | None. |
| `TRM001` | The rule intentionally evaluates spelling and terminology in comments, strings, and identifiers. | None; a compiler port would lose the primary evidence. |

## Scope boundaries

The current binary has no `DOC001`, `SEM017`, `SEM018`, or `TST007` rule. Those identifiers must not be described as enforced until
they are implemented, tested, registered, and benchmarked. A clean run only
means the implemented evidence was absent; it does not replace runtime review,
benchmark review, or whole-program security analysis.

Before moving any source rule to rustc, add all of the following:

1. A precise public rule contract and a source-vs-driver precision comparison.
2. Focused positive and negative regression cases that exercise the released
   CLI path.
3. A reproducible pinned-SDK benchmark showing better true-positive coverage
   or fewer false positives.
4. Removal or demotion of the source implementation only after the driver is
   demonstrably the stronger final authority.
