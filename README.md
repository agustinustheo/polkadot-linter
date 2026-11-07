<p align="center">
  <img src="assets/logo.svg" alt="polkadot-linter: a magnifying glass over a grid of dots, one flagged" width="820">
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
  <a href="http://agustinustheo.github.io/polkadot-linter"><img alt="Docs site" src="https://img.shields.io/badge/docs-mdBook-552BBF?logo=mdbook&logoColor=white"></a>
  <img alt="Rust 1.91+" src="https://img.shields.io/badge/rust-1.91%2B-F74C00?logo=rust&logoColor=white">
  <img alt="Driver: nightly-2025-09-01" src="https://img.shields.io/badge/driver-nightly--2025--09--01-555?logo=rust&logoColor=white">
</p>

---

`polkadot-linter` finds bugs specific to Polkadot SDK code: XCM barriers that
nested programs bypass, proxy filters that fail open, discarded proof
verdicts, vulnerable crate versions in `Cargo.lock`, free XCM delivery, missing
benchmarks. It runs after `cargo fmt` and `cargo clippy`, and covers what they
cannot know about FRAME.

Most rules come from a real incident. Each one is measured on third-party
parachain code before it is turned on. Rules that were too noisy stay
available, but are off unless you ask for them.

## Highlights

- **Precision first.** 29 of 63 rules are on by default. The rest were
  measured, found noisy, and turned off. The numbers are in
  [`benchmarks/precision/`](benchmarks/precision).
- **From incidents.** Rules encode real fixes: polkadot-sdk #7200, #11738,
  #12771, #1739, fellows runtimes #1237 and #1341, GHSA-wwx5, and the dotUSD backports.
- **Two engines.** Fast `syn` rules for source and runtime config, and
  compiler-backed rules that see resolved types through a rustc driver.
- **CI-ready.** `human`, `json` and SARIF 2.1.0 output. Exit code `1` on errors,
  or on warnings with `--fail-on-warning`.
- **Configurable.** One TOML file. Turn rules on or off, change severities, tune
  heuristics. `config/all-rules.toml` runs everything.

## Install

```bash
cargo install polkadot-linter
polkadot-linter --version
```

That gives you every syntax and lockfile rule. Compiler-backed rules also need
the driver, which builds on a pinned nightly:

```bash
rustup toolchain install nightly-2025-09-01 --component rustc-dev --component llvm-tools-preview
cargo +nightly-2025-09-01 install polkadot-linter --features rustc-driver
```

This installs both `polkadot-linter` and `polkadot-linter-driver`. Pass the
driver with `--driver-path ~/.cargo/bin/polkadot-linter-driver` (the default is
`target/debug/polkadot-linter-driver`, for source checkouts). Releases are
published to [crates.io](https://crates.io/crates/polkadot-linter) only; there
are no prebuilt binaries.

## Tour

```bash
polkadot-linter ./pallets ./runtime                       # scan; compiler rules run if the driver is found
polkadot-linter --driver-path ~/.cargo/bin/polkadot-linter-driver ./runtime
polkadot-linter --syntax-only .                           # syntax and lockfile rules only, no build
polkadot-linter --rules RTC,SEC020 ./runtime              # pick families or rule IDs
polkadot-linter -s warning --fail-on-warning ./pallets    # CI gate
polkadot-linter -f sarif ./pallets > polkadot-linter.sarif
polkadot-linter -c config/all-rules.toml ./pallets        # every rule, noisy ones included
polkadot-linter --package pallet-xcm --lib ./xcm/pallet-xcm   # compiler rules for one package
```

`polkadot-linter --help` lists every flag. The
[CLI reference](http://agustinustheo.github.io/polkadot-linter/guide/cli-reference.html)
explains them.

## For AI agents

Most runs come from coding agents. [`skill/SKILL.md`](skill/SKILL.md) tells
them when to run the linter, which commands to use, how to read the JSON, and
how to triage each finding (fix it, or allow it with a reason).

- **Claude Code:** copy the skill so every session knows the tool:

  ```bash
  mkdir -p ~/.claude/skills/polkadot-linter && cp skill/SKILL.md ~/.claude/skills/polkadot-linter/
  ```

- **Codex and other agents:** add one line to `AGENTS.md` (or the agent's
  instructions):
  *"For Polkadot SDK, FRAME or runtime code, run `polkadot-linter` and triage
  each finding as described in https://github.com/agustinustheo/polkadot-linter/blob/main/skill/SKILL.md."*

The linter itself never calls an LLM.

## Two engines

| | Syntax engine | Compiler-backed engine |
|---|---|---|
| Sees | source text, attributes, `Cargo.lock` | resolved types, trait impls, macro-expanded code |
| Rules | `RTC`, `BEN`, `TST`, `TRM`, `MOK`, most `SEM`, `VAL001` | all `SEC`, `SEM006`, `SEM009`, `SEM010`, `SEM016`, `VAL002`, `VAL003` |
| Runs | always | when the scan paths share one nearest `Cargo.toml` (or `--manifest-path`) and the driver exists |
| Toolchain | stable, Rust 1.91+ | `nightly-2025-09-01` with `rustc-dev` |

The compiler engine runs `cargo check` with the driver as a workspace wrapper.
If the driver is missing, the CLI says so and emits syntax findings only.
`--syntax-only` skips the compiler engine; `--no-syntax` skips the syntax one.
`--features` and `--no-default-features` apply to both.

## What it catches by default

| Rule | Severity | Catches | From |
|---|---|---|---|
| [RTC003](http://agustinustheo.github.io/polkadot-linter/rules/RTC003.html) | error | XCM delivery is free (`WaiveDeliveryFees`, `()` price) | |
| [RTC006](http://agustinustheo.github.io/polkadot-linter/rules/RTC006.html) | warning | XCM `FeeManager = ()` burns delivery fees | |
| [RTC007](http://agustinustheo.github.io/polkadot-linter/rules/RTC007.html) | warning | `DenyThenTry` deny not wrapped in `DenyRecursively`; nested programs bypass it | polkadot-sdk #7148, #7200 |
| [RTC008](http://agustinustheo.github.io/polkadot-linter/rules/RTC008.html) | warning | Proxy filter deny-list or `is_superset` wildcard fails open | polkadot-sdk #12771, fellows runtimes #1341 |
| [RTC009](http://agustinustheo.github.io/polkadot-linter/rules/RTC009.html) | error | `Cargo.lock` pins a crate with a quietly patched security bug | dotUSD fix backports, InitiateTransfer origin leak |
| [RTC010](http://agustinustheo.github.io/polkadot-linter/rules/RTC010.html) | warning | `AuthorizedAliasers` on the unpaid-execution barrier: unmetered storage reads | fellows runtimes #1237 |
| [RTC001](http://agustinustheo.github.io/polkadot-linter/rules/RTC001.html), [RTC002](http://agustinustheo.github.io/polkadot-linter/rules/RTC002.html) | warning | Placeholder `WeightInfo` in a runtime; pallet missing from `define_benchmarks!` | |
| [RTC005](http://agustinustheo.github.io/polkadot-linter/rules/RTC005.html) | warning | Runtime uses collective-flip randomness | |
| [SEC020](http://agustinustheo.github.io/polkadot-linter/rules/SEC020.html) | error | A verifier's `bool` verdict is discarded, so bad proofs pass | polkadot-sdk #11738 |
| [SEC021](http://agustinustheo.github.io/polkadot-linter/rules/SEC021.html) | error | Inverted verify guard: `if verify(..) { reject }` rejects valid proofs and accepts bad ones | GHSA-wwx5 (grandpa-verifier-primitives) |
| [SEM019](http://agustinustheo.github.io/polkadot-linter/rules/SEM019.html) | warning | Verifier trait method defaults to `true` | polkadot-sdk #1739 |
| [SEC006](http://agustinustheo.github.io/polkadot-linter/rules/SEC006.html), [SEC010](http://agustinustheo.github.io/polkadot-linter/rules/SEC010.html), [SEC011](http://agustinustheo.github.io/polkadot-linter/rules/SEC011.html), [SEC012](http://agustinustheo.github.io/polkadot-linter/rules/SEC012.html), [SEC015](http://agustinustheo.github.io/polkadot-linter/rules/SEC015.html) | warning | Ignored `repatriate_reserved` remainder, non-transactional hooks, storage iteration in dispatchables, unbounded `clear_prefix`, `dispatch_bypass_filter` | |
| [SEM010](http://agustinustheo.github.io/polkadot-linter/rules/SEM010.html) | error | `^` used as exponentiation (it is XOR) | |
| [SEM011](http://agustinustheo.github.io/polkadot-linter/rules/SEM011.html), [SEM015](http://agustinustheo.github.io/polkadot-linter/rules/SEM015.html), [SEM016](http://agustinustheo.github.io/polkadot-linter/rules/SEM016.html) | warning | Zero weights; `authorize` without `weight_of_authorize`; missing `AuthorizeCall` | |
| [BEN002](http://agustinustheo.github.io/polkadot-linter/rules/BEN002.html), [BEN003](http://agustinustheo.github.io/polkadot-linter/rules/BEN003.html) | warning | Benchmark without verification; extrinsic without benchmark | |
| [TST002](http://agustinustheo.github.io/polkadot-linter/rules/TST002.html), [TST006](http://agustinustheo.github.io/polkadot-linter/rules/TST006.html) | error, warning | `assert_ok!` on `apply_extrinsic`; storage-mutating extrinsic without an event | |
| SEM009, SEM013, SEM014, TRM001 | advisory, warning | Redundant `contains_key`, `#[repr(u8)]` invalidity enums, OCW log targets, spelling | |

Every rule, including the 34 that are off by default, is in the
[rule reference](http://agustinustheo.github.io/polkadot-linter/rule-index.html).

## How rules are measured

Defaults come from two labeled audits:

- **[2026-11-external](benchmarks/precision/2026-11-external/README.md).** 10
  third-party parachain repositories (Moonbeam, Astar, Acala, Hydration, ...).
  291 sampled findings, two blind labelers plus an adjudicator. A rule is off if
  under 30% of its findings are worth acting on, or 30% or more are false.
  Default findings went from 2,680 to 750; the estimated useful share from
  18.5% to 58.3%.
- **[2026-11-run-audit](benchmarks/precision/2026-11-run-audit/README.md).**
  polkadot-sdk plus the same repositories, compiler rules on 8 runtimes. It
  turned off 7 more rules, which had produced 313 default findings with 4 true
  positives in 91 labeled samples. Default findings went from 1,534 to 1,221.

Rules are also run against auditor-reported bugs at the exact commits that were
audited. Labels are judgments, not an oracle, and samples are small (12 to 15
per rule). Each README lists its caveats.

## Configuration

Put a `polkadot-linter.toml` in the working directory, or pass `-c FILE`. It
overrides the bundled [`config/default.toml`](config/default.toml) entry by
entry.

```toml
[rules.enabled]
SEC019 = true          # opt in to a default-off rule
BEN002 = false

[rules.severity]
TST002 = "error"
```

A severity alone does not enable a rule. Other sections tune the heuristics of
each family; see
[Configuration](http://agustinustheo.github.io/polkadot-linter/guide/configuration.html).

To keep a reviewed, intentional finding quiet, add
`// polkadot-linter: allow(RTC004, reason = "dev chain")` on or above the line.
When adopting the linter on an existing codebase, record today's findings with
`--write-baseline baseline.json` and run with `--baseline baseline.json` so only
new findings are reported. See
[Suppressing findings](http://agustinustheo.github.io/polkadot-linter/guide/suppressing-findings.html).

## Output and CI

- `-f human` (default): coloured, with locations and suggested fixes.
- `-f json`: an array of diagnostics.
- `-f sarif`: SARIF 2.1.0 for GitHub code scanning.

Exit codes: `0` nothing blocking · `1` an error, or a warning with
`--fail-on-warning` · `2` bad config or usage. `-s warning` hides advisories.
A typical pipeline is `cargo fmt` → `cargo clippy` → `polkadot-linter`.

## Project layout

| Path | What |
|---|---|
| `src/main.rs` | CLI, engine selection, exit codes |
| `src/engine.rs` | syntax engine: file walk, rule dispatch |
| `src/rules/` | syntax rules by family, plus `vulnerable_crates.tsv` for RTC009 |
| `src/bin/polkadot-linter-driver.rs` | rustc driver with the compiler-backed rules |
| `src/rustc_pipeline.rs` | runs `cargo check` with the driver |
| `src/frame_model.rs`, `src/project_model.rs` | FRAME and Cargo project models |
| `config/default.toml`, `config/all-rules.toml` | bundled defaults, every rule on |
| `benchmarks/precision/` | precision audits behind the defaults |
| `benchmarks/labels/`, `benchmarks/*.tsv` | labeled findings and pinned-SDK baselines |
| `scripts/` | corpus and driver checks run in CI |
| `docs/` | the [mdBook site](http://agustinustheo.github.io/polkadot-linter) and per-rule pages |

To add a rule: implement it in `src/rules/` (syntax) or the driver
(compiler-backed), register its ID in `src/rules/mod.rs`, add fixtures or
driver coverage, and extend the SDK benchmark manifest if it needs pinned-SDK
coverage. The [rule authority matrix](http://agustinustheo.github.io/polkadot-linter/rule-authority-matrix.html)
says which engine owns each rule.

## License

[Apache License, Version 2.0](LICENSE). See also [NOTICE](NOTICE).
