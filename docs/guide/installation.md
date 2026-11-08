# Installation

## From crates.io

```bash
cargo install polkadot-linter
polkadot-linter --version
```

This installs the `polkadot-linter` CLI (Rust 1.91+). It runs every syntax and
lockfile rule. Releases are published to crates.io only; there are no prebuilt
binaries.

## The compiler-backed driver

Compiler-backed rules (every `SEC` rule, plus `SEM009`, `SEM010` and
`SEM016`) run through `polkadot-linter-driver`, a rustc
driver built on a pinned nightly:

```bash
rustup toolchain install nightly-2025-09-01 --component rustc-dev --component llvm-tools-preview
cargo +nightly-2025-09-01 install polkadot-linter --features rustc-driver
```

This installs both binaries. The CLI looks for the driver at
`target/debug/polkadot-linter-driver` by default, so pass its installed path:

```bash
polkadot-linter --driver-path ~/.cargo/bin/polkadot-linter-driver ./runtime
```

If the driver is missing, the CLI prints a notice and emits syntax findings
only. The driver runs `cargo check` with `nightly-2025-09-01`; override it with
`--toolchain` (see the [Command-Line Reference](cli-reference.md)).

## From a checkout

```bash
git clone https://github.com/agustinustheo/polkadot-linter
cd polkadot-linter
cargo build --release
cargo +nightly-2025-09-01 build --features rustc-driver --bin polkadot-linter-driver
```

The second command puts the driver at `target/debug/polkadot-linter-driver`,
where the CLI finds it when run from the checkout.

Throughout this guide, commands are written as `cargo run -- …` so they work from
a source checkout. If you have installed the binary onto your `PATH`, drop the
`cargo run --` prefix and call `polkadot-linter …` directly.

Next: [Quick Start](quick-start.md).
