# Configuration

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
