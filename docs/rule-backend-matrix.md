# Rule Backend Matrix

Each public rule has one diagnostic authority. A rule is moved to the rustc
driver only when compiler resolution or control-flow analysis improves its
evidence; duplicate `syn` and rustc descriptors are not allowed.

## Rustc Driver

| Rules | Evidence used |
| --- | --- |
| `VAL002` | Resolved `Get::get` calls through generic and associated projections, resolved collection-length receivers, local aliases/casts, and path-sensitive nonzero or nonempty proofs. `scripts/check-rustc-sdk-val002.sh` pins nine active FRAME findings and excludes the guarded offchain-worker division. |
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
| `VAL001` | 4 | Source authority masks cfg-gated test/try-runtime/integrity items, derives local bindings from the full AST pattern across multiline reads, avoids substring aliases, excludes reads that select a guarded `if` branch, and stops tracking a read once its binding is consumed. The full corpus dropped from 52 to 4 findings. A rustc prototype resolved direct FRAME reads and local uses, but missed `BondedPool::<T>::get` before an independent guard in `pallet-nomination-pools`. A pinned `pallet-nomination-pools` compiler probe also found no named `chill` function body after FRAME expansion, so the current driver cannot associate that authored storage read and guard. It must not replace source until that expansion mapping and storage aliases resolve. |
| `TST001` - `TST005` | 2, 5, 65, 84, 4 | Test assertion, placement, and observability conventions require source-level intent. rustc can type-check tests but cannot establish that a test validates the required behavior. |
| `TST006` | 79 | A rustc prototype correctly identified `pallet-babe::plan_config_change` as a dispatchable, but FRAME expansion mapped its HIR body to `#[frame_support::pallet]` rather than the authored body. The resolved HIR therefore exposed neither `PendingEpochConfigChange::<T>::put` nor direct event calls. That misses known source candidates, so the source implementation remains the sole authority until compiler analysis can preserve the authored dispatchable body. |
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
