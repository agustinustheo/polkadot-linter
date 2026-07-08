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

Expected work:

- evaluate `rustc`/Clippy-style analysis as the backend for hard rules
- resolve types, cfg gates, macro expansion, and call targets where needed
- reimplement high-noise rules that cannot be fixed reliably with `syn` alone

## Phase 3: Rebuild as a Focused FRAME Security Analyzer

Goal: replace general Rust-pattern security rules with a FRAME-specific
analysis model.

Expected work:

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
