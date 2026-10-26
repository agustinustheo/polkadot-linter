# BEN evidence and independent oracle

Pinned SDK: `b18fb34a8ae348df5866e4b718d82871d744e60d`.

The independent inventory is `scripts/benchmark-source-oracle.py`. It uses Python
regexes and indentation boundaries, not the Rust linter, `syn`, or the linter's
macro token parser. It records the source package, benchmark name, definition
location, and terminal calls in the measured section. Simple dispatched `Call`
bindings are resolved separately with regexes. Ownership of companion crates in
labels was additionally checked against the SDK Cargo manifests.

Compiler expansion was feasible without building a full node/runtime. On this
Mac, expansion of membership took about one minute including shared dependencies;
assets and vesting then took seconds. Reproduce from the repository root:

```sh
SDK=.repos/polkadot-sdk
for pallet in membership assets vesting; do
  RUSTC_BOOTSTRAP=1 cargo +1.93.0 rustc --manifest-path "$SDK/Cargo.toml" \
    -p "pallet-$pallet" --features runtime-benchmarks --lib -- \
    -Zunpretty=expanded > "/tmp/$pallet-expanded.rs"
  python3 scripts/benchmark-source-oracle.py "$SDK" \
    --expanded "/tmp/$pallet-expanded.rs" --package "pallet-$pallet"
done
```

`BEN-oracle.json` preserves the cross-check results. All names in the independent
inventory exactly match the compiler's generated `SelectedBenchmark` enums:

- membership: 7 v1 benchmarks;
- assets: 39 v1 instance-pallet benchmarks;
- vesting: 9 v2 benchmarks, including const generics and renamed dispatch calls.

Dispatch mappings also agree for all 55 definitions. Of these, 50 definitions
measure dispatchables; five assets benchmarks measure non-dispatchable helpers
and have empty dispatch mappings. The comparison reads generated call variant
constructors and calls between `recording.start()` and `recording.stop()`, then
restricts both sides to the compiler's generated dispatchable constructors.
Verification evidence for the labeled vesting/assets cases was also checked in
the expanded `if verify` sections after `recording.stop()`.

Weights headers provide supporting provenance only. For example, assets'
`weights.rs` identifies the FRAME benchmark CLI that generated it. Header claims
alone were not used as proof that a current source benchmark exists.

## Corpus counts and delta

Counts are in BEN001 / BEN002 / BEN003 order, with default syntax-only settings.
The original source from main `562eee4` was compiled separately for the before
scans. The after scans were also run over each whole root; BEN-only scans were
used to validate the labels.

| Scan root | Before | After |
| --- | --- | --- |
| substrate/frame | 388 / 328 / 259 | 16 / 195 / 67 |
| cumulus/pallets | 0 / 19 / 11 | 0 / 6 / 10 |
| polkadot/runtime/parachains | 0 / 41 / 0 | 0 / 25 / 0 |

Removed FRAME findings, assigning overlapping causes to one primary bucket:

| Rule | Removed findings by cause | Newly reported |
| --- | --- | --- |
| BEN001 | 160 v1 discovery; 81 benchmark modules/submodules; 58 companion crates; 66 test/example contexts; 7 inline state-trie-migration benchmarks | 0 |
| BEN002 | 110 const-generic signatures; 55 full-body/qualified/generic verification recognition; 11 test/example contexts; 1 module helper mistaken for a benchmark | 44 |
| BEN003 | 129 v1 discovery/measured calls; 33 companion crates; 7 benchmark modules/submodules; 7 measured-call mappings; 8 test/example contexts; 8 rustdoc examples | 0 |

BEN002's 44 newly reported definitions include v1 benchmarks previously missed,
benchmarks outside files named `benchmarking`, and cases whose assertions are
only in setup or measured code. They are not all independently adjudicated TPs;
the ledger records verified examples, including v1 bags-list and v2 beefy-mmr.

## Labels and limitations

`BEN.tsv` contains 31 FP and 10 TP labels (BEN001: 11/4; BEN002: 11/4;
BEN003: 9/2). Missing node-authorization benchmarks and people weight functions
provide real BEN001/BEN003 recall guards. The appended test block in
`tests/rules_test.rs` also has positive fixtures for every BEN rule in both v1 and
v2 syntax, including missing benchmarks beside valid unrelated benchmarks.

The regex inventory is not a compiler: it assumes conventional indentation and
single-line benchmark attributes, does not evaluate cfg predicates, resolve
arbitrary macros/helper indirection, or establish Cargo ownership itself. It is
cross-checked against expansion only for the three packages above, not the full
SDK. A runtime/node CLI `benchmark pallet --list` was not built or run.

Remaining known FP patterns include intentionally diagnostic/skipped benchmarks
(e.g. contracts' `print_schedule`, which returns `BenchmarkError::Skip`), custom
verification helpers absent from `verification_patterns`, arbitrary benchmark
crate names outside the recognized companion convention, and measured calls
hidden behind helper indirection. Source discovery also cannot prove that every
file under `src/` is wired into the compiled module tree. A v2 benchmark without
measurement markers retains the historical name/variant matching fallback;
explicitly measured calls take precedence.

The shared `scripts/check-labeled-findings.sh` was absent on this branch. A
temporary independent Python check loaded the three BEN scan JSON files, keyed
findings by `(rule_id, SDK-relative path, line)`, and asserted presence for TP and
absence for FP. All 41 labels passed. The PR body includes the check recipe.
