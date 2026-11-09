# Polkadot Linter

`polkadot-linter` finds bugs that are specific to Polkadot SDK runtimes and
pallets. Examples: an XCM barrier that nested programs bypass, a proxy filter
that fails open, a discarded proof verdict, or a vulnerable crate version in
`Cargo.lock`.

Run it after `cargo fmt` and `cargo clippy`. Those tools do not know FRAME.
This linter does.

```bash
cargo install polkadot-linter
polkadot-linter ./runtime ./pallets
```

## Few findings, high precision

The linter has 31 rules. 28 are on by default. A rule is on by default only if
it was measured as accurate on real parachain code. Most rules come from a real
incident. We deleted 33 rules that were mostly wrong or not worth fixing.
[Retired rules](retired-rules.md) tells you how to catch those problems
instead.

## Two engines

| Engine | What it reads | Rules | Requirement |
| --- | --- | --- | --- |
| Syntax | Source text, attributes, `Cargo.lock` | RTC, TST, TRM, SEM011, SEM014, SEM015, SEM019, LNT001 | Stable Rust 1.91+ |
| Compiler | Resolved types and expanded macros | All SEC, SEM009, SEM010, SEM016 | The driver on `nightly-2025-09-01` |

The syntax engine always runs. The compiler engine runs when the driver is
installed and the scan paths belong to one Cargo project.
[Install](guide/installation.md) shows how to install the driver.

## Where to go next

1. [Install](guide/installation.md) the linter.
2. Do the [Quick start](guide/quick-start.md).
3. Read the page of each rule that reports a finding. [All rules](rule-index.md)
   lists them.
4. Add the linter to [CI or your AI agent](guide/ci-integration.md).
