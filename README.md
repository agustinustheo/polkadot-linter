# Polkadot Linter

Polkadot Linter is an external Rust linter for Polkadot SDK codebases. It focuses on repeatable FRAME and runtime checks that sit above general-purpose tooling like `cargo fmt` and `cargo clippy`.

## Quick Start

```bash
cargo build --release

# Scan one or more directories
cargo run -- ../pallets
cargo run -- ../pallets ../runtimes ../support

# Use a project-level configuration
cargo run -- -c ../polkadot-linter.toml ../pallets

# Emit machine-readable output
cargo run -- ../pallets -f json
cargo run -- ../pallets -f sarif > polkadot-linter.sarif

# Tighten CI behaviour
cargo run -- ../pallets -s warning --fail-on-warning
```

## Rule Families

Polkadot Linter groups checks by family so teams can enable, disable, or filter them more easily:

| Prefix | Family |
| --- | --- |
| `VAL` | Validation order and guard rails |
| `SEM` | Semantic and style checks |
| `TST` | Test quality checks |
| `MOK` | Mock usage heuristics |
| `BEN` | Benchmark coverage and verification |
| `TRM` | Terminology and text conventions |
| `SEC` | Security-focused rules |

## Rule Matrix

This matrix maps each diagnostic code to the rule it represents and the kind of issue it flags.

| Code | Family | Name | Severity | What it entails |
| --- | --- | --- | --- | --- |
| `VAL001` | Validation | `validation-before-heavy-read` | `warning` | Expensive storage reads happen before cheap precondition checks such as `ensure!` or bounds checks. |
| `VAL002` | Validation | `division-without-zero-guard` | `warning` | Division uses a config or runtime-derived value without first proving the divisor cannot be zero. |
| `VAL003` | Validation | `storage-write-before-validation` | `warning` | Storage is mutated before all validations finish, which can leave unnecessary writes or rollback risk. |
| `SEM002` | Semantic | `prefer-collect-turbofish` | `advisory` | `iter.collect()` uses a separate type annotation instead of `.collect::<Vec<_>>()`. |
| `SEM003` | Semantic | `prefer-ref-iteration` | `advisory` | A `for` loop uses `.iter()` where borrowing the collection directly would be more idiomatic. |
| `SEM004` | Semantic | `no-wildcard-imports` | `warning` | Production code uses wildcard imports such as `use foo::*`. |
| `SEM005` | Semantic | `parameterise-weight-functions` | `warning` | Weight functions multiply a constant weight instead of calling a parameterised `WeightInfo` function. |
| `SEM006` | Semantic | `dbweight-missing-pov` | `warning` | `DbWeight::get().reads()` is used where proof size accounting should come from a benchmarked weight. |
| `SEM007` | Semantic | `runtime-debug-deprecated` | `warning` | Deprecated `RuntimeDebug` or `RuntimeDebugNoBound` derives are used. |
| `SEM008` | Semantic | `sp-std-deprecated` | `warning` | Code still depends on deprecated `sp_std` instead of `alloc`-based `no_std` imports. |
| `SEM009` | Semantic | `redundant-contains-key-before-remove` | `advisory` | A storage map is checked with `contains_key()` immediately before `remove()` or `take()`. |
| `SEM010` | Semantic | `xor-as-exponentiation` | `error` | `^` is used as if it were exponentiation, even though Rust treats it as bitwise XOR. |
| `SEM011` | Semantic | `weight-zero-placeholder` | `warning` | A pallet weight attribute uses `Weight::zero()`, making the call effectively free. |
| `SEM012` | Semantic | `allow-dead-code-in-pallet` | `warning` | Production pallet code suppresses dead-code warnings instead of removing unused items. |
| `TST001` | Test smell | `prefer-assert-noop` | `warning` | Tests manually check for failure instead of using `assert_noop!`, which also verifies storage is unchanged. |
| `TST002` | Test smell | `apply-extrinsic-assert-ok` | `error` | `assert_ok!` is used on `apply_extrinsic`, which can hide the inner `DispatchError`. |
| `TST003` | Test smell | `imports-inside-closures` | `advisory` | `use` imports appear inside functions or closures instead of at module scope. |
| `TST004` | Test smell | `pays-yes-error-path` | `advisory` | A `Pays::No` success path lacks a companion test proving the failing path still pays fees. |
| `TST005` | Test smell | `implementation-detail-assertions` | `advisory` | Tests assert on likely internal fields such as `.inner` or `.state` instead of observable behaviour. |
| `TST006` | Test smell | `extrinsic-without-event` | `advisory` | A storage-mutating extrinsic does not emit an event for external observers. |
| `MOK001` | Mock usage | `excessive-mock-setup` | `warning` | Test setup is dominated by mocks and scaffolding relative to actual assertions. |
| `BEN001` | Benchmark | `benchmark-for-weight-function` | `warning` | A weight function exists without a matching benchmark. |
| `BEN002` | Benchmark | `benchmark-verification` | `warning` | A benchmark lacks a verification step that proves the benchmarked operation really happened. |
| `BEN003` | Benchmark | `extrinsic-without-benchmark` | `warning` | A dispatchable call has no matching benchmark in the sibling benchmarking file. |
| `TRM001` | Terminology | `spelling-conventions` | `advisory` | Comments, docs, strings, or configured identifiers violate the project’s spelling dictionary. |
| `SEC001` | Security | `unbounded-vec-in-extrinsic` | `warning` | An extrinsic accepts an unbounded `Vec<T>` parameter that can grow without limit. |
| `SEC002` | Security | `debug-assert-in-production` | `warning` | Production runtime code relies on `debug_assert!`-style checks. |
| `SEC003` | Security | `missing-decode-depth-limit` | `warning` | User-controlled decoding happens without `decode_with_depth_limit`. |
| `SEC004` | Security | `unsafe-weight-arithmetic` | `warning` | A weight attribute uses non-saturating arithmetic that can overflow or undercount. |
| `SEC005` | Security | `expensive-weight-calculation` | `warning` | Weight calculation performs storage reads, encoding, or other expensive work before dispatch. |
| `SEC006` | Security | `unchecked-repatriate-reserved` | `warning` | The remaining balance from `repatriate_reserved` is ignored instead of being checked. |
| `SEC007` | Security | `let-underscore-result` | `warning` | `let _ =` discards a likely `Result`, hiding potential failures. |
| `SEC008` | Security | `panic-in-production` | `warning` | Production code uses panic paths such as `unwrap`, `expect`, `panic!`, or `todo!`. |
| `SEC009` | Security | `raw-arithmetic-in-fallible` | `advisory` | Fallible functions use raw arithmetic operators where overflow handling should be explicit. |
| `SEC010` | Security | `missing-transactional-in-hook` | `warning` | A hook performs multiple storage writes without a transactional storage layer. |
| `SEC011` | Security | `unbounded-storage-iteration` | `warning` | Dispatchable or hook code iterates storage in a state-size-dependent way. |
| `SEC012` | Security | `unbounded-clear-prefix` | `warning` | Trie cleanup uses an effectively unbounded `clear_prefix` or related call. |
| `SEC013` | Security | `unbounded-storage-collections` | `warning` | Storage types use unbounded collections without explicitly marking them as unbounded. |
| `SEC014` | Security | `identity-hasher-on-common-keys` | `warning` | `Identity` hashing is used with common key types that should not be stored unhashed. |
| `SEC015` | Security | `dispatch-bypass-filter` | `warning` | Production code calls `.dispatch_bypass_filter()`, bypassing the runtime filter. |
| `SEC016` | Security | `unguarded-migration` | `warning` | `on_runtime_upgrade` writes storage without guarding on storage version checks. |
| `SEC017` | Security | `unbounded-vec-in-event` | `warning` | A pallet event carries an unbounded `Vec<>` payload. |

## Configuration

The default configuration lives in [`config/default.toml`](config/default.toml). A project can override it with a `polkadot-linter.toml` file:

```toml
[rules.enabled]
BEN002 = false
VAL001 = false

[rules.severity]
TST002 = "error"

[validation_order]
heavy_operations = ["::get(", "::iter(", "::contains_key("]
cheap_validations = ["ensure!", ".is_empty()", ".is_zero()"]

[terminology.british_english]
"optimisation" = "optimization"
```

### Configuration Notes

- [`polkadot-linter.toml`](polkadot-linter.toml) is intentionally written as a compact project-level example. It shows how to disable noisy rules temporarily, promote specific rules, and tune family-specific heuristics.
- The disabled entries under `[rules.enabled]` represent accepted technical debt in a target codebase. The expected workflow is to re-enable those rules incrementally as violations are fixed.
- `[rules.severity]` lets you promote or demote individual rules without changing the implementation. The sample config keeps `TST002` at `error` because it can hide a real dispatch failure in tests.
- `[validation_order]`, `[test_smells]`, `[mock_usage]`, and `[benchmarking]` tune the heuristics behind rule families. Most teams will only need to touch these when their local conventions differ from the defaults.
- `[terminology.british_english]` and `[terminology.forbidden_terms]` are meant to be customized per project. Keep only the spellings and term replacements that your team actually wants enforced.

The full schema and defaults remain in [`config/default.toml`](config/default.toml).

## Output Formats

- `human`: coloured terminal output with file locations and suggestions
- `json`: machine-readable diagnostics
- `sarif`: SARIF 2.1.0 output for CI and code scanning

## Project Layout

```text
src/
  lib.rs
  main.rs
  config.rs
  diagnostics.rs
  engine.rs
  rules/
tests/
  rules_test.rs
  fixtures/
config/
  default.toml
polkadot-linter.toml
```

## Adding a Rule

1. Implement `LintRule` in the appropriate module under `src/rules/`.
2. Register it in `src/rules/mod.rs`.
3. Add `bad_*.rs` and `good_*.rs` fixtures under `tests/fixtures/`.
4. Extend `tests/rules_test.rs`.

## Tooling Relationship

Polkadot Linter is intended to complement standard Rust tooling, not replace it. A typical run order in CI is:

```text
cargo fmt -> cargo clippy -> polkadot-linter
```

## License

This project is licensed under [The Unlicense](LICENSE).
