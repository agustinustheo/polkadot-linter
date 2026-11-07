# Run-based rule audit, polkadot-sdk plus 10 parachain repositories (2026-11)

This audit turned off `TST004`, `SEC007`, `SEC013`, `SEC014`, `SEC017`, `SEC018`
and `VAL002` by default. It extends the blind audit in
[`../2026-11-external`](../2026-11-external/README.md), which sampled the 10
external repositories but ran compiler rules on only 3 runtimes.

## Method

- **Build.** The PR #17 release binary and the `nightly-2025-09-01` driver, run
  with `config/default.toml` and `config/all-rules.toml`.
- **Syntax corpus.** The 10 repositories at the SHAs in
  [`../2026-11-external/manifest.tsv`](../2026-11-external/manifest.tsv), with
  Acala submodules excluded, plus polkadot-sdk at `b18fb34`
  (`substrate/frame`, `cumulus/parachains/runtimes`, `polkadot/runtime`).
- **Compiler corpus.** 8 runtimes that built: Astar, hydradx,
  asset-hub-polkadot, polkadot (relay), the parachain template,
  `kitchensink-runtime`, `asset-hub-westend-runtime` and `westend-runtime`.
  The SDK runtimes pull in most of `substrate/frame` as dependencies. Moonbeam,
  centrifuge, kilt, peaq and Acala failed to fetch or resolve dependencies.
  Findings are deduplicated by (file, line) across scans. Only files inside the
  scanned repositories count.
- **Sampling.** Up to 15 findings per rule, round-robin across corpora, with
  seed 20261107. Rules with fewer findings were labeled in full.
- **Labels** (`labels.tsv`, with a `file:line` relative to the repository and
  the evidence line):
  - **TP:** the finding is real and worth acting on.
  - **NOISE:** the pattern is present but not worth acting on. It is
    deliberate, documented, in dev, test or template code, or bounded by
    config.
  - **FP:** the rule's claim is false. The evidence cites the line that
    refutes it.
- `inventory.tsv` lists every rule with its engine, detection method, default
  after PR #17, and finding counts per corpus.

A second check agrees: an audited-commit benchmark of 56 auditor-reported bugs
(not published), run at the exact commits the auditors reviewed. None of the 7
rules caught any of those bugs. Their few hits in audited files were all labeled
NOISE.

## Rules turned off

| Rule | Findings | TP / labeled | Why |
| --- | ---: | ---: | --- |
| `TST004` | 96 syntax (89 in the SDK) | 0/15 (2 FP) | It asks for a `Pays::Yes` error-path test that no SDK or sampled pallet writes. 2 findings were private helpers, not extrinsics. |
| `SEC013` | 50 compiler | 0/15 | All 50 are in `#[pallet::without_storage_info]` pallets, a deliberate opt-out. FRAME already rejects values without `MaxEncodedLen` everywhere else. |
| `SEC017` | 24 compiler | 0/15 | Every sampled `Vec` is bounded elsewhere: config maximums, validator sets, `BoundedVec` inputs. 8 come from the `ping` demo pallet. |
| `VAL002` | 10 compiler | 0/10 | All divide by a `Get` config constant, where zero is a misconfiguration. |
| `SEC014` | 6 compiler | 0/6 | Every key is an internally assigned sequential index (session, authority, MMR node, asset index). |
| `SEC007` | 103 compiler | 2/15 | `let _ =` is already an explicit discard. 39/103 have a comment within 3 lines and 19/103 are in host crates. The rule skips closure bodies, so it misses dispatchables, which FRAME wraps in `with_storage_layer(\|\| ..)`. |
| `SEC018` | 24 compiler | 2/15 (2 FP) | Per-byte length fees already price the input, and the rule does not check for O(n) work in the body. |

## Rules kept (labeled in the same run)

| Rule | Findings | TP / labeled | Note |
| --- | ---: | ---: | --- |
| `BEN002` | 475 | 15/23 | Narrowing candidate: assertions inside the measured `#[block]` |
| `BEN003` | 290 | 9/15 (3 FP) | Narrowing candidate: benchmarks hosted in runtime crates |
| `TST006` | 137 | 17/23 (3 FP) | Narrowing candidate: events emitted by a called helper |
| `RTC001` | 189 | 4/15 | Narrowing candidate: dev, testnet and template runtimes |
| `RTC002` | 24 | 13/15 | |
| `RTC003` | 34 | 8/15 | 0 FP. Every NOISE finding is a dev or testnet runtime. |
| `RTC005` | 6 | 4/6 | |
| `RTC006` | 3 | 2/3 | |
| `SEC011` | 53 | 1/15 (10 FP) | 23/53 are in migrations, try-runtime hooks or asserts. Narrow it, then measure again. |
| `SEC012` | 8 | 6/8 | |
| `SEC006` | 2 | 0/2 | Too rare to matter |

`SEC010`, `SEC015`, `SEM009`, `SEM010`, `SEM011`, `SEM013`-`SEM016`, `TST002`
and `TRM001` had 0 findings on this corpus.

## Effect on default-config findings

Syntax counts come from a fresh `--syntax-only` run of this branch's release
binary. The run used the old defaults (`origin/main` `default.toml` with the 7
rules enabled) and the new `config/default.toml`. Compiler counts come from the
all-rules compiler scans above, filtered to the rules each default enables.

| Corpus | Before | After |
| --- | ---: | ---: |
| 10 external repositories, syntax | 765 | 758 |
| polkadot-sdk (`substrate/frame`, cumulus runtimes, `polkadot/runtime`), syntax | 512 | 423 |
| 8 runtimes, compiler (deduplicated) | 280 | 63 |
| **Total** | **1,557** | **1,244** |

Before the change, the 7 rules produced 313 default findings with 4 TPs in 91
labeled samples.

## Caveats

- One labeler judged each finding against the cited lines. The labels are not
  an oracle. Where a rule's claim can be checked (a pallet opts out, a bound
  exists, a function is not an extrinsic), the label is FP and cites the line.
- The compiler corpus leans toward the SDK, so it reflects the code Parity
  reviewers see most. Five external runtimes did not build.
- These rules can still catch real bugs: SEC007 found 2 burned-credit discards
  in Astar, and SEC018 found O(n)-body calls in node-authorization. Those are
  narrow patterns that would need dedicated rules. The rules stay available
  through `config/all-rules.toml` or `[rules.enabled]`.
