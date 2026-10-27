# Labeled findings

Ground-truth labels for source-rule findings on the pinned Polkadot SDK
checkout (`.repos/polkadot-sdk`, commit
`b18fb34a8ae348df5866e4b718d82871d744e60d`). Rule changes that claim to remove
false positives must be backed by these labels, and must not win by
suppressing true positives.

`scripts/check-labeled-findings.sh [SDK_DIR] [LABELS_DIR]` runs a syntax-only
scan over every SDK root the labels reference (`substrate/frame`,
`substrate/bin/node/runtime`, `cumulus`, `polkadot/runtime`, `polkadot/xcm`,
`templates`) and checks every `*.tsv` file here. CI runs it after the
source-rule corpus baseline check, preceded by
`scripts/check-labeled-findings.sh --self-test`.

The checker fails when a scanned root produces no findings at all, when a
label file has no rows, or when a row names an unknown rule, a compiler-backed
rule (those never run under `--syntax-only`), a non-canonical path (`./`,
`../`), a line past the end of the file, or a `(rule_id, path, line)` already
labeled in any file.

## Format

One file per rule family or branch (`SEM.tsv`, `TST.tsv`, `VAL.tsv`, ...).
Tab-separated, with this exact header line:

```text
rule_id	path	line	label	evidence	note
```

- `rule_id`: a rule documented in `docs/rules/` that the syntax-only scan runs.
- `path`: canonical and relative to the SDK checkout root, e.g.
  `substrate/frame/assets/src/functions.rs`.
- `line`: the line the linter reports (or should report, for `TP`).
- `label`: `FP` — the linter must not report `rule_id` at `path:line`; `TP` —
  it must.
- `evidence`:
  - `oracle:<name>` — derived from an independent source of truth (compiler
    output, macro expansion, a type signature, benchmark CLI `--list`, a rule
    doc's explicit exemption text). Name the oracle.
  - `manual` — someone read the code; the note must cite the concrete fact
    (e.g. "ensure! at L737 reads amount2_optimal computed from L725 read").
  - `scope:<reason>` — the location is outside the rule's documented scope
    (e.g. a dev-only runtime), so it must not be reported. Only valid with
    label `FP`.
- `note`: short justification, no tabs.

Only label what you verified against the pinned SDK source. Balance `FP`
labels for a rule with `TP` labels where real true positives exist in the
corpus; where none exist, keep a positive fixture in `tests/` as the recall
guard.
