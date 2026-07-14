# Quick Start

Scan one or more directories:

```bash
# Scan a single directory
cargo run -- ../pallets

# Scan several paths at once
cargo run -- ../pallets ../runtimes ../support

# Scan the current directory (the default when no path is given)
cargo run --
```

Point the tool at your project configuration and tighten CI behaviour:

```bash
# Use a project-level configuration file
cargo run -- -c ../polkadot-linter.toml ../pallets

# Treat warnings as failures (non-zero exit) for CI gating
cargo run -- ../pallets -s warning --fail-on-warning
```

Emit machine-readable output for tooling and code scanning:

```bash
cargo run -- ../pallets -f json
cargo run -- ../pallets -f sarif > polkadot-linter.sarif
```

Route a specific compiler-backed rule through the rustc pipeline for one Cargo
package (the manifest is discovered from the scan path):

```bash
cargo run -- \
  --rules SEC003 \
  --package pallet-xcm \
  --lib \
  ../polkadot-sdk/polkadot/xcm/pallet-xcm
```

From here, the [Command-Line Reference](cli-reference.md) documents every flag,
and [Output & Exit Codes](output-and-exit-codes.md) explains how to read and gate
on the results.
