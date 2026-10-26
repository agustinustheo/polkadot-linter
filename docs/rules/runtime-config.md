# Runtime Configuration Rules (RTC)

Runtime configuration rules inspect the *runtime crate* rather than individual pallets: `impl <pallet>::Config for Runtime` blocks, `construct_runtime!` / `#[frame_support::runtime]` declarations, `define_benchmarks!`, `parameter_types!` and the XCM executor configuration. Every rule encodes a misconfiguration that Security Research Labs reported in at least one public audit of a Polkadot SDK runtime ([srlabs/audit-reports](https://github.com/srlabs/audit-reports)).

| Rule | Title | Default severity | Engine |
| --- | --- | --- | --- |
| [`RTC001`](RTC001.md) | Placeholder WeightInfo in a Runtime Config | Warning | source (syn) |
| [`RTC002`](RTC002.md) | Pallet Missing from define_benchmarks! | Warning | source (syn) |
| [`RTC003`](RTC003.md) | XCM Delivery Fees Burned or Waived | Error | source (syn) |
| [`RTC004`](RTC004.md) | Zero Existential or Storage Deposit | Warning | source (syn) |
| [`RTC005`](RTC005.md) | Insecure Collective-Flip Randomness in the Runtime | Warning | source (syn) |

## Which code is inspected

A file is inspected only when all of the following hold:

1. **It belongs to a runtime crate.** The nearest `Cargo.toml` above the file must declare a `[package]` whose `src/lib.rs` contains `construct_runtime!`, a `#[frame_support::runtime]` module, or `#[frame_construct_runtime]`. Files under a virtual workspace manifest only (no package) are orphans and are skipped. Files that have no `Cargo.toml` above them at all fall back to checking the file itself for a runtime declaration.
2. **It is not test code.** `#[cfg(test)]`/`#[test]` items and items gated on the `runtime-benchmarks`, `try-runtime`, or `test-helpers` features are removed before analysis. Paths with a `mock`, `mocks`, `test`, `tests`, or `benches` component, or a component containing `mock`, `-test`, `test-`, `_test`, are skipped, as are benchmark files.
3. **It is not a test runtime.** A runtime whose package name or crate directory contains `test` or `mock` (`polkadot-test-runtime`, `cumulus-test-runtime`, ...) is always skipped.
4. **It is not a dev runtime, unless you opt in.** Packages listed in `[runtime_config].dev_runtime_packages` are skipped unless `include_dev_runtimes = true`. The default list is `kitchensink-runtime`, `penpal-runtime`, `rococo-parachain-runtime`, `yet-another-parachain-runtime`, `pallet-staking-async-parachain-runtime` and `pallet-staking-async-rc-runtime`: runtimes that exist to exercise node, emulator, or pallet features and that deliberately use placeholder weights, zero deposits, and free XCM delivery.

Everything else is treated as production-intended. That deliberately includes the SDK **templates**: SR Labs' OpenZeppelin audits show that template misconfigurations become production misconfigurations when teams fork them. It also includes the Westend/Rococo relay chains and the system parachains, which are live test networks.

## Configuration

```toml
[rules.severity]
RTC003 = "error"

[runtime_config]
severity = "warning"
include_dev_runtimes = false
dev_runtime_packages = ["kitchensink-runtime", "penpal-runtime"]
unit_weight_info_allowlist = ["pallet_babe", "pallet_beefy", "pallet_grandpa"]
flag_substrate_weight = false
substrate_weight_severity = "advisory"
```

## Pinned SDK corpus

`scripts/check-source-runtime-corpus.sh` scans `polkadot/runtime`, `cumulus/parachains/runtimes`, `templates`, `substrate/bin/node/runtime`, `cumulus/test` and `polkadot/xcm` at the pinned SDK commit, compares per-root counts with `benchmarks/polkadot-sdk-source-runtime-baseline.tsv`, and checks every TP/FP label in `benchmarks/labels/RTC.tsv`. Every reported finding must be labeled.
