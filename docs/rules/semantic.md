# Semantic Rules (SEM)

Semantic rules catch correct-looking code with wrong or outdated meaning: deprecated APIs, weight placeholders, operators that don't do what they appear to do, and missing pieces of newer FRAME idioms.

| Rule | Title | Default severity | Engine |
| --- | --- | --- | --- |
| [`SEM002`](SEM002.md) | Prefer Turbofish Syntax for collect() | Advisory | source (syn) |
| [`SEM003`](SEM003.md) | Prefer Reference Iteration Over .iter() in for Loops | Advisory | source (syn) |
| [`SEM004`](SEM004.md) | Wildcard Imports in Non-Test Code | Warning | source (syn) |
| [`SEM005`](SEM005.md) | Unparameterised Weight Functions Multiplied by Component Counts | Warning | source (syn) |
| [`SEM006`](SEM006.md) | DbWeight Estimates Ignore Proof Size | Warning | compiler-backed |
| [`SEM007`](SEM007.md) | Deprecated RuntimeDebug Derive | Warning | source (syn) |
| [`SEM008`](SEM008.md) | Deprecated sp_std Paths | Warning | source (syn) |
| [`SEM009`](SEM009.md) | Redundant contains_key Before remove or take | Advisory | compiler-backed |
| [`SEM010`](SEM010.md) | XOR Operator Mistaken for Exponentiation | Error | compiler-backed |
| [`SEM011`](SEM011.md) | Weight::zero() Placeholder in Weight Attributes | Warning | source (syn) |
| [`SEM012`](SEM012.md) | Suppressed Dead Code in Production Pallet Code | Warning | source (syn) |
| [`SEM013`](SEM013.md) | Custom Invalidity Enum Without `#[repr(u8)]` | Warning | source (syn) |
| [`SEM014`](SEM014.md) | Missing Log Target in SubmitTransaction Logging | Advisory | source (syn) |
| [`SEM015`](SEM015.md) | Missing `#[pallet::weight_of_authorize]` on Authorized Calls | Warning | source (syn) |
| [`SEM016`](SEM016.md) | Missing `AuthorizeCall` in `CreateAuthorizedTransaction` Extension | Warning | compiler-backed |
