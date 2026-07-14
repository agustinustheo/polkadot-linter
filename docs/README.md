# Polkadot Linter Documentation

`polkadot-linter` combines compiler-backed Rust security analysis with narrowly
scoped source rules where authored text is the strongest available evidence.

## Contents

- The **Rules Reference** documents every rule with a dedicated page — what it
  detects, why it matters on a live Polkadot/Substrate chain, bad/good code
  examples, and configuration. Rules are grouped by family:
  [Security](rules/security.md), [Validation](rules/validation.md),
  [Semantic](rules/semantic.md), [Benchmarking](rules/benchmarking.md),
  [Testing](rules/testing.md), [Mock Usage](rules/mock-usage.md), and
  [Terminology](rules/terminology.md).
- [Rule Authority Matrix](rule-authority-matrix.md) explains which engine is
  authoritative for every rule that needs a migration decision.
- [Polkadot SDK Best Practices](best-practices.md) maps high-value FRAME and
  SDK development practices to enforced checks, partial checks, and review
  gates.

## Local Build

Install the pinned mdBook release and build the site from the repository root:

```bash
cargo install mdbook --locked --version 0.4.52
mdbook build
```

The generated site is written to `book/`. The documentation workflow builds
this same artifact for pull requests and deploys it to GitHub Pages only from
`main`.

## Deployment

Before the first deployment, set **Settings > Pages > Build and deployment >
Source** to **GitHub Actions**. The workflow uploads the built mdBook artifact
for pull requests and deploys that artifact from pushes to `main`.
