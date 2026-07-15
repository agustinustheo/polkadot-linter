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

## crates.io releases

Pushing a tag named `v<package-version>` runs the release workflow. It verifies
the tag against `Cargo.toml`, checks formatting, runs Clippy with warnings
denied, runs tests, builds the release binary, and performs a `cargo publish
--dry-run` before publishing to crates.io.

The workflow reads its publishing credential from the repository Actions secret
`CARGO_REGISTRY_TOKEN`. Never place a crates.io token in the repository, tag,
or workflow source.
