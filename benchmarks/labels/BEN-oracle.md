# BEN evidence and compiler oracle

Pinned SDK: `b18fb34a8ae348df5866e4b718d82871d744e60d`.

The compiler cross-check is `scripts/check-benchmark-expansion.sh`. Run it from
the repository root with Rust 1.93.0, Cargo, Bash, awk, diff, and jq installed:

```sh
scripts/check-benchmark-expansion.sh .repos/polkadot-sdk /tmp/ben-expansion
```

It expands the real membership, assets, and vesting crates using
`RUSTC_BOOTSTRAP=1 cargo +1.93.0 rustc --features runtime-benchmarks --lib -- -Zunpretty=expanded`.
It checks the SDK commit, compiler-created `SelectedBenchmark` variants, measured
dispatch mappings in `BenchmarkingSetup` implementations, and verification
branches after `recording.stop()`, against `BEN-oracle.json`. Expansion outputs
and expected/actual TSV files are preserved in the requested output directory.
The expected values in `BEN-oracle.json` are hand-written. The membership
extractor pattern-matches the authored `Membership::<...>::call` spelling in
expanded measured blocks; expansion does not resolve use aliases. Its seven
call mappings therefore require manual source checks. Assets and vesting use
generated Call constructors (and three dispatched local Call variants in assets).

All seven membership calls were read in `substrate/frame/membership/src/benchmarking.rs`:
`add_member` at L51, `remove_member` at L71, `swap_member` at L93,
`reset_members` at L114, `change_key` at L138, `set_prime` at L154, and
`clear_prime` at L168. Each measures `Membership::<T, I>::<same name>`;
`use super::{Pallet as Membership, *}` at L20 establishes the owning alias.
`BEN.tsv` has four membership labels (one BEN001 and three BEN003), all now
`manual`; the seven entries in the JSON are call mappings, not seven TSV labels.

Fresh expansions passed in round two: membership has seven v1 benchmarks,
assets has 39 v1 instance benchmarks, and vesting has nine v2 benchmarks. All 55
names and mappings agree with the hand-written expected fixture. Fifty measure dispatchables;
five assets definitions measure helpers and have empty dispatch mappings.
The assets create and vesting verification labels are backed by expanded
post-measurement `if verify` branches.

This is a **manual script**, not a CI step: cold expansion builds real SDK
benchmark dependencies in addition to the linter's existing driver CI workload.
Warm local execution took roughly seven seconds for the three packages, but
that does not represent a clean CI build budget. The extractor handles the
pinned compiler output, not arbitrary Rust syntax. A runtime/node
`benchmark pallet --list` was not built or run.

The former Python source inventory was removed. Its exclusion and terminal-call
heuristics shared blind spots with the implementation, so it was not independent
evidence. Labels supported by generated variants or verification branches retain
`oracle:macro-expansion`. Membership call ownership and other source-read
labels are `manual` and cite source locations and concrete facts.
Weight-generation headers are supporting provenance, not proof of coverage.

## Corpus counts

Counts are BEN001 / BEN002 / BEN003, with default syntax-only settings. Main
(`562eee4`) and round one (`336d3f2`) were separately compiled from their exact
committed sources and rescanned against the same pinned SDK.

| Scan root | Main | Round one | Round two |
| --- | --- | --- | --- |
| substrate/frame | 388 / 328 / 259 | 16 / 195 / 67 | 16 / 197 / 72 |
| cumulus/pallets | 0 / 19 / 11 | 0 / 6 / 10 | 0 / 6 / 10 |
| polkadot/runtime/parachains | 0 / 41 / 0 | 0 / 25 / 0 | 0 / 25 / 0 |

Round-two FRAME changes: two BEN002 findings in example libraries are restored;
eight BEN003 gaps in example libraries are restored; three nomination-pools
BEN003 FPs disappear because assertion arguments now expose the owning `Pools`
call. No other findings change. Empty v1 verification rejection has no additional
pinned-corpus impact. Companion discovery requires matching package names and declared ownership;
arbitrary nested packages containing "benchmark" are rejected.

Scan-scoped caching memoizes discovery by crate root and Cargo manifests by path.
On this machine, separately compiled debug binaries scanned FRAME BEN rules in
64.7s (round one) and 17.0s (round two); timings include source parsing and are
not a performance guarantee.

## Labels and limitations

`BEN.tsv` contains 46 labels: 34 FP / 12 TP (BEN001: 11/4; BEN002: 11/5;
BEN003: 12/3). Every row was re-read against pinned sources or fresh expansion,
then checked against the three round-two scans for exact rule/path/line presence
(TP) or absence (FP). New examples cover the restored library targets and the
three assertion-wrapped nomination-pools calls. Missing node-authorization
benchmarks and people weight functions remain real recall guards. Synthetic
fixtures cover both macro generations, wrong receivers, undispatched Calls,
local owning aliases, module ancestry, empty verification, test-directory scan
roots, custom Cargo examples, and cache refresh after source edits.

The shared `scripts/check-labeled-findings.sh` is absent on this branch and will
be supplied by PR #7. No competing checker was added. The temporary validation
loads the three JSON scans, keys findings by `(rule_id, SDK-relative path, line)`,
and asserts presence for TP and absence for FP.

Ownership is per Cargo crate, not per pallet. Multiple pallets in one crate can
share names; compiler-selected non-test cfg branches, arbitrary external
benchmark crate names, macro-generated modules/benchmarks, and helper indirection
remain limitations. Declared `mod` and `#[path]` ancestry is followed, retaining
runtime-benchmark gates and excluding test-only paths. A v2 benchmark without
measurement markers retains historical name/variant matching as a compatibility
fallback. Remaining FPs include intentionally skipped diagnostic benchmarks
(contracts `print_schedule` returns `BenchmarkError::Skip`) and custom verification
helpers absent from the configuration. The complete rustc-driver CI script suite
was not rerun for this source-rule change.
