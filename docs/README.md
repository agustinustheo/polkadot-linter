# Polkadot Linter

**A precision-first linter for Polkadot SDK runtimes and pallets. A rule is on
by default only when it was measured accurate on real code.**

`polkadot-linter` finds bugs specific to Polkadot SDK code: XCM barriers that
nested programs bypass, proxy filters that fail open, discarded proof verdicts,
vulnerable crate versions in `Cargo.lock`, free XCM delivery, missing
benchmarks. It runs after `cargo fmt` and `cargo clippy`, and covers what they
cannot know about FRAME.

```bash
cargo install polkadot-linter
polkadot-linter ./pallets ./runtime
```

## Two engines

- **Syntax engine** (`syn`, stable Rust 1.91+). Reads source text, attributes
  and `Cargo.lock`. Runs on every scan. Covers the `RTC`, `BEN`, `TST`, `TRM`
  and `MOK` families and most `SEM` rules.
- **Compiler-backed engine.** A rustc driver on `nightly-2025-09-01` that sees
  resolved types, trait impls and macro-expanded code. Covers every `SEC` rule
  plus `SEM006`, `SEM009`, `SEM010`, `SEM016`, `VAL002` and `VAL003`. It runs
  when the scan paths resolve to one Cargo project and the driver binary is
  available; `--syntax-only` skips it.

[Installation](guide/installation.md) shows how to build or install the driver,
and the [Rule Authority Matrix](rule-authority-matrix.md) says which engine
owns each rule.

## Precision first

31 of the 64 rules are on by default. The other 33 were measured on real code
and found too noisy, or are Parity SDK style conventions. They stay available
through `[rules.enabled]` or `config/all-rules.toml`.

Two labeled audits set the defaults. On 10 third-party parachain repositories,
a rule is off if under 30% of its sampled findings are worth acting on, or 30%
or more are false positives; default findings dropped from 2,680 to 750 and the
estimated useful share rose from 18.5% to 58.3%. A second, run-based audit on
polkadot-sdk and the same repositories turned off 7 more rules (4 true positives
in 91 labeled samples). Rules are also checked against auditor-reported bugs at
the commits that were audited. The method, numbers and caveats are in the
repository's `benchmarks/precision/` directory.

## Where to start

- **New here?** Read [Installation](guide/installation.md), then
  [Quick Start](guide/quick-start.md).
- **Running it in CI?** The [Command-Line Reference](guide/cli-reference.md)
  documents every flag, and [Output & Exit Codes](guide/output-and-exit-codes.md)
  covers formats, severities and gating. See also
  [CI Integration](guide/ci-integration.md).
- **Tuning it?** See [Configuration](guide/configuration.md).
- **Using a coding agent?** Give it the
  [agent skill](https://github.com/agustinustheo/polkadot-linter/blob/main/skill/SKILL.md).
  The repository README's "For AI agents" section shows how to install it.
- **Looking up a rule?** [All Rules](rule-index.md) lists every rule on one
  page. The **Rules Reference** in the sidebar has a page per rule: what it
  detects, the incident behind it, bad and good examples, and configuration.

[Polkadot SDK Best Practices](best-practices/README.md) reproduces BlockDeep's
Libro guide and notes what, if anything, the linter enforces for each practice.
