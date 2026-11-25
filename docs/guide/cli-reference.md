# Command-line options

```text
polkadot-linter [OPTIONS] [PATHS]...
```

`PATHS` are files or directories. The default is the current directory.
`polkadot-linter --help` shows the same list.

## Common options

| Flag | Default | What it does |
| --- | --- | --- |
| `-c`, `--config <FILE>` | `polkadot-linter.toml` | Config file. If you name a file that does not exist, the run fails. |
| `--profile <NAME>` | none | Built-in rule profile (`default`, `security`, or `all`) overlaid on top of the loaded config, preserving project excludes and severity overrides. A rule that the project file lists in `[rules.enabled]` keeps that setting. `default` leaves the project config unchanged. `security` also turns on `exclude_testnet_runtimes` and `rtc008_value_escalations_only` when the project config leaves them unset; stderr names the testnet runtimes it skips, or says that it lints them because they are every runtime in the scan. |
| `-f`, `--format <FMT>` | `human` | `human`, `json` or `sarif`. JSON diagnostics include `confidence` (`low`/`medium`/`high`) and optional `also_in`. |
| `-s`, `--severity <LEVEL>` | `advisory` | Lowest severity to show: `advisory`, `warning` or `error`. |
| `--confidence <LEVEL>` | `low` | Lowest triage confidence to show (`low`, `medium`, or `high`). Applied before exit-code calculation. |
| `--group` | off | Group diagnostics with identical rule ID and message into one entry (keeping the highest-severity/confidence occurrence first, with secondary locations in `also_in` / SARIF `relatedLocations`). Applied after inline allows and baselines. |
| `--fail-on-warning` | off | Exit with code 1 on a warning too. |
| `--rules <IDS>` | all enabled rules | Rule IDs or families, for example `RTC,SEC020`. Rules that are off stay off. |
| `--include <GLOBS>` | none | Only scan files that match. |
| `--exclude <GLOBS>` | none | Do not scan files that match. |
| `--baseline <PATH>` | none | Hide the findings in this baseline. |
| `--write-baseline <PATH>` | none | Write all current findings to this baseline, then exit 0. |
| `-v`, `--verbose` | off | Show debug logs. |

## Engine options

| Flag | Default | What it does |
| --- | --- | --- |
| `--syntax-only` | off | Run only the syntax engine. No build. |
| `--no-syntax` | off | Run only the compiler engine. |
| `--no-progress` | off | Hide Cargo build progress. |
| `--driver-path <FILE>` | next to the `polkadot-linter` binary, then `target/debug/polkadot-linter-driver` | Path to the driver. |
| `--toolchain <NAME>` | `nightly-2025-09-01` | Toolchain for the compiler engine. |
| `--manifest-path <FILE>` | found from the paths | `Cargo.toml` to check. Use it when the paths belong to more than one Cargo project. |
| `--package <NAME>` | Cargo's default package selection | Package to check. Repeat the flag to choose packages. |
| `--lib` | off | Check only the library target. |
| `--features <LIST>` | none | Cargo features. Both engines use them. |
| `--no-default-features` | off | Turn off default Cargo features. Both engines use this. |
| `--target-dir <DIR>` | Cargo default | Cargo target directory. |
| `--compiler-backed-rules <IDS>` | all compiler rules | Compiler rules to run. |
| `--source-filter <TEXT>` | none | Only report compiler findings in files whose path contains this text. |

The compiler engine runs `cargo check` with the driver. It sees only the crates
that Cargo compiles again. The linter forces a new build for each scan, so a
warm `--target-dir` is safe.
