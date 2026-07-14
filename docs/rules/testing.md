# Test Quality Rules (TST)

Test-quality rules flag test-suite patterns that weaken confidence: assertions that hide dispatch errors, missing error-path coverage, and assertions coupled to implementation details.

| Rule | Title | Default severity | Engine |
| --- | --- | --- | --- |
| [`TST001`](TST001.md) | Prefer assert_noop! for Dispatch Error Assertions | Warning | source (syn) |
| [`TST002`](TST002.md) | assert_ok! on apply_extrinsic Hides Dispatch Errors | Error | source (syn) |
| [`TST003`](TST003.md) | Imports Inside Test Closures and Function Bodies | Advisory | source (syn) |
| [`TST004`](TST004.md) | Missing Pays::Yes Error-Path Test for Fee-Refunding Extrinsics | Advisory | source (syn) |
| [`TST005`](TST005.md) | Assertions on Implementation Details | Advisory | source (syn) |
| [`TST006`](TST006.md) | Storage-Mutating Extrinsic Without an Event | Advisory | source (syn) |
