#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-source-rule-corpus.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
BASELINE="${2:-$ROOT_DIR/benchmarks/polkadot-sdk-source-frame-baseline.tsv}"
[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
[[ "$BASELINE" = /* ]] || BASELINE="$ROOT_DIR/$BASELINE"

RESULTS_JSON="$(mktemp)"
ACTUAL_BASELINE="$(mktemp)"
trap 'rm -f "$RESULTS_JSON" "$ACTUAL_BASELINE"' EXIT

set +e
cargo +1.93.0 run --quiet --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/default.toml" \
  --syntax-only \
  --format json \
  "$SDK_DIR/substrate/frame" >"$RESULTS_JSON"
scan_status=$?
set -e
if [[ "$scan_status" -ne 0 && "$scan_status" -ne 1 ]]; then
  echo "source-rule corpus scan failed with status $scan_status" >&2
  exit "$scan_status"
fi

while IFS=$'\t' read -r rule_id _; do
  count="$(jq --arg rule_id "$rule_id" '[.[] | select(.rule_id == $rule_id)] | length' "$RESULTS_JSON")"
  printf '%s\t%s\n' "$rule_id" "$count"
done <"$BASELINE" >"$ACTUAL_BASELINE"

if ! diff -u "$BASELINE" "$ACTUAL_BASELINE"; then
  echo "source-rule FRAME corpus differs from the pinned baseline" >&2
  exit 1
fi

echo "source-rule FRAME corpus matches pinned baseline"
