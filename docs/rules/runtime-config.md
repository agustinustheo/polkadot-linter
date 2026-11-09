# Runtime configuration rules (RTC)

RTC rules check the runtime crate: `impl <pallet>::Config for Runtime`,
`construct_runtime!`, `define_benchmarks!`, `parameter_types!` and the XCM
config. Each rule comes from a public audit
([SR Labs](https://github.com/srlabs/audit-reports)) or a fix in polkadot-sdk or
polkadot-fellows/runtimes. All RTC rules use the syntax engine.

| Rule | Finds | Severity |
| --- | --- | --- |
| [RTC001](RTC001.md) | Placeholder `WeightInfo` in a runtime | warning |
| [RTC003](RTC003.md) | XCM delivery is free | warning |
| [RTC004](RTC004.md) | Zero existential or storage deposit (off by default) | warning |
| [RTC005](RTC005.md) | Collective-flip randomness | warning |
| [RTC006](RTC006.md) | XCM fees burned | warning |
| [RTC007](RTC007.md) | XCM deny barrier misses nested programs | warning |
| [RTC008](RTC008.md) | Proxy filter fails open | warning |
| [RTC009](RTC009.md) | Vulnerable crate version in `Cargo.lock` | error |
| [RTC010](RTC010.md) | Storage-reading aliaser on the unpaid barrier | warning |

## Which code the rules check

RTC009 reads every `Cargo.lock`. The other RTC rules check a file only when all
four conditions are true:

1. **The file is in a runtime crate.** The linter starts at the library target
   of the nearest package (default `src/lib.rs`). It follows `mod` items. The
   modules it reaches must declare exactly one runtime: `construct_runtime!`
   (or a fork such as `construct_runtime_modified!`), `#[frame_support::runtime]`,
   `#[runtime]` or `#[frame_construct_runtime]`. A crate with more than one
   runtime is a test harness, so the linter skips it.
2. **The file is not test code.** The linter removes `#[cfg(test)]` and
   `#[test]` items. It also removes items behind the `runtime-benchmarks`,
   `try-runtime` and `test-helpers` features. It skips paths with a `test`,
   `tests`, `mock`, `mocks` or `benches` name token. (`mock.rs` matches.
   `testnet.rs` does not.)
3. **The runtime is not a test runtime.** The linter always skips a runtime
   whose package or directory name has a token in `test_runtime_tokens`. For
   example, it skips `polkadot-test-runtime` but not `mychain-testnet-runtime`.
4. **The runtime is not a dev runtime.** The linter skips runtimes in
   `dev_runtime_packages` and runtimes with a token in `dev_runtime_tokens`.
   For example, it skips `peaq-dev` but not `devops-runtime`. Set
   `include_dev_runtimes = true` to check them.

The linter checks all other runtimes. This includes the SDK templates, because
teams copy them into production. It also includes Westend, Rococo and the
system parachains.

## Configuration

These are the defaults:

```toml
[runtime_config]
# severity = "warning"   # sets one severity for all RTC rules
include_dev_runtimes = false
dev_runtime_packages = [
    "kitchensink-runtime", "pallet-staking-async-parachain-runtime",
    "pallet-staking-async-rc-runtime", "penpal-runtime",
    "rococo-parachain-runtime", "yet-another-parachain-runtime",
]
dev_runtime_tokens = ["dev", "development"]
test_runtime_tokens = ["test", "tests", "mock", "mocks"]  # [] checks all runtimes
```

Add your own dev and testnet runtimes (for example `moonbase-runtime`) to
`dev_runtime_packages`.
