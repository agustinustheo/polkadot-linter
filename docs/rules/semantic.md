# Semantic rules (SEM)

SEM rules find code that compiles but does not do what it seems to do. Examples:
a zero weight, an operator with a different meaning, or a verifier that accepts
all input.

| Rule | Finds | Severity | Engine |
| --- | --- | --- | --- |
| [SEM009](SEM009.md) | Redundant `contains_key` before `remove` or `take` | advisory | compiler |
| [SEM010](SEM010.md) | `^` used as a power (it is XOR) | error | compiler |
| [SEM011](SEM011.md) | Zero weight on a call | warning | syntax |
| [SEM014](SEM014.md) | Missing log target near `SubmitTransaction` | advisory | syntax |
| [SEM015](SEM015.md) | Missing `weight_of_authorize` | warning | syntax |
| [SEM016](SEM016.md) | Missing `AuthorizeCall` | warning | compiler |
| [SEM019](SEM019.md) | Verifier trait method defaults to `true` | warning | syntax |
