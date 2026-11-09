# Install

## The CLI

```bash
cargo install polkadot-linter
polkadot-linter --version
```

This needs Rust 1.91 or later. The CLI runs all syntax rules. Releases are on
crates.io only. There are no prebuilt binaries.

## The compiler driver

The compiler rules (all SEC rules, SEM009, SEM010 and SEM016) need the
`polkadot-linter-driver` binary. The driver builds on a pinned nightly
toolchain:

```bash
rustup toolchain install nightly-2025-09-01 --component rustc-dev --component llvm-tools-preview
cargo +nightly-2025-09-01 install polkadot-linter --features rustc-driver
```

This installs the CLI and the driver. After `cargo install`, the linter finds the driver automatically. From a source checkout, pass `--driver-path target/debug/polkadot-linter-driver` if you built it elsewhere.

If the CLI cannot find the driver, it prints a notice. It then reports syntax
findings only.

## From a source checkout

```bash
git clone https://github.com/agustinustheo/polkadot-linter
cd polkadot-linter
cargo build --release
cargo +nightly-2025-09-01 build --features rustc-driver --bin polkadot-linter-driver
```

The last command puts the driver at `target/debug/polkadot-linter-driver`. The
CLI finds it there when you run it from the checkout. In a checkout, use
`cargo run --` in place of `polkadot-linter`.
