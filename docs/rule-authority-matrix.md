# Rule Authority Matrix

This matrix is derived from the public rule registry in
`src/rules/mod.rs`. It describes what the released CLI actually runs, rather
than planned or experimental rules.

## Rustc-backed authority

The public CLI routes these rules through the compiler driver whenever it
scans a compilable Cargo project. The parser implementations with matching
security IDs are retired and are not registered in the public rule set.

| Rules | Evidence used |
| --- | --- |
| `VAL002`, `VAL003` | Resolved expressions, control flow, and storage-write evidence. |
| `SEM006`, `SEM009`, `SEM010`, `SEM016` | Resolved paths, operators, storage methods, and trait implementations. |
| `SEC001`-`SEC018` | Resolved types and paths, macro/cfg-expanded code, trait/operator resolution, and control-flow/dataflow evidence as required by each rule. |

The focused driver checks in `scripts/check-rustc-*.sh` cover the released
compiler path. `scripts/check-sdk-benchmarks.sh` maintains pinned SDK
baselines for the hard security rules and `VAL002`; additional rule-specific
SDK benchmark coverage remains tracked as migration work.

## Source authority

These rules remain parser/source-based because their current contract depends
on authored layout, comments, spelling, benchmark structure, or FRAME
attributes that macro expansion does not preserve. A rustc implementation is
not an upgrade unless it can provide stronger benchmarked evidence; it must
not replace a more accurate source implementation merely for uniformity.

| Rules | Why source remains authoritative | Next enabling work, if any |
| --- | --- | --- |
| `VAL001` | Validation ordering is reported over the authored dispatchable body; FRAME expansion can hide the source ordering that the diagnostic explains. | Compiler-to-pre-expansion source mapping with dataflow evidence. |
| `SEM002`-`SEM005`, `SEM007`, `SEM008`, `SEM011`-`SEM015` | These are authored style, lexical import, or FRAME-attribute conventions. | Only migrate where a compiler implementation demonstrates improved precision. |
| `TST001`-`TST006`, `MOK001` | Test assertion style, test design, and mock/setup ratios are source-level conventions. | Test-target compilation plus source-span/effect analysis for a specific rule. |
| `BEN001`-`BEN003` | Benchmark identity and `verify` blocks must be associated with their authored macro bodies. | Compiler-to-authored FRAME benchmark mapping with macro provenance. |
| `TRM001` | The rule intentionally evaluates spelling and terminology in comments, strings, and identifiers. | None; a compiler port would lose the primary evidence. |

## Scope boundaries

The current binary has no `DOC001`, `SEC019`, `SEC020`, `SEM017`, `SEM018`,
or `TST007` rule. Those identifiers must not be described as enforced until
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
