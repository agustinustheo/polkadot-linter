# 2026-11 acceptance test: paritytech/individuality-community

One serious, public Polkadot SDK project, linted with the bundled default
config, and every finding judged by whether an engineer would act on it.

- **Repository:** [paritytech/individuality-community](https://github.com/paritytech/individuality-community)
  at commit `58da05d`.
- **Command:** `polkadot-linter --syntax-only` from the repository root, bundled
  `config/default.toml` (no project config).
- **Before:** 39 findings on `main` at `ff42d33`.
- **After:** 3 findings, the three worth fixing.

## Standard

A finding is a true positive only if an engineer would fix it at reasonable
cost. "True but not worth fixing" counts against a rule like a false positive.
This is stricter than the earlier audits (`2026-11-external`,
`2026-11-run-audit`), which counted correct-but-low-value findings as true
positives and so labelled BEN002 and TST006 too leniently.

## Method

Three reviewers (Opus, Fable, Sol), working independently and told to be
skeptical, read each of the 39 findings in the source and gave one label:

- `fix`: correct and worth an engineer's time at reasonable cost.
- `not-worth-fixing`: correct, but not worth changing (testnet-only code,
  microbenchmarks, documented intent, internal maintenance steps).
- `wrong`: the claim is false (the benchmark exists, the event is emitted, the
  block does verify).

`labels.tsv` holds the majority label for each finding, each reviewer's vote
(in the order Opus/Fable/Sol) and a one-line reason. One finding had a
three-way split (BEN002 `coinage/src/benchmarking.rs:4611`); two of three
reviewers would not fix it, so it is labelled `not-worth-fixing`.

## Results

| Rule | Findings | fix | not worth fixing | wrong | Change in this PR |
|---|---|---|---|---|---|
| BEN003 | 12 | 0 | 3 | 9 | Off by default |
| BEN002 | 9 | 0 | 8 | 1 | Off by default |
| TST006 | 8 | 0 | 7 | 1 | Off by default |
| SEM011 | 3 | 0 | 3 | 0 | Skips root-only calls behind a `testnet` cfg feature |
| SEM013 | 2 | 0 | 2 | 0 | Off by default |
| RTC001 | 2 | 0 | 2 | 0 | `paseo` added to its test-network name tokens |
| RTC003 | 1 | 1 | 0 | 0 | Kept |
| RTC007 | 1 | 1 | 0 | 0 | Kept |
| RTC008 | 1 | 1 | 0 | 0 | Kept |
| **Total** | **39** | **3** | **25** | **11** | **39 → 3** |

BEN003 is off because 9 of its 12 findings were wrong. It does not follow
weight helpers (`Pallet::<T>::foo_weight(..)` that select benchmarked
`WeightInfo` functions), benchmarks that call the extrinsic inside a closure
built by a setup helper, or metered calls whose weight is a step budget
refunded from a `WeightMeter`. A wrong finding costs more than a missed one. A
later change may fix these cases and turn it back on.

Remaining findings after this change:

```
RTC008 runtimes/next-asset-hub-paseo/src/lib.rs:832
RTC007 runtimes/next-asset-hub-paseo/src/xcm_config.rs:340
RTC003 runtimes/next-people-paseo/src/xcm_config.rs:424
```

## Re-run

```bash
git clone https://github.com/paritytech/individuality-community /tmp/ic
git -C /tmp/ic checkout 58da05d
benchmarks/precision/2026-11-individuality-community/check.sh /tmp/ic
```

`check.sh` builds the linter, runs it with the default config, prints the
findings, and fails if there are more than 6 or if any of the three findings
worth fixing is missing.

## Caveats

- One repository. It is an acceptance test for noise, not a precision
  estimate; the per-rule counts are small.
- Labels are reviewer judgments, not an oracle. The reviewers agreed on 35 of
  39 findings.
- Turning rules off loses some true positives elsewhere; the PR that added
  this directory lists the counts on the other corpora.
