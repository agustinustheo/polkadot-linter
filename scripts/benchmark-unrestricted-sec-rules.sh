#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/benchmark-unrestricted-sec-rules.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_PATH="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
OUTPUT_DIR="${2:-$ROOT_DIR/.benchmarks}"
CONFIG_FILE="${POLKADOT_LINTER_UNRESTRICTED_CONFIG:-$ROOT_DIR/config/default.toml}"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$OUTPUT_DIR"

RAW_JSON="$OUTPUT_DIR/sec-rules-unrestricted-$TIMESTAMP.json"
SUMMARY_TXT="$OUTPUT_DIR/sec-rules-unrestricted-$TIMESTAMP-summary.txt"

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$CONFIG_FILE" \
  --rules SEC \
  --no-rustc \
  --format json \
  "$TARGET_PATH" > "$RAW_JSON"

{
  echo "Benchmark target: $TARGET_PATH"
  echo "Config: $CONFIG_FILE"
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
