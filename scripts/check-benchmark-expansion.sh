#!/usr/bin/env bash
# Manual compiler-expansion cross-check for the pinned SDK. This builds real SDK dependencies;
# it is deliberately separate from the fast source-rule CI corpus check.
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
OUTPUT_DIR="${2:-$ROOT_DIR/.benchmarks/benchmark-expansion}"
mkdir -p "$OUTPUT_DIR"
SDK_DIR="$(cd "$SDK_DIR" && pwd)"
EXPECTED="$ROOT_DIR/benchmarks/labels/BEN-oracle.json"
test "$(git -C "$SDK_DIR" rev-parse HEAD)" = "$(jq -r .sdk_commit "$EXPECTED")"
for pallet in membership assets vesting; do
  package="pallet-$pallet"
  expanded="$OUTPUT_DIR/$pallet-expanded.rs"
  RUSTC_BOOTSTRAP=1 cargo +1.93.0 rustc --manifest-path "$SDK_DIR/Cargo.toml" \
    -p "$package" --features runtime-benchmarks --lib -- -Zunpretty=expanded > "$expanded"
  # Read compiler-created SelectedBenchmark and BenchmarkingSetup impls. Strip
  # literals before counting braces. This extractor is pinned-output tooling,
  # not a source inventory or a general Rust parser.
  awk -v package="$package" '
    { gsub(/"([^"\\]|\\.)*"/, "\"\""); source = source " " $0 }
    END {
      if (!match(source, /enum SelectedBenchmark[[:space:]]*\{/)) exit 1
      enum = substr(source, RSTART + RLENGTH)
      sub(/\}.*/, "", enum)
      n = split(enum, names, /,/)
      for (i = 1; i <= n; i++) {
        gsub(/[[:space:]]/, "", names[i])
        if (names[i] != "") print "benchmark\t" names[i]
      }
      rest = source
      while (match(rest, /BenchmarkingSetup<[^>]*>[[:space:]]+for[[:space:]]+[[:alnum:]_]+/)) {
        header = substr(rest, RSTART, RLENGTH)
        name = header; sub(/.*for[[:space:]]+/, "", name)
        rest = substr(rest, RSTART + RLENGTH)
        start = index(rest, "{"); depth = 1
        for (end = start + 1; end <= length(rest) && depth; end++) {
          c = substr(rest, end, 1)
          if (c == "{") depth++
          if (c == "}") depth--
        }
        body = substr(rest, start, end - start)
        rest = substr(rest, end)
        if (name == "SelectedBenchmark") continue
        if (body !~ /recording.start\(\)/ || body !~ /recording.stop\(\)/) exit 2
        calls = body
        while (match(calls, /new_call_variant_[[:alnum:]_]+/)) {
          call = substr(calls, RSTART, RLENGTH); sub(/^new_call_variant_/, "", call)
          print "call\t" name "\t" call
          calls = substr(calls, RSTART + RLENGTH)
        }
        measured = body; sub(/.*recording.start\(\);/, "", measured); sub(/recording.stop\(\).*/, "", measured)
        if (package == "pallet-assets" && measured ~ /call.dispatch_bypass_filter\(/) {
          variants = body
          while (match(variants, /Call[[:space:]]*::[[:space:]]*<[^>]*>[[:space:]]*::[[:space:]]*[[:alnum:]_]+[[:space:]]*\{/)) {
            call = substr(variants, RSTART, RLENGTH); sub(/.*::[[:space:]]*/, "", call); sub(/[[:space:]]*\{$/, "", call)
            print "call\t" name "\t" call
            variants = substr(variants, RSTART + RLENGTH)
          }
        }
        # Membership retains the authored receiver spelling in expansion.
        # This pattern does not resolve the use alias; ownership is manual.
        if (package == "pallet-membership") {
          measured = body; sub(/.*recording.start\(\);/, "", measured); sub(/recording.stop\(\).*/, "", measured)
          while (match(measured, /Membership[[:space:]]*::[[:space:]]*<[^>]*>[[:space:]]*::[[:space:]]*[[:alnum:]_]+/)) {
            call = substr(measured, RSTART, RLENGTH); sub(/.*::[[:space:]]*/, "", call)
            print "call\t" name "\t" call
            measured = substr(measured, RSTART + RLENGTH)
          }
        }
        if (body ~ /recording.stop\(\);[[:space:]]*if verify[[:space:]]*\{/) print "verification\t" name
      }
    }
  ' "$expanded" | LC_ALL=C sort -u > "$OUTPUT_DIR/$pallet.actual.tsv"
  jq -r --arg package "$package" '
    .checks[] | select(.package == $package) |
    (.benchmarks[] | "benchmark\t" + .),
    (.measured_calls | to_entries[] | .key as $name | .value[] | "call\t" + $name + "\t" + .),
    (.verification[] | "verification\t" + .)
  ' "$EXPECTED" | LC_ALL=C sort -u > "$OUTPUT_DIR/$pallet.expected.tsv"
  diff -u "$OUTPUT_DIR/$pallet.expected.tsv" "$OUTPUT_DIR/$pallet.actual.tsv"
  echo "$package: compiler benchmark names, dispatch mappings, verification branches match"
done
