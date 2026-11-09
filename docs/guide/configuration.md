# Configuration

The linter reads `polkadot-linter.toml` from the working directory. To use a
different file, pass `-c FILE`. Your file changes the bundled
[`config/default.toml`](https://github.com/agustinustheo/polkadot-linter/blob/main/config/default.toml)
one entry at a time. You only write the entries you want to change.

```toml
[rules.enabled]
SEC024 = true          # turn on a rule that is off by default
SEM014 = false         # turn off a rule

[rules.severity]
SEM009 = "warning"     # advisory, warning or error
```

## Sections

| Section | What it controls |
| --- | --- |
| `[rules.enabled]` | Turns each rule on or off. |
| `[rules.severity]` | Changes the severity of each rule. It does not turn a rule on. |
| `[runtime_config]` | Which runtimes the RTC rules check, and RTC001 settings. See [Runtime configuration](../rules/runtime-config.md). |
| `[test_smells]` | `severity` for the TST rules. |
| `[terminology]` | Settings for TRM001. See [TRM001](../rules/TRM001.md). |
| `[general]` | `include` and `exclude` globs for files. |

## Rules that are off by default

`RTC004`, `SEC004` and `SEC024` are off by default. They are often correct but
seldom worth a change. To turn one on, set it to `true` under
`[rules.enabled]`. To run every rule, use the bundled `config/all-rules.toml`:

```bash
polkadot-linter -c config/all-rules.toml ./runtime
```

## Old config files

The linter ignores entries for deleted rules. It also ignores the removed
`[validation_order]`, `[mock_usage]` and `[benchmarking]` sections, and prints
one warning for each section. Delete these sections to stop the warning. Other
unknown keys are an error.
