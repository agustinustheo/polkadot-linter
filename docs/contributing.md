# Contributing

## Project layout

| Path | Contents |
| --- | --- |
| `src/main.rs` | CLI, engine selection, exit codes |
| `src/engine.rs` | Syntax engine: file walk and rule dispatch |
| `src/rules/` | Syntax rules by family. `vulnerable_crates.tsv` is the RTC009 table. |
| `src/bin/polkadot-linter-driver.rs` | The rustc driver and the compiler rules |
| `src/rustc_pipeline.rs` | Runs `cargo check` with the driver |
| `src/suppress.rs` | Allow comments, LNT001 and baselines |
| `config/default.toml`, `config/all-rules.toml` | Bundled defaults; every rule on |
| `benchmarks/` | Precision audits, labeled findings and pinned-SDK baselines |
| `scripts/` | Corpus and driver checks that CI runs |
| `docs/` | This mdBook |

## Which engine owns a rule

Each rule has one engine. The engine is the one with the strongest evidence for
that rule.

- **Compiler engine:** all SEC rules, SEM009, SEM010 and SEM016. They need
  resolved types, trait impls or expanded macros.
- **Syntax engine:** RTC001-RTC008 and RTC010 read authored runtime config.
  Macro expansion removes some of it, for example `define_benchmarks!`
  members. SEM011, SEM015, SEM019 and TST002 read authored attributes
  and source shape. TRM001 reads comments and strings. RTC009 reads
  `Cargo.lock`.

Move a rule to the other engine only when a benchmark shows better precision
or recall. Add regression cases for the released CLI path first.

## Add a rule

1. Find a real incident or audit finding. Write down the exact bug shape.
2. Implement the rule in `src/rules/` (syntax) or in the driver (compiler).
3. Register its ID in `src/rules/mod.rs`.
4. Add bad and good fixtures, or driver checks in `scripts/`.
5. Measure it on real code with the [labels](measurement.md). Turn it on by
   default only if its findings are worth a fix.
6. Add `docs/rules/<ID>.md` with the same sections as the other rule pages.
   Add it to `docs/SUMMARY.md`, `docs/rule-index.md` and its family page.

## Checks

```bash
cargo +1.93.0 test
cargo +1.93.0 clippy --all-targets -- -D warnings
cargo +nightly-2025-09-01 build --features rustc-driver --bin polkadot-linter-driver
mdbook build
```

CI also scans a pinned polkadot-sdk commit and compares the results with the
baselines and labels in `benchmarks/`.

## Release

1. Set the version in `Cargo.toml`.
2. Push a tag `v<version>`.

The release workflow checks that the tag matches `Cargo.toml`. It then runs
fmt, clippy and tests, does a `cargo publish --dry-run`, and publishes to
crates.io. The workflow reads the crates.io token from the
`CARGO_REGISTRY_TOKEN` Actions secret. Do not put a token in the repository.
