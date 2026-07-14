# Installation

`polkadot-linter` is a Cargo project. Build the release binaries from a checkout:

```bash
git clone https://github.com/agustinustheo/polkadot-linter
cd polkadot-linter
cargo build --release
```

This produces two binaries:

- `polkadot-linter` — the CLI you run against your code.
- `polkadot-linter-driver` — the rustc wrapper used by the compiler-backed
  pipeline. The CLI expects it at `target/debug/polkadot-linter-driver` by
  default; override with `--driver-path` (see the
  [Command-Line Reference](cli-reference.md)).

Throughout this guide, commands are written as `cargo run -- …` so they work from
a source checkout. If you have installed the binary onto your `PATH`, drop the
`cargo run --` prefix and call `polkadot-linter …` directly.

Next: [Quick Start](quick-start.md).
