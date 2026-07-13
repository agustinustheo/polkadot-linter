# Security Linter Phases

This repository now follows a phased plan for improving security-rule quality.
The phases are intentionally sequential. We do not mix architecture work for
later phases into the current one.

## Phase 1: Stabilize (Complete)

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

## Phase 2: Add Semantic Infrastructure (Complete for Public SEC Rules)

Goal: move important rules off pure syntax matching and onto a typed semantic
layer.

Current implementation:

- the public CLI routes `SEC001` through `SEC018` and `VAL003` through the rustc driver
  when scanning a Cargo project
- the syntax engine no longer registers any migrated rule, so it cannot
  produce an alternate diagnostic for the same rule ID
- pinned SDK packages have per-rule rustc baselines in CI

Expected work:

- deepen interprocedural dataflow and FRAME-domain modeling where the current
  compiler rules document coverage limits
- retain syntax only for non-security rules that are inherently text, style, or
  source-attribute checks

See [`rustc-driver-analysis.md`](rustc-driver-analysis.md) for the current
backend and coverage matrix.

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

Use the per-rule rustc SDK baseline scripts to collect reproducible security
findings against the pinned checkout. CI runs these scripts directly.

Current pinned-corpus policy:

- `.repos/polkadot-sdk` is a recorded git submodule pinned to
  `b18fb34a8ae348df5866e4b718d82871d744e60d`
- CI checks out the submodule, verifies that exact commit, and compares each
  compiler-backed rule's normalized SDK output to its pinned baseline
- rules disabled in a project configuration are not sent to the rustc driver
- `SEC018` recognizes narrow accepted-path validators such as fixed-size
  signature verification, static statement equality, session-key decoding, and
  `OpaqueKeys::ownership_proof_is_valid` proof tuple validation
- production security rules skip SDK packages that are explicitly documented as
  test-support pallets, including `pallet-root-offences` and `cumulus-ping`
