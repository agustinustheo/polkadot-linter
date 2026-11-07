---
name: polkadot-linter
description: Lint Polkadot SDK, FRAME pallet, runtime and XCM Rust code with the `polkadot-linter` binary, then triage each finding (fix it, or add an inline allow with a reason). Use whenever you write, change or review code in a Polkadot SDK, Substrate, FRAME, parachain or runtime repository (pallets, `impl Config for Runtime`, `construct_runtime!`, XCM config, benchmarks), or when asked to lint, audit or check such code.
---

# Polkadot SDK linting with polkadot-linter

`polkadot-linter` finds Polkadot SDK bugs that `cargo clippy` cannot see:
unsafe XCM barriers, ignored proof checks, vulnerable crates, missing benchmarks.

## When to run it

After you change pallets, runtime config, XCM config or benchmarks, before you
open a PR in a Polkadot SDK repository, and when the user asks for a lint,
audit or review of such code.

## Install

```bash
cargo install polkadot-linter          # syntax and Cargo.lock rules
polkadot-linter --version
```

The `SEC` rules and a few others need the compiler-backed driver:

```bash
rustup toolchain install nightly-2025-09-01 --component rustc-dev --component llvm-tools-preview
cargo +nightly-2025-09-01 install polkadot-linter --features rustc-driver
```

Then pass `--driver-path ~/.cargo/bin/polkadot-linter-driver`. If the driver
is missing, the run still exits `0` but prints
`Compiler-backed analysis skipped ...` on stderr. Then no `SEC` rule ran:
say so in your report.

## Commands

```bash
polkadot-linter --syntax-only -f json .                 # quick pass, no build
polkadot-linter -f json --no-progress \
  --driver-path ~/.cargo/bin/polkadot-linter-driver \
  --manifest-path runtime/Cargo.toml runtime            # full pass, builds the crate
polkadot-linter --syntax-only -f json --rules RTC,SEC020 .   # some families or rule IDs
polkadot-linter -c config/all-rules.toml -f json .      # every rule; file from the linter repo
```

Start with `--syntax-only`. Run the full pass before a PR; the crate must
build. Use `-c config/all-rules.toml` only when the user asks: the extra rules
are off because they were measured as noisy. Do not edit the project's
`polkadot-linter.toml` to silence findings.

## Reading the JSON

stdout is a JSON array (`[]` when clean). A summary line goes to stderr.

```json
{"rule_id": "RTC001", "rule_name": "placeholder-weight-info-in-runtime",
 "category": "runtimeconfig", "severity": "warning",
 "file": "./runtime/src/lib.rs", "line": 3, "column": null, "end_line": null,
 "message": "what is wrong", "explanation": "why it matters",
 "suggestion": "how to fix it"}
```

`severity` is `error`, `warning` or `advisory`. `column` and `end_line` may be
`null`. Syntax rules give `file` relative to the current directory. The driver
gives it relative to the Cargo workspace root.

## Triage each finding

1. Read the rule page: `docs/rules/<ID>.md` in the linter repository, or
   `https://agustinustheo.github.io/polkadot-linter/rules/<ID>.html`.
2. Read the flagged code and enough context to understand it.
3. Decide:
   - **Real problem:** fix the code. Re-run the linter to confirm.
   - **Intentional or does not apply:** add an inline allow with a concrete
     reason (below).
   - **Not sure:** ask the human. Do not guess.

- Never add an allow without a concrete reason. "false positive" or "ok" is
  not a reason; say why the code is safe.
- Never suppress `SEC` or `RTC` findings just to make the output clean. These
  are security findings. Fix them or ask.
- A human reviews every fix and every allow in the PR. List them in the PR
  description.

## Inline allow

```rust
// polkadot-linter: allow(RTC001, reason = "dev runtime, never deployed")
type WeightInfo = pallet_xcm::TestWeightInfo;

type WeightInfo = (); // polkadot-linter: allow(RTC001, reason = "...")
```

- At the end of a line, it covers that line.
- On its own line, it covers the next statement. It skips blank lines,
  comments, doc comments and `#[...]` attributes. It covers lines until one
  ends with `;` or `,`, or has a `{` or `}`. So above an `impl` or
  `parameter_types! {` it covers only the header line: put it next to the
  finding.
- List several rules with `allow(RTC001, SEC020, reason = "...")`.
- `reason` is required. Rule `LNT001` reports allows with no reason or an
  unknown rule ID (these suppress nothing), and unused allows. Fix those too.
- Allows work only in `.rs` files. For `Cargo.lock` findings (`RTC009`),
  update the crate.

## Baseline (adopting the linter on an existing codebase)

```bash
polkadot-linter --write-baseline polkadot-linter-baseline.json .  # record all current findings, exit 0
polkadot-linter --baseline polkadot-linter-baseline.json .        # report only new findings
```

- Only write a baseline when the user asks to adopt the linter on old code.
  Do not rewrite it to hide findings you introduced.
- A finding is matched by rule ID, file path relative to the current
  directory, and the text of the flagged line (not the line number). Run from
  the same directory every time. Editing the flagged line makes it new again.
- Commit the baseline. Shrink it over time. Allows and the baseline need a
  version whose `--help` lists `--baseline`.

## Exit codes

`0` no blocking findings · `1` at least one `error`, or a `warning` with
`--fail-on-warning` · `2` usage error (bad or missing config file), or the
crate failed to build in the full pass. Findings hidden by an allow or the
baseline do not affect the exit code.
