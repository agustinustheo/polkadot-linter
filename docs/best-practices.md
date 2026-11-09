# Best-practice coverage

BlockDeep's [Libro](https://libro.blockdeep.dev/) lists Polkadot SDK best
practices. This table shows the practices that a rule checks. Most practices
need human review, because they depend on chain policy, economics or design. A
clean lint run does not prove that you follow them.

| Practice | Rules | Coverage |
| --- | --- | --- |
| [Avoid unbounded iteration](https://libro.blockdeep.dev/critical/Avoid_Unbounded_Iteration.html) | SEC011, SEC012 | Partial |
| [Use benchmarking for accurate dynamic weights](https://libro.blockdeep.dev/critical/Use_Benchmarking_for_Accurate_Dynamic_Weights.html) | SEM011, RTC001, RTC002, SEC004 | Partial |
| [Avoid redundant storage access in mutations](https://libro.blockdeep.dev/high/Avoid_Redundant_Storage_Access_in_Mutations.html) | SEM009 | Enforced |
| [Distribute finalization costs](https://libro.blockdeep.dev/high/Prevent_Inconsistent_State_By_Distributing_Finalization_Costs.html) | SEC011, SEC012 | Partial |
| [Use atomic operations](https://libro.blockdeep.dev/high/Use_Atomic_Operations_To_Prevent_State_Inconsistencies.html) | SEC010 | Enforced |
| [Use safe arithmetic operations](https://libro.blockdeep.dev/high/Use_Safe_Arithmetic_Operations.html) | SEM010, SEC004 | Partial |
| [Avoid resource-intensive execution inside hooks](https://libro.blockdeep.dev/medium/Avoid_Resource_Intensive_Execution_Inside_Hooks.html) | SEC010, SEC011, SEC012 | Enforced |
| [Implement tests for all error cases](https://libro.blockdeep.dev/medium/Implement_Tests_For_All_Error_Cases.html) | TST002 | Partial |
| [Use appropriate naming conventions](https://libro.blockdeep.dev/low/Use_Appropriate_Naming_Conventions.html) | TRM001 | Partial |
| [Avoid typographical errors](https://libro.blockdeep.dev/informational/Avoid_Typographical_Errors.html) | TRM001 | Enforced |
| [Use proper naming criteria](https://libro.blockdeep.dev/informational/Use_Proper_Naming_Criteria.html) | TRM001 | Partial |

"Enforced" means a default rule reports the main form of the problem.
"Partial" means a rule checks one part, or the rule is off by default (SEC004).

No rule covers the other Libro practices. Examples: origin checks, input
validation, `unwrap` in the runtime, XCM barriers and fee management,
`try_state` hooks and documentation.
