# Command-Line Reference

```text
polkadot-linter [OPTIONS] [PATHS]...
```

`PATHS` are one or more files or directories to scan. When omitted, the current
directory (`.`) is scanned.

## Common options

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

## Engine selection

| Flag | Default | Purpose |
| --- | --- | --- |
| `--syntax-only` | off | Run only the syntax/token pass; skip compiler-backed analysis. Appropriate when you only want the source/text rule families. |
| `--no-syntax` | off | Skip the syntax/token scan and emit only the compiler-backed (auxiliary) analysis results. |
| `--no-progress` | off | Hide Cargo's compiler progress during the compiler-backed phase. |

## Compiler-backed (rustc driver) options

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
