# Test rules (TST)

TST rules find tests that can pass when the code under test fails.

| Rule | Finds | Severity |
| --- | --- | --- |
| [TST002](TST002.md) | `assert_ok!` on `apply_extrinsic` hides dispatch errors | error |

Set one severity for all TST rules with `severity` under `[test_smells]`.
