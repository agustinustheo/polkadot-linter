# Polkadot SDK Best Practices

This section reproduces BlockDeep's **Libro — Polkadot SDK Best Practices**
guide and annotates each practice with a **Linter coverage** note describing what,
if anything, `polkadot-linter` enforces for it.

> **Credit and license.** The best-practice text, code examples, and severity
> classifications in this section are the work of
> [BlockDeep](https://github.com/blockdeep) and are reproduced from the
> [Libro book](https://libro.blockdeep.dev/) ([source](https://github.com/blockdeep/libro))
> under the **Apache-2.0** license. Only the "Linter coverage" callouts and this
> overview page are added by the polkadot-linter project. If you want the
> canonical guide with the maintainers' full context, read it upstream at
> <https://libro.blockdeep.dev/>.

## Why this section exists

Libro explains *what good FRAME and runtime code looks like and why*.
`polkadot-linter` mechanically checks a subset of those recommendations. Pairing
the two makes the boundary explicit: which recommendations the tool can prove are
violated, which it can partially flag, and which remain human review. A clean
lint run means only that the enforced evidence is absent — it does not satisfy
the review gates.

## Reading the coverage labels

Every practice page carries one callout, just under its severity, with one of
three labels:

- **Enforced** — a shipping rule reports a strong subset of this practice by
  default. Violations of that subset will show up in a normal run.
- **Partial** — a rule checks a concrete, high-signal subset, but reviewer
  judgment is still required to confirm the practice is fully met.
- **Review gate** — no shipping rule covers this. It depends on chain policy,
  runtime configuration, economic assumptions, or whole-program evidence that
  static analysis cannot establish reliably. Some pages point to a *related*
  rule that touches an adjacent concern; a related rule is never a substitute.

Rule codes in the callouts link to the corresponding page in the
[Rules Reference](../README.md). See the
[Rule Authority Matrix](../rule-authority-matrix.md) for which engine
(compiler-backed rustc vs. source) is authoritative for each rule.

## Coverage at a glance

### Critical

| Practice | Coverage | Rules |
| --- | --- | --- |
| [Use appropriate origin checks](critical/Use_Appropriate_Origin_Checks.md) | Review gate | — (SEC015 related) |
| [Avoid unbounded iteration](critical/Avoid_Unbounded_Iteration.md) | Partial | SEC011, SEC012 |
| [Validate input parameters](critical/Unchecked_Input_Parameters.md) | Partial | VAL001–003, SEC001, SEC003, SEC018 |
| [Avoid `unwrap` inside the runtime](critical/Avoid_Unwrap_Usage_Inside_Runtime.md) | Enforced | SEC002, SEC008 |
| [Benchmark dynamic weights](critical/Use_Benchmarking_for_Accurate_Dynamic_Weights.md) | Enforced | BEN001, BEN003, SEM005, SEM006, SEM011, SEC004, SEC005, SEC018 |
| [Prefer reserve transfers over teleports](critical/Prioritize_Reserve_Asset_Transfer_Over_Teleport.md) | Review gate | — |

### High

| Practice | Coverage | Rules |
| --- | --- | --- |
| [Avoid redundant storage access in mutations](high/Avoid_Redundant_Storage_Access_in_Mutations.md) | Enforced | SEM009 |
| [Avoid pseudo-random numbers](high/Avoid_the_Usage_of_Pseudo_Random_Numbers.md) | Review gate | — |
| [Be careful with storage growth](high/Be_Careful_With_Storage_Growth.md) | Enforced | SEC001, SEC013, SEC014, SEC017, SEC018 |
| [Benchmark worst-case extrinsics](high/Benchmark_Extrinsic_Worst_Case_Scenario.md) | Partial | BEN001–003, SEC018 |
| [Consistent asset registration](high/Ensure_Consistent_Asset_Registration_by_Adhering_to_Host_Chain_Schema.md) | Review gate | — |
| [Implement a `try_state` hook](high/Implement_Try_State_Hook.md) | Review gate | — |
| [Keep dependencies up to date](high/Keep_Dependencies_Up_To_Date.md) | Review gate | — |
| [Make proper use of XCM junctions](high/Make_Proper_Usage_of_XCM_Junctions.md) | Review gate | — |
| [Distribute finalization costs](high/Prevent_Inconsistent_State_By_Distributing_Finalization_Costs.md) | Partial | SEC011, SEC012 |
| [Set up XCM barriers correctly](high/Properly_Setup_XCM_Barrier.md) | Review gate | — |
| [Use atomic operations](high/Use_Atomic_Operations_To_Prevent_State_Inconsistencies.md) | Enforced | SEC010, VAL003 |
| [Use safe arithmetic](high/Use_Safe_Arithmetic_Operations.md) | Enforced | SEC004, SEC009, VAL002, SEM010 |

### Medium

| Practice | Coverage | Rules |
| --- | --- | --- |
| [Append entries efficiently](medium/Append_Entries_Efficiently.md) | Review gate | — |
| [Avoid hardcoded parameters and values](medium/Avoid_Hardcoded_Parameters_and_Values.md) | Partial | SEC001, SEC013, SEC018 |
| [Avoid redundant data structures](medium/Avoid_Redundant_Data_Structures.md) | Review gate | — |
| [Avoid resource-intensive execution in hooks](medium/Avoid_Resource_Intensive_Execution_Inside_Hooks.md) | Enforced | SEC010, SEC011, SEC012 |
| [Break down complex functions](medium/Break_Down_Complex_Functions.md) | Review gate | — |
| [Define constants to replace magic numbers](medium/Define_Constants_to_Replace_Magic_Numbers.md) | Review gate | — |
| [Enhance logging in migration scripts](medium/Enhance_Logging_in_Migration_Scripts.md) | Review gate | — (SEM014 related) |
| [Efficient data structures](medium/Enhance_Performance_with_Efficient_Data_Structures.md) | Review gate | — |
| [Interface segregation](medium/Implement_Proper_Interface_Segregation.md) | Review gate | — |
| [XCM fee management](medium/Implement_Proper_XCM_Fee_Management.md) | Review gate | — |
| [Test all error cases](medium/Implement_Tests_For_All_Error_Cases.md) | Partial | TST001, TST002, TST004, TST006 |
| [Include error documentation](medium/Include_Error_Documentation.md) | Review gate | — |
| [Include extrinsic documentation](medium/Include_Extrinsic_Documentation.md) | Review gate | — |
| [Include tests for edge cases](medium/Include_Tests_for_Edge_Cases.md) | Partial | TST004, TST005, MOK001 |
| [Make `BoundedVec` size configurable](medium/Make_BoundedVec_Size_Configurable.md) | Partial | SEC001, SEC013, SEC018 |
| [Modularize large files](medium/Modularize_Large_Files.md) | Review gate | — |
| [Provide event documentation](medium/Provide_Event_Documentation.md) | Review gate | — |
| [Provide pallet configuration documentation](medium/Provide_Pallet_Configuration_Documentation.md) | Review gate | — |
| [Remove deprecated storage getters](medium/Remove_Deprecated_Storage_Getters.md) | Review gate | — |
| [Transition away from the `Currency` trait](medium/Transition_Away_from_Currency_Trait.md) | Review gate | — |

### Low

| Practice | Coverage | Rules |
| --- | --- | --- |
| [Adopt enums for optional input](low/Adopt_Enums_for_Optional_Input.md) | Review gate | — |
| [Avoid hardcoded error messages](low/Avoid_Hardcoded_Error_Messages.md) | Review gate | — |
| [Avoid repetitive generic instantiation](low/Avoid_Repetitive_Generic_Type_Instantiation.md) | Review gate | — (SEM002 related) |
| [Avoid unnecessary cloning](low/Avoid_Unnecessary_Cloning.md) | Review gate | — (SEM003 related) |
| [Expose runtime APIs](low/Expose_Runtime_APIs_For_Key_Functionalities.md) | Review gate | — |
| [Implement descriptive logging](low/Implement_Descriptive_Logging.md) | Review gate | — (SEM014 related) |
| [Remove unnecessary return values](low/Remove_Unnecessary_Return_Values.md) | Review gate | — (SEC007 related) |
| [Remove unused code](low/Remove_Unused_Code.md) | Enforced | SEM012 |
| [Update benchmarks with latest syntax](low/Update_Benchmarks_With_Latest_Syntax.md) | Partial | BEN002 |
| [Use appropriate naming conventions](low/Use_Appropriate_Naming_Conventions.md) | Partial | TRM001 |

### Informational

| Practice | Coverage | Rules |
| --- | --- | --- |
| [Avoid typographical errors](informational/Avoid_Typographical_Errors.md) | Enforced | TRM001 |
| [Maintain consistent documentation standards](informational/Maintain_Consistent_Documentation_Standards.md) | Review gate | — |
| [Make backend logic frontend-agnostic](informational/Make_Backend_Logic_Frontend_Agnostic.md) | Review gate | — |
| [Use proper naming criteria](informational/Use_Proper_Naming_Criteria.md) | Partial | TRM001 |

## Using this guide

Run the linter alongside formatting, Clippy, dependency and advisory checks,
benchmark review, and runtime integration tests. The enforced and partial checks
catch the mechanical mistakes; the review gates are where human judgment about
chain policy, economics, and design still has to happen.
