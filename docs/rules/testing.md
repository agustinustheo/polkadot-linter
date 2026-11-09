# Test Quality Rules (TST)

Test-quality rules flag test-suite patterns that weaken confidence, such as assertions that hide dispatch errors.

| Rule | Title | Default severity | Engine |
| --- | --- | --- | --- |
| [`TST002`](TST002.md) | assert_ok! on apply_extrinsic Hides Dispatch Errors | Error | source (syn) |

Retired rules of this family are listed in [What we tried and removed](../retired-rules.md).
