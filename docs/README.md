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

This site documents what each rule detects and how to run the tool. If you are
looking for the rule catalogue, jump to the **Rules Reference** in the sidebar.
For how the checks map onto broader Polkadot development guidance, see
[Polkadot SDK Best Practices](best-practices/README.md).

## Installation

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
  default; override with `--driver-path` (see below).

Throughout this guide, commands are written as `cargo run -- …` so they work from
a source checkout. If you have installed the binary onto your `PATH`, drop the
`cargo run --` prefix and call `polkadot-linter …` directly.

## Quick start

Scan one or more directories:

```bash
# Scan a single directory
cargo run -- ../pallets

# Scan several paths at once
cargo run -- ../pallets ../runtimes ../support

# Scan the current directory (the default when no path is given)
cargo run --
```

Point the tool at your project configuration and tighten CI behaviour:

```bash
# Use a project-level configuration file
cargo run -- -c ../polkadot-linter.toml ../pallets

# Treat warnings as failures (non-zero exit) for CI gating
cargo run -- ../pallets -s warning --fail-on-warning
```

Emit machine-readable output for tooling and code scanning:

```bash
cargo run -- ../pallets -f json
cargo run -- ../pallets -f sarif > polkadot-linter.sarif
```

Route a specific compiler-backed rule through the rustc pipeline for one Cargo
package (the manifest is discovered from the scan path):

```bash
cargo run -- \
  --rules SEC003 \
  --package pallet-xcm \
  --lib \
  ../polkadot-sdk/polkadot/xcm/pallet-xcm
```

## Command-line reference

```text
polkadot-linter [OPTIONS] [PATHS]...
```

`PATHS` are one or more files or directories to scan. When omitted, the current
directory (`.`) is scanned.

### Common options

| Flag | Default | Purpose |
| --- | --- | --- |
| `-c`, `--config <FILE>` | `polkadot-linter.toml` | Project configuration file. If the default name is absent, built-in defaults are used; a missing *explicitly named* config is an error. |
| `-f`, `--format <FMT>` | `human` | Output format: `human`, `json`, or `sarif`. |
| `-s`, `--severity <LEVEL>` | `advisory` | Minimum severity to report: `advisory`, `warning`, or `error`. Lower-severity diagnostics are filtered out of the output. |
| `--fail-on-warning` | off | Exit non-zero if any warning (or error) is reported. |
| `--rules <IDS>` | all enabled | Comma-separated rule IDs or family prefixes to run (for example `--rules SEC003,VAL001` or `--rules SEC`). |
| `--include <GLOBS>` | — | Comma-separated glob patterns of files to include. |
| `--exclude <GLOBS>` | — | Comma-separated glob patterns of files to exclude. |
| `-v`, `--verbose` | off | Verbose (`debug`-level) logging. |

### Engine selection

| Flag | Default | Purpose |
| --- | --- | --- |
| `--syntax-only` | off | Run only the syntax/token pass; skip compiler-backed analysis. Appropriate when you only want the source/text rule families. |
| `--no-syntax` | off | Skip the syntax/token scan and emit only the compiler-backed (auxiliary) analysis results. |
| `--no-progress` | off | Hide Cargo's compiler progress during the compiler-backed phase. |

### Compiler-backed (rustc driver) options

These apply to the rustc-driven pipeline, which runs automatically when the scan
path resolves to a single Cargo project.

| Flag | Default | Purpose |
| --- | --- | --- |
| `--manifest-path <FILE>` | discovered | `Cargo.toml` to analyze. Discovered from the scan path if not given. |
| `--package <NAME>` | — | Package to pass to `cargo check`; may be repeated. |
| `--lib` | off | Analyze only the library target. |
| `--no-default-features` | off | Pass `--no-default-features` to `cargo check`. |
| `--target-dir <DIR>` | Cargo default | Cargo target directory for the compiler-backed check. |
| `--toolchain <NAME>` | `nightly-2025-09-01` | Rust toolchain used for compiler-backed analysis. |
| `--driver-path <FILE>` | `target/debug/polkadot-linter-driver` | Path to the `polkadot-linter-driver` binary. |
| `--compiler-backed-rules <IDS>` | migrated set | Comma-separated rule IDs to route through the rustc driver. |
| `--source-filter <SUBSTR>` | — | Comma-separated file-substring filters for compiler-backed diagnostics. |

> **Warm target directories.** The compiler-backed pipeline observes rustc
> diagnostics only when Cargo actually recompiles. If you re-run against a warm
> `--target-dir`, Cargo may skip compilation and the compiler-backed rules will
> report nothing. For repeatable compiler-backed runs, use a fresh target
> directory.

## Output formats

- **`human`** — coloured terminal output with file locations, the rule ID and
  severity, an explanation, and a suggested fix. Best for local use.
- **`json`** — machine-readable diagnostics for scripts and dashboards.
- **`sarif`** — SARIF 2.1.0, for CI code-scanning integrations (for example
  GitHub code scanning). Redirect to a file: `-f sarif > polkadot-linter.sarif`.

## Severity and exit codes

Every rule carries a severity: `advisory`, `warning`, or `error`. The
`-s/--severity` threshold filters which diagnostics appear — for example
`-s warning` hides advisories. Individual rule severities can be promoted or
demoted in configuration (see [Configuration](#configuration)).

The process exit code is designed for CI gating:

| Exit code | Meaning |
| --- | --- |
| `0` | No blocking diagnostics. |
| `1` | At least one `error`, **or** at least one `warning` when `--fail-on-warning` is set. |
| `2` | Usage error — bad configuration, a missing explicitly named config file, or a missing compiler-backed driver binary. |

## Selecting rules

Rules are grouped into families by prefix. Pass whole families or specific IDs to
`--rules`:

| Prefix | Family |
| --- | --- |
| `VAL` | Validation order and guard rails |
| `SEM` | Semantic and style checks |
| `TST` | Test quality checks |
| `MOK` | Mock usage heuristics |
| `BEN` | Benchmark coverage and verification |
| `TRM` | Terminology and text conventions |
| `SEC` | Security-focused rules |

```bash
# Run only the security family
cargo run -- ../pallets --rules SEC

# Run two specific rules
cargo run -- ../pallets --rules VAL001,SEC008
```

Each rule has a dedicated page in the **Rules Reference** describing what it
detects, why it matters on a live chain, bad/good examples, and configuration.

## Configuration

By default the CLI looks for `polkadot-linter.toml` in the working directory. A
project can enable or disable rules, override severities, and tune the
family-specific heuristics:

```toml
[rules.enabled]
BEN002 = false          # disable a noisy rule

[rules.severity]
TST002 = "error"        # promote a rule

[validation_order]
heavy_operations = ["::get(", "::iter(", "::contains_key("]
cheap_validations = ["ensure!", ".is_empty()", ".is_zero()"]

[terminology.british_english]
"optimisation" = "optimization"
```

- `[rules.enabled]` is optional; rules without an explicit `false` stay enabled.
- `[rules.severity]` promotes or demotes individual rules without code changes.
- `[validation_order]`, `[test_smells]`, `[mock_usage]`, and `[benchmarking]`
  tune the heuristics behind those families.
- `[terminology.british_english]` and `[terminology.forbidden_terms]` drive
  `TRM001` and are meant to be customised per project.

The full schema and defaults live in the repository's `config/default.toml`.

## CI integration

`polkadot-linter` complements standard Rust tooling rather than replacing it. A
typical CI order is:

```text
cargo fmt -> cargo clippy -> polkadot-linter
```

A minimal gating step that fails the build on warnings and errors:

```bash
cargo run --release -- ./pallets ./runtime -s warning --fail-on-warning
```

To feed a code-scanning dashboard, emit SARIF and upload the artifact:

```bash
cargo run --release -- ./pallets -f sarif > polkadot-linter.sarif
```

## Where to go next

- **Rules Reference** — every rule, grouped by family, in the sidebar.
- [Rule Authority Matrix](rule-authority-matrix.md) — which engine is
  authoritative for each rule that needs a migration decision.
- [Polkadot SDK Best Practices](best-practices/README.md) — how the enforced
  checks map onto broader FRAME/SDK development guidance, adapted from Libro.
