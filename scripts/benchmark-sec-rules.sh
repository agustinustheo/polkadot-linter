#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/benchmark-sec-rules.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_PATH="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
OUTPUT_DIR="${2:-$ROOT_DIR/.benchmarks}"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$OUTPUT_DIR"

RAW_JSON="$OUTPUT_DIR/sec-rules-$TIMESTAMP.json"
SUMMARY_TXT="$OUTPUT_DIR/sec-rules-$TIMESTAMP-summary.txt"

cargo run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  --rules SEC \
  -f json \
  "$TARGET_PATH" > "$RAW_JSON"

{
  echo "Benchmark target: $TARGET_PATH"
  echo "Generated at: $TIMESTAMP"
  echo
  echo "Findings by rule:"
  jq -r '
    group_by(.rule_id)
    | sort_by(length)
    | reverse
    | .[]
    | "\(. | length)\t\(.[0].rule_id)\t\(.[0].rule_name)"
  ' "$RAW_JSON"
  echo
  echo "Top files:"
  jq -r '
    group_by(.file)
    | sort_by(length)
    | reverse
    | .[:20]
    | .[]
    | "\(. | length)\t\(.[0].file)"
  ' "$RAW_JSON"
} > "$SUMMARY_TXT"

echo "Wrote:"
echo "  $RAW_JSON"
echo "  $SUMMARY_TXT"
