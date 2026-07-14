# CI Integration

`polkadot-linter` complements standard Rust tooling rather than replacing it. A
typical CI order is:

```text
cargo fmt -> cargo clippy -> polkadot-linter
```

A minimal gating step that fails the build on warnings and errors:

```bash
cargo run --release -- ./pallets ./runtime -s warning --fail-on-warning
```

See [Output & Exit Codes](output-and-exit-codes.md) for exactly which conditions
produce a non-zero exit.

To feed a code-scanning dashboard, emit SARIF and upload the artifact:

```bash
cargo run --release -- ./pallets -f sarif > polkadot-linter.sarif
```
