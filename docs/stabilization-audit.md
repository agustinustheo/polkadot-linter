# Stabilization Audit

This audit records the current stopping point for the Phase 1 false-positive
reduction work on `fix/implementation-bugs-false-positives`.

## Current Corpus State

The unrestricted SEC scan against the pinned `polkadot-sdk` checkout at
`b18fb34a8ae348df5866e4b718d82871d744e60d` currently emits 348 diagnostics:

| Rule | Findings |
| --- | ---: |
| `SEC001` | 7 |
| `SEC002` | 70 |
| `SEC006` | 1 |
| `SEC007` | 2 |
| `SEC008` | 45 |
| `SEC009` | 142 |
| `SEC011` | 1 |
| `SEC012` | 8 |
| `SEC013` | 45 |
| `SEC016` | 2 |
| `SEC017` | 12 |
| `SEC018` | 13 |

The focused CI benchmark remains pinned to the validated `SEC018` baseline.
The broader unrestricted scan is used during stabilization to prove each
increment removes intended records only and adds no new findings.

## Fixed During Stabilization

The current branch has reduced the initial benchmark noise substantially by
adding narrow, evidence-backed handling for:

- non-production paths, test helpers, benchmarks, proc macros, and cfg-gated
  code
- privileged or bounded `Vec` dispatchable inputs
- bounded event and storage payloads
- versioned or documented migration paths
- documented panic/debug-assert invariants
- exact SDK invariant clusters where source context proves the diagnostic is
  not a production error-handling path
- validated `SEC018` weight/input accounting findings and accepted-path
  validators

Each committed suppression is paired with regression tests that include
near-miss cases, and each SDK-corpus increment was checked with a normalized
finding diff.

## Remaining Finding Classes

Some remaining diagnostics are still good candidates for narrow Phase 1 fixes
when the source context is exact and stable. Examples include isolated
`SEC002` invariant clusters with clear comments and function-local proof.

The following classes should not keep accumulating token-level suppressions:

- `SEC009` raw arithmetic in fallible functions. Precision requires type
  information, operator trait resolution, and stronger dataflow around bounds
  and checked/saturating alternatives.
- `SEC013` unbounded storage collections. Precision requires resolved storage
  aliases and value-type structure after FRAME macro expansion.
- `SEC008` and `SEC002` reachability. Precision requires cfg/macro expansion,
  target role, and control-flow context rather than string matching alone.
- `SEC003` decode-depth analysis. Precision requires resolved decoded types and
  whether the decoded input is user-controlled or recursive.
- `SEC018` weight/input accounting beyond the current validated patterns.
  Precision requires dispatchable extraction, parameter-to-weight dataflow, and
  resolved helper calls.

Those rules should move to a rustc-backed or Clippy-style analyzer before they
are treated as audit-grade.
