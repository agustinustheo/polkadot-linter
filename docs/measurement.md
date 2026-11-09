# How rules are measured

A rule is on by default only when real-code evidence shows that its findings
are worth acting on. Three labeled audits set the defaults.

## Labels

| Label | Meaning |
| --- | --- |
| TP | The finding is real and worth acting on. |
| NOISE | The pattern is there, but it is not worth acting on. For example, a deliberate choice or a dev runtime. |
| FP | The finding is wrong. The condition the rule claims is not true. |

The acceptance test uses a stricter TP: an engineer would fix it at a
reasonable cost.

## The audits

| Audit | Code | Result |
| --- | --- | --- |
| [2026-11-external](https://github.com/agustinustheo/polkadot-linter/tree/main/benchmarks/precision/2026-11-external) | 10 third-party parachain repositories (Moonbeam, Astar, Acala, Hydration, ...). 291 sampled findings, two blind labelers and one adjudicator. | Rule turned off if TP < 30% or FP ≥ 30%. Default findings went from 2,680 to 750. The useful share went from 18.5% to 58.3%. |
| [2026-11-run-audit](https://github.com/agustinustheo/polkadot-linter/tree/main/benchmarks/precision/2026-11-run-audit) | polkadot-sdk and the same repositories. Compiler rules on 8 runtimes. | 7 more rules turned off: 4 TPs in 91 labeled samples. Default findings went from 1,534 to 1,221. |
| [2026-11-individuality-community](https://github.com/agustinustheo/polkadot-linter/tree/main/benchmarks/precision/2026-11-individuality-community) | One serious project, strict TP, three skeptical reviewers. | 3 of 39 default findings were worth a fix. After the changes, the project gets 3 findings: those 3. |

The rules that these audits turned off were later deleted. See
[Retired rules](retired-rules.md).

## Other checks

- **Auditor bugs.** We run the rules at the exact commits that security
  auditors reviewed. Then we check which reported bugs each rule finds.
- **Pinned SDK corpus.** CI scans a pinned polkadot-sdk commit. It compares the
  findings with labeled baselines in `benchmarks/`. A new finding or a lost
  finding fails CI.

## Limits

Labels are human (or model) judgments, not an oracle. Samples are small: 12 to
15 findings per rule. Each audit README lists its caveats.
