#!/usr/bin/env bash

# Runtime-configuration (RTC) corpus check against the pinned Polkadot SDK.
#
# 1. Scans the SDK runtime roots with the syntax-only engine and compares the
#    per-root RTC finding counts with benchmarks/polkadot-sdk-source-runtime-baseline.tsv.
# 2. Checks every RTC row of benchmarks/labels/RTC.tsv: TP rows must be
#    reported at path:line, FP rows must not be.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-source-runtime-corpus.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
BASELINE="${2:-$ROOT_DIR/benchmarks/polkadot-sdk-source-runtime-baseline.tsv}"
LABELS="${3:-$ROOT_DIR/benchmarks/labels/RTC.tsv}"
[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
[[ "$BASELINE" = /* ]] || BASELINE="$ROOT_DIR/$BASELINE"
[[ "$LABELS" = /* ]] || LABELS="$ROOT_DIR/$LABELS"
SDK_DIR="$(cd "$SDK_DIR" && pwd)"

SCAN_ROOTS=(
  polkadot/runtime
  cumulus/parachains/runtimes
  templates
  substrate/bin/node/runtime
  cumulus/test
  polkadot/xcm
)

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

cargo +1.93.0 build --quiet --bin polkadot-linter
LINTER="$ROOT_DIR/target/debug/polkadot-linter"

for scan_root in "${SCAN_ROOTS[@]}"; do
  output="$WORK_DIR/$(echo "$scan_root" | tr '/' '_').json"
  set +e
  "$LINTER" \
    --config "$ROOT_DIR/config/default.toml" \
    --syntax-only \
    --format json \
    --rules runtime-config \
    "$SDK_DIR/$scan_root" >"$output" 2>"$WORK_DIR/stderr"
  scan_status=$?
  set -e
  if [[ "$scan_status" -ne 0 && "$scan_status" -ne 1 ]]; then
    cat "$WORK_DIR/stderr" >&2
    echo "runtime corpus scan of $scan_root failed with status $scan_status" >&2
    exit "$scan_status"
  fi
done

# All findings as `rule_id<TAB>sdk-relative-path<TAB>line`.
jq -s -r --arg sdk "$SDK_DIR/" \
  'add | .[] | [.rule_id, (.file | ltrimstr($sdk)), (.line | tostring)] | @tsv' \
  "$WORK_DIR"/*.json | sort -u >"$WORK_DIR/findings.tsv"

status=0

while IFS=$'\t' read -r rule_id scan_root _; do
  [[ "$rule_id" == "rule_id" ]] && { printf 'rule_id\tscan_root\tcount\n'; continue; }
  count="$(awk -F'\t' -v rule="$rule_id" -v root="$scan_root/" \
    '$1 == rule && index($2, root) == 1 { n++ } END { print n + 0 }' "$WORK_DIR/findings.tsv")"
  printf '%s\t%s\t%s\n' "$rule_id" "$scan_root" "$count"
done <"$BASELINE" >"$WORK_DIR/actual-baseline.tsv"

if ! diff -u "$BASELINE" "$WORK_DIR/actual-baseline.tsv"; then
  echo "source-rule runtime corpus differs from the pinned baseline" >&2
  status=1
fi

failures=0
checked=0
while IFS=$'\t' read -r rule_id path line label _evidence _note; do
  [[ "$rule_id" == "rule_id" || -z "$rule_id" ]] && continue
  checked=$((checked + 1))
  if grep -qxF "$(printf '%s\t%s\t%s' "$rule_id" "$path" "$line")" "$WORK_DIR/findings.tsv"; then
    reported=1
  else
    reported=0
  fi
  if [[ "$label" == "TP" && "$reported" -eq 0 ]]; then
    echo "label mismatch: expected $rule_id at $path:$line (TP) but it was not reported" >&2
    failures=$((failures + 1))
  elif [[ "$label" == "FP" && "$reported" -eq 1 ]]; then
    echo "label mismatch: $rule_id reported at $path:$line, which is labeled FP" >&2
    failures=$((failures + 1))
  fi
done <"$LABELS"

if [[ "$failures" -ne 0 ]]; then
  echo "$failures of $checked RTC labels failed" >&2
  status=1
else
  echo "all $checked RTC labels match"
fi

# Every reported finding must be labeled, so new findings get reviewed.
unlabeled="$(awk -F'\t' 'NR == FNR { if (FNR > 1) seen[$1 "\t" $2 "\t" $3] = 1; next }
  !(($1 "\t" $2 "\t" $3) in seen)' "$LABELS" "$WORK_DIR/findings.tsv")"
if [[ -n "$unlabeled" ]]; then
  echo "unlabeled RTC findings (add them to $LABELS):" >&2
  echo "$unlabeled" >&2
  status=1
fi

if [[ "$status" -eq 0 ]]; then
  echo "source-rule runtime corpus matches pinned baseline"
fi
exit "$status"
