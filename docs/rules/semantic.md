# Semantic Rules (SEM)

Semantic rules catch correct-looking code with wrong meaning: weight placeholders, operators that don't do what they appear to do, verifiers that accept everything, and missing pieces of newer FRAME idioms.

| Rule | Title | Default severity | Engine |
| --- | --- | --- | --- |
| [`SEM009`](SEM009.md) | Redundant contains_key Before remove or take | Advisory | compiler-backed |
| [`SEM010`](SEM010.md) | XOR Operator Mistaken for Exponentiation | Error | compiler-backed |
| [`SEM011`](SEM011.md) | Weight::zero() Placeholder in Weight Attributes | Warning | source (syn) |
| [`SEM014`](SEM014.md) | Missing Log Target in SubmitTransaction Logging | Advisory | source (syn) |
| [`SEM015`](SEM015.md) | Missing `#[pallet::weight_of_authorize]` on Authorized Calls | Warning | source (syn) |
| [`SEM016`](SEM016.md) | Missing `AuthorizeCall` in `CreateAuthorizedTransaction` Extension | Warning | compiler-backed |
| [`SEM019`](SEM019.md) | Verifier Trait Method Defaults to `true` | Warning | source (syn) |

Retired rules of this family are listed in [What we tried and removed](../retired-rules.md).
