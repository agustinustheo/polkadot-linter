# Polkadot Linter

`polkadot-linter` is an external Rust linter for Polkadot SDK codebases. It
focuses on repeatable FRAME and runtime checks that sit *above* general-purpose
tooling like `cargo fmt` and `cargo clippy` — the security, weight, benchmarking,
validation-order, and test-quality patterns that are specific to building on
Substrate.

It combines two engines:

- a **compiler-backed** pipeline that runs security-critical rules through a
  rustc driver, so findings are grounded in resolved types, trait
  implementations, and macro-expanded code; and
- a **syntax/source** pipeline for rules where the authored text, attributes, or
  test structure is the strongest available evidence.

## Where to start

- **New here?** Read [Installation](guide/installation.md), then
  [Quick Start](guide/quick-start.md).
- **Running it in anger?** The [Command-Line Reference](guide/cli-reference.md)
  documents every flag, and [Output & Exit Codes](guide/output-and-exit-codes.md)
  covers formats, severities, and CI gating.
- **Tuning it?** See [Configuration](guide/configuration.md) and
  [CI Integration](guide/ci-integration.md).
- **Deciding what a finding means?** The
  [Rule Authority Matrix](rule-authority-matrix.md) says which engine is
  authoritative for each rule.

## The rest of this site

- The **Polkadot Linter Rules Reference** (sidebar) documents every rule with a
  dedicated page — what it detects, why it matters on a live chain, bad/good
  examples, and configuration.
- [Polkadot SDK Best Practices](best-practices/README.md) reproduces BlockDeep's
  Libro guide and annotates each practice with what, if anything, the linter
  enforces for it.
