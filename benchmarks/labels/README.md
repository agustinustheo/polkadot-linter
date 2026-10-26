# Labeled findings

Ground-truth labels for source-rule findings on the pinned Polkadot SDK
checkout (`.repos/polkadot-sdk`, commit
`b18fb34a8ae348df5866e4b718d82871d744e60d`). Rule changes that claim to remove
false positives must be backed by these labels, and must not win by
suppressing true positives.

`scripts/check-labeled-findings.sh [SDK_DIR] [LABELS_DIR]` runs a syntax-only
scan over every SDK root the labels reference (`substrate/frame`, `cumulus`,
`polkadot/runtime`, `templates`) and checks every `*.tsv` file here. CI runs it
after the source-rule corpus baseline check.

## Format

One file per rule family or branch (`SEM.tsv`, `TST.tsv`, `VAL.tsv`, ...).
Tab-separated, with this exact header line:

```text
rule_id	path	line	label	evidence	note
```

- `path`: relative to the SDK checkout root, e.g.
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
- `note`: short justification, no tabs.

Only label what you verified against the pinned SDK source. Balance `FP`
labels for a rule with `TP` labels where real true positives exist in the
corpus; where none exist, keep a positive fixture in `tests/` as the recall
guard.
