# Rule Backend Matrix

Each public rule has one diagnostic authority. A rule is moved to the rustc
driver only when compiler resolution or control-flow analysis improves its
evidence; duplicate `syn` and rustc descriptors are not allowed.

## Rustc Driver

| Rules | Evidence used |
| --- | --- |
| `SEC001` - `SEC018` | Resolved paths and types, cfg/macro-expanded HIR, reachable bodies, and rule-specific FRAME dataflow. |
| `VAL003` | Resolved FRAME storage writes followed by a fallible validation edge. |

The syntax engine does not register these rules. Their old parser
implementations are retained only as test fixtures while focused rustc
regressions and pinned SDK baselines validate the public implementation.

## Syntax Authority

| Rules | Reason |
| --- | --- |
| `SEM002`, `SEM003`, `SEM004`, `SEM007`, `SEM008`, `SEM010`, `SEM012`, `SEM013` | Source style, import, derive, attribute, or representation conventions. Resolved types do not add decision-quality evidence. |
| `SEM005`, `SEM006`, `SEM009`, `SEM011`, `SEM014`, `SEM015`, `SEM016` | FRAME source conventions that are currently source-attribute or local-style checks. They remain syntax-backed until a compiler model can improve the finding contract and has a benchmark. |
| `VAL001` | Requires a cost model that distinguishes resolved storage reads from cheap operations and proves whether a later guard is data-dependent on that read. |
| `VAL002` | Requires denominator provenance through configuration/storage reads plus path-sensitive zero-proof and cross-function integrity-test evidence. |
| `TST*`, `MOK*`, `BEN*`, `TRM*` | Test quality, mock/benchmark conventions, and text terminology are inherently source- or text-oriented. |

## Migration Gate

Before moving a syntax rule, the rustc implementation must have focused
positive and negative tests, the syntax registration must be removed, and a
reproducible project or SDK benchmark must demonstrate that the compiler
evidence improves precision or coverage.
