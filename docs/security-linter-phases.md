# Security Linter Phases

This repository now follows a phased plan for improving security-rule quality.
The phases are intentionally sequential. We do not mix architecture work for
later phases into the current one.

## Phase 1: Stabilize

Goal: make the current `syn`-based tool usable without changing its core
architecture.

Scope:

- tighten rules that already have measurable signal
- suppress obvious false positives from privileged origins, benchmark code,
  test utilities, and similar non-audit paths
- add new high-signal rules when the finding class is concrete and benchmarked
- add a repeatable benchmark harness against pinned `polkadot-sdk` snapshots
- keep low-signal rules quarantined instead of expanding them

Exit criteria:

- the focused ruleset stays low-volume on `polkadot-sdk`
- new rules are backed by regression tests and corpus runs
- the benchmark harness can summarize SEC findings by rule and by file

Non-goals:

- typed analysis
- macro expansion or call-target resolution
- a new rule engine or domain IR

## Phase 2: Add Semantic Infrastructure

Goal: move important rules off pure syntax matching and onto a typed semantic
layer.

Current implementation:

- an opt-in rustdoc JSON backend can be supplied with `--rustdoc-json`
- the first prototype migrates `SEC013` storage value-shape detection onto
  compiler-resolved type aliases
- the default fast scanner remains unchanged while typed rules are validated
  rule by rule

Expected work:

- add a cargo/rustc-backed project model so scans understand declared target
  roles even when file paths do not follow `tests/` or `benches/` conventions
- evaluate `rustc`/Clippy-style analysis as the backend for hard rules
- resolve types, cfg gates, macro expansion, and call targets where needed
- reimplement high-noise rules that cannot be fixed reliably with `syn` alone

See [`rustdoc-analysis.md`](rustdoc-analysis.md) for the current backend,
migration plan, and compatibility story.

## Phase 3: Rebuild as a Focused FRAME Security Analyzer

Goal: replace general Rust-pattern security rules with a FRAME-specific
analysis model.

Expected work:

- introduce a FRAME domain model for pallet dispatchables, origins, and weight
  attributes instead of re-parsing those concepts independently inside each rule
- define a domain model for dispatchables, origins, weights, storage, hooks,
  events, and migrations
- rewrite the retained security rules around that model
- treat precision and corpus-benchmark stability as release gates

## Benchmark Harness

Use the phase-1 benchmark harness to collect SEC finding volume against the
pinned SDK checkout:

```bash
scripts/benchmark-sec-rules.sh
scripts/benchmark-sec-rules.sh .repos/polkadot-sdk .benchmarks
```

The script writes:

- raw JSON findings
- a text summary with counts by rule and top files

Current pinned-corpus policy:

- `.repos/polkadot-sdk` is a recorded git submodule pinned to
  `b18fb34a8ae348df5866e4b718d82871d744e60d`
- CI checks out the submodule, verifies that exact commit, runs this benchmark,
  and fails if the SEC finding count rises above the curated ceiling
- CI also compares the raw benchmark output against
  `benchmarks/polkadot-sdk-sec018-baseline.tsv`, so a different finding set
  fails even if the count stays unchanged
- `SEC012` and `SEC013` remain implemented and unit-tested, but are disabled in
  the project config because the SDK benchmark report showed they are still too
  noisy for default audit output
- the default SEC benchmark is currently focused on `SEC018`, with obvious
  non-findings filtered out before diagnostics are emitted
- `SEC018` recognizes narrow accepted-path validators such as fixed-size
  signature verification, static statement equality, session-key decoding, and
  `OpaqueKeys::ownership_proof_is_valid` proof tuple validation
- production security rules skip SDK packages that are explicitly documented as
  test-support pallets, including `pallet-root-offences` and `cumulus-ping`
