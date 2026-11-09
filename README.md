<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.png">
    <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.png">
    <img src="assets/logo-light.png" alt="polkadot-linter" width="820">
  </picture>
</p>

<p align="center">
  <b>A precision-first linter for Polkadot SDK runtimes and pallets. A rule is on by default only when it was measured accurate on real code.</b>
</p>

<p align="center">
  <a href="https://github.com/agustinustheo/polkadot-linter/actions/workflows/ci.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/agustinustheo/polkadot-linter/ci.yml?branch=main&label=build&logo=githubactions&logoColor=white"></a>
  <a href="https://github.com/agustinustheo/polkadot-linter/actions/workflows/docs.yml"><img alt="Docs build" src="https://img.shields.io/github/actions/workflow/status/agustinustheo/polkadot-linter/docs.yml?branch=main&label=docs&logo=mdbook"></a>
  <a href="https://github.com/agustinustheo/polkadot-linter/tags"><img alt="Release" src="https://img.shields.io/github/v/tag/agustinustheo/polkadot-linter?label=release&logo=github&color=E6007A"></a>
  <a href="LICENSE"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-2EC4CC"></a>
  <br>
  <a href="https://crates.io/crates/polkadot-linter"><img alt="crates.io" src="https://img.shields.io/crates/v/polkadot-linter?logo=rust&color=E6007A"></a>
  <a href="https://crates.io/crates/polkadot-linter"><img alt="Downloads" src="https://img.shields.io/crates/d/polkadot-linter?color=555"></a>
  <a href="https://agustinustheo.github.io/polkadot-linter"><img alt="Docs site" src="https://img.shields.io/badge/docs-mdBook-552BBF?logo=mdbook&logoColor=white"></a>
  <img alt="Rust 1.91+" src="https://img.shields.io/badge/rust-1.91%2B-F74C00?logo=rust&logoColor=white">
  <img alt="Driver: nightly-2025-09-01" src="https://img.shields.io/badge/driver-nightly--2025--09--01-555?logo=rust&logoColor=white">
</p>

---

`polkadot-linter` finds bugs that are specific to Polkadot SDK runtimes and
pallets: XCM barriers that nested programs bypass, proxy filters that fail open,
discarded proof verdicts, vulnerable crate versions in `Cargo.lock`, free XCM
delivery. Run it after `cargo fmt` and `cargo clippy`. It checks what they do
not know about FRAME.

- **29 rules, 25 on by default.** A rule is on only if it was measured as
  accurate on real parachain code. We deleted 35 rules that were not.
- **From incidents.** Most rules copy a real bug: the dotUSD exploit,
  polkadot-sdk #7200, #11738, #12771, fellows runtimes #1237 and #1341,
  GHSA-wwx5.
- **Two engines.** Fast syntax rules, and compiler rules that see resolved
  types through a rustc driver.
- **CI-ready.** Human, JSON and SARIF output. Allow comments and baselines.

**Documentation: [agustinustheo.github.io/polkadot-linter](https://agustinustheo.github.io/polkadot-linter)**

## Install

```bash
cargo install polkadot-linter
```

The compiler rules (SEC, SEM009, SEM010, SEM016) also need the driver:

```bash
rustup toolchain install nightly-2025-09-01 --component rustc-dev --component llvm-tools-preview
cargo +nightly-2025-09-01 install polkadot-linter --features rustc-driver
```

## Use

```bash
polkadot-linter --manifest-path Cargo.toml ./runtime ./pallets   # full scan from the workspace root
polkadot-linter --syntax-only .                          # no build, syntax rules only
polkadot-linter --rules RTC,SEC020 ./runtime             # some families or rules
polkadot-linter -s warning --fail-on-warning ./pallets   # CI gate
polkadot-linter -f sarif ./pallets > polkadot-linter.sarif
```

Exit code `1` means an error was found (or a warning, with
`--fail-on-warning`). Exit code `2` means the run failed.

To hide a finding that you reviewed, add
`// polkadot-linter: allow(RTC004, reason = "dev chain")`. To adopt the linter
on an existing codebase, use `--write-baseline` and `--baseline`.

## Rules

| Family | Rules | Finds |
| --- | --- | --- |
| [RTC](https://agustinustheo.github.io/polkadot-linter/rules/runtime-config.html) | RTC001-RTC010 | Runtime and XCM config mistakes, vulnerable `Cargo.lock` versions |
| [SEC](https://agustinustheo.github.io/polkadot-linter/rules/security.html) | SEC004, SEC006, SEC010-SEC012, SEC015, SEC019-SEC024 | Lost funds, skipped origin or proof checks, unbounded storage work |
| [SEM](https://agustinustheo.github.io/polkadot-linter/rules/semantic.html) | SEM009-SEM011, SEM015, SEM016, SEM019 | Zero weights, `^` as a power, verifiers that accept all input |
| [TST](https://agustinustheo.github.io/polkadot-linter/rules/testing.html) | TST002 | `assert_ok!` that hides dispatch errors |
| [TRM](https://agustinustheo.github.io/polkadot-linter/rules/terminology.html) | TRM001 | Spelling and terms |

`RTC004`, `SEC004`, `SEC011` and `SEC024` are off by default. [All rules](https://agustinustheo.github.io/polkadot-linter/rule-index.html)
lists every rule with its severity. [Retired rules](https://agustinustheo.github.io/polkadot-linter/retired-rules.html)
lists the 35 deleted rules and the best way to catch each problem instead.

## For AI agents

[`skill/SKILL.md`](skill/SKILL.md) tells a coding agent when to run the linter
and how to triage each finding. For Claude Code:

```bash
mkdir -p ~/.claude/skills/polkadot-linter && curl -fsSL https://raw.githubusercontent.com/agustinustheo/polkadot-linter/main/skill/SKILL.md -o ~/.claude/skills/polkadot-linter/SKILL.md
```

For other agents, see [CI and AI agents](https://agustinustheo.github.io/polkadot-linter/guide/ci-integration.html). The
linter does not call an LLM.

## Configuration

Put a `polkadot-linter.toml` in the working directory, or pass `-c FILE`. It
changes the bundled [`config/default.toml`](config/default.toml) one entry at a
time.

```toml
[rules.enabled]
SEC024 = true    # turn on a rule that is off by default

[rules.severity]
SEM009 = "warning"
```

See [Configuration](https://agustinustheo.github.io/polkadot-linter/guide/configuration.html).

## How rules are measured

Three labeled audits set the defaults: 10 third-party parachain repositories,
a run-based audit of polkadot-sdk, and a strict acceptance test on
`paritytech/individuality-community`. The data is in
[`benchmarks/precision/`](benchmarks/precision). See
[How rules are measured](https://agustinustheo.github.io/polkadot-linter/measurement.html).

## Contributing

See [Contributing](https://agustinustheo.github.io/polkadot-linter/contributing.html).

## License

[Apache License, Version 2.0](LICENSE). See also [NOTICE](NOTICE).
