# Quick start

## 1. Run a scan

```bash
polkadot-linter ./runtime ./pallets
```

With no path, the linter scans the current directory. Use `--syntax-only` to
skip the build that the compiler rules need.

## 2. Read the findings

Each finding shows a file location, a rule ID, a severity, an explanation and
a suggested fix. To learn more, open the rule page from [All rules](../rule-index.md).

There are three severities:

| Severity | Meaning |
| --- | --- |
| `error` | Very likely a bug. Fix it. |
| `warning` | Probably a problem. Review it. |
| `advisory` | A hint. Fix it when it is easy. |

`-s warning` hides advisories. `-s error` shows errors only.

## 3. Act on each finding

- If the finding is correct, fix the code.
- If the code is correct on purpose, add an allow comment with a reason. See
  [Suppress findings and baselines](suppressing-findings.md).
- On an existing codebase, record today's findings in a baseline. Then only
  new findings are reported.

## Output formats

| Flag | Output |
| --- | --- |
| `-f human` | Colored text for a terminal (default) |
| `-f json` | A JSON array of findings |
| `-f sarif` | SARIF 2.1.0, for GitHub code scanning |

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | No error. With `--fail-on-warning`, also no warning. |
| `1` | At least one error. With `--fail-on-warning`, at least one warning or error. |
| `2` | The run failed: bad configuration or flags, a missing config file that you named, a baseline that cannot be read or written, or a compiler run that failed. |

## Next steps

- Change rules and severities: [Configuration](configuration.md).
- See every flag: [Command-line options](cli-reference.md).
- Run the linter in CI: [CI and AI agents](ci-integration.md).
