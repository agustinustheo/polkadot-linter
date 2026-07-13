# Rule Backend Matrix

Each public rule has one diagnostic authority. A rule is moved to the rustc
driver only when compiler resolution or control-flow analysis improves its
evidence; duplicate `syn` and rustc descriptors are not allowed.

## Rustc Driver

| Rules | Evidence used |
| --- | --- |
| `SEC001` - `SEC018` | Resolved paths and types, cfg/macro-expanded HIR, reachable bodies, and rule-specific FRAME dataflow. |
| `VAL003` | Resolved FRAME storage writes followed by a fallible validation edge. |
| `SEM006` | Type-checked `RuntimeDbWeight` reads/writes calls outside generated weights files. |
| `SEM009` | Resolved FRAME storage calls, identical storage owner paths, and identical local key bindings. |
| `SEM010` | Type-checked integer XOR expressions with suspicious decimal-base literal operands. |
| `SEM016` | Resolved `frame_system::offchain::CreateAuthorizedTransaction` implementations and resolved `frame_system::AuthorizeCall::new()` construction. |

The syntax engine does not register these rules. Their old parser
implementations are retained only as test fixtures while focused rustc
regressions and pinned SDK baselines validate the public implementation.

## Syntax Authority

| Rule | Pinned FRAME corpus | Source-authority evidence |
| --- | ---: | --- |
| `SEM002` | 194 | The rule requires the spelling distinction between a typed `let` binding and `.collect::<Vec<_>>()`; HIR type inference does not improve that convention check. |
| `SEM003` | 59 | The rule prefers `for value in &collection` over `.iter()` syntax. Trait resolution can prove iteration works, but cannot improve this source-style decision. |
| `SEM004` | 77 | The rule checks explicit import-tree form, visibility, and cfg-gated source modules. Resolved names discard the intended import structure. |
| `SEM005` | 2 | The rule checks a source-level weight convention: a zero-argument `WeightInfo` call multiplied by a scale factor instead of a parameterized benchmark. Resolved call identity does not prove a better weight contract. |
| `SEM007` | 0 | The deprecated `RuntimeDebug` derive/path spelling and its replacement are source conventions; resolved trait identity adds no decision-quality evidence. |
| `SEM008` | 54 | The deprecated `sp_std` import spelling is a source compatibility convention. Resolution cannot improve the diagnostic beyond the exact import path. |
| `SEM011` | 0 | A zero-weight placeholder in a FRAME weight attribute is a lexical attribute-expression convention; macro expansion loses the authoring form. |
| `SEM012` | 8 | `#[allow(dead_code)]` is an attribute-policy check. Compiler reachability does not decide whether a production suppression is acceptable. |
| `SEM013` | 0 | The custom-invalidity enum name plus `#[repr(u8)]` declaration is a representation-policy check; type layout resolution adds no evidence beyond the source attributes. |
| `SEM014` | 0 | The rule associates a nearby `SubmitTransaction` call with a lexical logging target. Compiler resolution cannot establish the intended operational log convention. |
| `SEM015` | 0 | The rule requires paired `#[pallet::authorize]` and `#[pallet::weight_of_authorize]` source attributes; FRAME macro expansion consumes the relation the rule checks. |
| `VAL001` | 10 | Source authority masks cfg-gated test/try-runtime/integrity items, requires predicate calls to occur in a fallible `ensure!`-style guard, and stops tracking a read once its local binding is consumed. The full corpus dropped from 52 to 10 findings. A rustc prototype resolved direct FRAME reads and local uses, correctly suppressing identity ownership cases and retaining the bounties candidate, but missed `BondedPool::<T>::get` before an independent guard in `pallet-nomination-pools`; it must not replace source until aliases such as that resolve. |
| `VAL002` | 10 | Requires denominator provenance through configuration/storage reads plus path-sensitive zero-proof and cross-function integrity-test evidence. A prototype resolved direct `Get` calls and local guards but missed the generic `Period: Get<u32>` division in pinned `pallet-collective`; it must not replace source until that case is covered. |
| `TST001` - `TST006` | 2, 5, 65, 84, 4, 79 | Test assertion, placement, and observability conventions require source-level intent. rustc can type-check tests but cannot establish that a test validates the required behavior. |
| `MOK001` | 4 | The setup-to-assertion ratio and mock-heavy test smell are source-structure conventions with no useful compiler semantic fact. |
| `BEN001` - `BEN003` | 1,013, 236, 203 | Benchmark naming, verification placement, and benchmark-to-dispatchable coverage are source/test-convention checks; runtime compiler analysis cannot prove benchmark adequacy. |
| `TRM001` | 0 | Terminology spelling is inherently textual. |

## Migration Gate

Before moving a syntax rule, the rustc implementation must have focused
positive and negative tests, the syntax registration must be removed, and a
reproducible project or SDK benchmark must demonstrate that the compiler
evidence improves precision or coverage.

`scripts/check-source-rule-corpus.sh` checks the full pinned FRAME source-rule
corpus against `benchmarks/polkadot-sdk-source-frame-baseline.tsv`. It is a
regression baseline, not a claim that every source finding is audit-validated;
the per-rule migration gate above remains the authority decision.
