# Rule Authority Matrix

The public CLI uses rustc-backed analysis as the final authority for every rule
listed as compiler-backed in src/rules/mod.rs. A source rule remains public
only when the compiler cannot yet preserve or improve the evidence required by
the rule.

| Rule | Current authority | Evidence | Next enabling work |
| --- | --- | --- | --- |
| VAL001 | rustc bridge | Resolved FRAME storage reads and authored dispatchable bodies; pinned nomination-pools baseline | None |
| TST006 | rustc bridge | Resolved storage writes and authored event calls; pinned timestamp baseline | None |
| BEN001 | rustc bridge | Resolved WeightInfo calls from authored weight expressions and compiled benchmark identities; pinned assets baseline | None |
| BEN003 | rustc bridge | Compiler-confirmed dispatchables and compiled benchmark identities; pinned assets baseline | None |
| BEN002 | source | The authored verify block and its position outside the measured block are consumed by the FRAME benchmark macro. HIR can prove compilation but not this authored verification contract. | Compiler-to-authored benchmark block mapping with macro provenance |
| TST001 | source | The requirement to prefer assert_noop includes an authored assertion style and storage-rollback intent that macro-expanded HIR does not retain. | Macro-call source mapping plus rollback-effect analysis |
| TST002 | rustc bridge | Rustc resolves the called function and nested Result type; a source-span bridge recovers the authored assert_ok macro location. The pinned parachain-system test target validates that a source candidate with a non-nested result emits no finding. | None |
| TST007 | source | Whether a test made a meaningful observable assertion after a successful dispatch is an authored test-design convention. Exact macro and helper-call matching is stronger than expanded HIR. | Test-target compilation plus assertion-effect classification |
| TST003 | source | Import placement is lexical source style; HIR does not preserve the intended source-scope convention better than the parser. | None; a compiler port would be weaker |
| TST004 | source | The rule compares separate tests and their semantic intent, including a missing companion error-path test. | Test-target compilation plus cross-test behavioral classification |
| TST005 | source | Whether an assertion targets an implementation detail is a test-design convention, not a resolved-type property. | None; a compiler port would be weaker |
| MOK001 | source | Mock/setup-to-assertion ratio is a test-design heuristic based on authored structure. | None; a compiler port would be weaker |
| SEM017 | source | `#[pallet::getter]` is an authored FRAME attribute consumed during expansion; exact parser matching is the strongest evidence. | None; a compiler port would lose the attribute |
| SEM018 | source | The rule is intentionally limited to authored primitive parameter annotations and immediate explicit conversions. Rustc resolves the destination type but cannot improve the API-design judgment. | Benchmark evidence that resolved type aliases identify a materially stronger signal |
| DOC001 | source | Rustdoc presence on a public FRAME item or variant is an authored source contract that macro expansion does not preserve consistently. | None; a compiler port would lose documentation placement |
| SEC019 | rustc | Exact external crate identity and resolved function identity for `rand::random`, limited to reachable runtime and pallet source paths. | Broaden only with a benchmarked catalogue of unsafe randomness APIs |
| SEC020 | rustc bridge | Resolved `frame_system::ensure_signed_or_root` identity and resolved FRAME storage writes, constrained by an authored discarded or unused-result statement. The pinned referenda baseline proves index-keyed signed-or-root calls remain silent. | None |

Each future migration must add a focused fixture, a pinned polkadot-sdk case,
and evidence that the compiler implementation improves precision or validated
coverage before the source implementation is removed.
