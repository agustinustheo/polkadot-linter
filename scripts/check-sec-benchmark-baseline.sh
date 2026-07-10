#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-sec-benchmark-baseline.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RAW_JSON="${1:?usage: scripts/check-sec-benchmark-baseline.sh <raw-json> [baseline-tsv]}"
BASELINE="${2:-$ROOT_DIR/benchmarks/polkadot-sdk-sec018-baseline.tsv}"

ACTUAL="$(mktemp)"
EXPECTED="$(mktemp)"
trap 'rm -f "$ACTUAL" "$EXPECTED"' EXIT

jq -r '.[] | [.rule_id, .file, (.line | tostring), .message] | @tsv' "$RAW_JSON" \
  | sort > "$ACTUAL"
sort "$BASELINE" > "$EXPECTED"

if ! diff -u "$EXPECTED" "$ACTUAL"; then
  echo "SEC benchmark findings differ from validated baseline" >&2
  exit 1
fi

echo "SEC benchmark findings match validated baseline"
