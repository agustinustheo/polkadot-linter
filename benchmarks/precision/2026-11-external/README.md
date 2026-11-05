# Precision audit: 10 external parachain repositories (2026-11-06)

This audit decides which rules `config/default.toml` enables.

## Method

- Corpus: the 10 repositories in `manifest.tsv` (default branch, shallow clones).
  For Acala, git submodules were excluded from the scan.
- Build: `main` f56d36f, merged with PR #15 (RTC007, RTC008) and PR #16
  (SEC019, SEC020), using the bundled default configuration.
- Scan: the syntax engine covered all 10 repositories. The compiler-backed scan
  covered 3: `parachain-template-runtime`, `asset-hub-polkadot-runtime` and
  `hydradx-runtime`, plus the workspace crates they depend on. Per-rule totals
  are in `counts.tsv`.
- Sample: up to 12 findings per rule (all of them when a rule had 12 or fewer),
  picked round-robin across repositories with seed 20261106. That gave 291
  findings.
- Labels: two blind labelers (GPT 6.1 Sol and Opus 5.5) labeled every sampled
  finding. They agreed on 186. GPT 6.1 Terra adjudicated the other 105 and left
  4 SEC003 findings `UNSURE`. `labels.tsv` holds both labels, the final label and
  the evidence: the adjudicator's reason where the labelers disagreed, otherwise
  the Opus evidence. `final.tsv` holds only the final labels.

## Labels

- `TP`: the finding is real and worth acting on.
- `NOISE`: the pattern the rule describes is present, but it is not worth acting
  on. Examples: a deliberate choice, a dev or test runtime, a dispatch that is
  rolled back, a canonical wrapper.
- `FP`: the condition the rule claims does not hold. For example, the benchmark,
  the guard or the exclusion the rule says it looks for actually exists.
- `UNSURE`: the adjudicator could not decide.

## Decision rule

A rule is off by default if its sampled TP share is under 30%, or if its FP
share is 30% or more.

| rule | findings | n | TP | NOISE | FP | default |
|---|---|---|---|---|---|---|
| SEC003 | 10 | 10 | 0 | 0 | 6 (+4 unsure) | off: reads Ethereum ABI decoding as SCALE |
| SEC004 | 4 | 4 | 0 | 4 | 0 | off |
| SEC005 | 8 | 8 | 0 | 8 | 0 | off (was advisory) |
| SEC008 | 32 | 12 | 0 | 4 | 8 | off: fires on genesis/`qed` code it claims to exclude |
| SEC016 | 9 | 9 | 0 | 9 | 0 | off |
| SEM008 | 1699 | 12 | 0 | 12 | 0 | off (was advisory; 58% of all output) |
| TST001 | 26 | 12 | 0 | 5 | 7 | off: validity/XCM errors read as dispatch results |
| RTC004 | 32 | 12 | 1 | 9 | 2 | off |
| SEC002 | 31 | 12 | 1 | 10 | 1 | off |
| SEM005 | 12 | 12 | 1 | 10 | 1 | off |
| VAL003 | 17 | 12 | 1 | 11 | 0 | off |
| SEC009 | 18 | 12 | 2 | 3 | 7 | off |
| SEC001 | 4 | 4 | 1 | 0 | 3 | off |
| SEM006 | 50 | 12 | 3 | 7 | 2 | off |
| BEN001 | 160 | 12 | 4 | 4 | 4 | off: misses runtime-hosted benchmarks |
| VAL001 | 5 | 5 | 2 | 0 | 3 | off |
| RTC001 | 182 | 12 | 4 | 8 | 0 | on |
| SEC017 | 8 | 8 | 3 | 5 | 0 | on |
| SEC007 | 13 | 12 | 5 | 7 | 0 | on |
| TST006 | 41 | 12 | 5 | 4 | 3 | on |
| SEC011 | 7 | 7 | 3 | 2 | 2 | on |
| BEN003 | 215 | 12 | 6 | 3 | 3 | on |
| RTC002 | 14 | 12 | 8 | 4 | 0 | on |
| RTC003 | 34 | 12 | 8 | 4 | 0 | on |
| RTC005 | 6 | 6 | 4 | 2 | 0 | on |
| BEN002 | 248 | 12 | 10 | 2 | 0 | on |
| TST004 | 7 | 7 | 6 | 1 | 0 | on |
| RTC006 | 3 | 3 | 3 | 0 | 0 | on |
| RTC007 | 11 | 11 | 11 | 0 | 0 | on (PR #15) |
| RTC008 | 4 | 4 | 4 | 0 | 0 | on (PR #15) |
| SEC018 | 1 | 1 | 1 | 0 | 0 | on |

## Effect

The syntax scan was rerun on the same 10 repositories with `--syntax-only`,
using the release build of the commit that changed the defaults. Acala's
submodules were again excluded. RTC007 and RTC008 are not on `main`, so they
are not counted. The useful share is estimated as the sum over rules of
findings × the rule's sampled TP rate, divided by the total number of findings.

| default config | findings | estimated useful | useful share |
|---|---|---|---|
| before | 2,680 | ≈496 | 18.5% |
| after | 750 | ≈437 | 58.3% |

Most of the estimated useful findings that are lost (≈53 of ≈59) come from
BEN001.

## Caveats

- The compiler-backed rules were scanned on only 3 repositories, and 178 of
  their 212 findings came from hydration. The SEC, SEM006 and VAL003 decisions
  rest on that narrow base.
- Samples are small: 12 per rule at most. A rate from n=4 or n=5 is only a
  direction.
- Rules with zero findings on this corpus keep their previous default, because
  the audit gives no evidence either way. They are SEC006, SEC010, SEC012-SEC015,
  SEC019, SEC020, SEM009-SEM011, SEM013-SEM016, TRM001, TST002 and VAL002.
- The 8 rules that were already off by default since 2026-10 (SEM002, SEM003,
  SEM004, SEM007, SEM012, TST003, TST005, MOK001) were not sampled again.
- The labels are model judgments with an adjudication step, not an oracle.
  Near-identical runtimes, such as moonbeam/moonriver/moonbase, were not
  deduplicated.
