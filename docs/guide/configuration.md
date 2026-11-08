# Configuration

By default the CLI looks for `polkadot-linter.toml` in the working directory. A
project can enable or disable rules, override severities, and tune the
runtime-configuration and terminology settings:

```toml
[rules.enabled]
SEM014 = false          # disable a rule
SEC004 = true           # enable a default-off rule

[rules.severity]
TST002 = "error"        # promote a rule

[terminology.british_english]
"optimisation" = "optimization"
```

- `[rules.enabled]` is optional and overrides the bundled entries. Three rules
  are off by default because they were correct but rarely actionable:
  `RTC004`, `SEC004` and `SEC024`. Each rule page and family table says "off by
  default", and [All Rules](../rule-index.md) lists them on one page. The
  evidence is in the repository's `benchmarks/precision/` audits. Set a rule to
  `true` to enable it, or use the repository's `config/all-rules.toml` to run
  every rule. Rules that were mostly wrong or not worth fixing were deleted;
  [What we tried and removed](../retired-rules.md) lists them. Entries for a
  deleted rule in `[rules.enabled]` or `[rules.severity]` are ignored.
- `[rules.severity]` promotes or demotes individual rules without code changes.
  It does not enable a rule: a severity set for a default-off rule (for example
  `SEC024 = "error"`) has no effect until the rule is also
  set to `true` under `[rules.enabled]`.
- `[test_smells]` sets the family-wide `severity` of the `TST` rules. The
  `[validation_order]`, `[mock_usage]` and `[benchmarking]` sections were
  removed with their rules; a config that still has them fails to load with an
  "unknown field" error, so delete them.
- `[runtime_config]` sets which runtimes the `RTC` rules inspect
  (`include_dev_runtimes`, `dev_runtime_packages`, `dev_runtime_tokens`,
  `test_runtime_tokens`), an optional family-wide `severity` (unset by
  default, so each rule keeps its own default: RTC003 is error, the rest
  warning), the pallets whose unit `WeightInfo` is intended (`unit_weight_info_allowlist`), and whether upstream
  `SubstrateWeight` bindings are reported (`flag_substrate_weight`). See
  [Runtime Configuration Rules](../rules/runtime-config.md).
- `[terminology.british_english]` and `[terminology.forbidden_terms]` drive
  `TRM001` and are meant to be customised per project.

The full schema and defaults live in the repository's `config/default.toml`.
