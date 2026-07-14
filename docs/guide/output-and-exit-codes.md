# Output & Exit Codes

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
demoted in configuration (see [Configuration](configuration.md)).

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

Each rule has a dedicated page in the **Polkadot Linter Rules Reference**
describing what it detects, why it matters on a live chain, bad/good examples,
and configuration.
