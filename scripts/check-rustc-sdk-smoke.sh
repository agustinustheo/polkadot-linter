#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sdk-smoke.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
OUTPUT_DIR="${2:-$ROOT_DIR/.benchmarks}"
[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
[[ "$OUTPUT_DIR" = /* ]] || OUTPUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
BASELINE="${3:-$ROOT_DIR/benchmarks/polkadot-sdk-rustc-multisig-sec001-sec008-baseline.tsv}"
PACKAGE="pallet-multisig"
PACKAGE_DIR="$SDK_DIR/substrate/frame/multisig"
PACKAGE_FILE="substrate/frame/multisig/src/lib.rs"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$OUTPUT_DIR"

RAW_JSON="$OUTPUT_DIR/rustc-sdk-smoke-$TIMESTAMP.json"
SUMMARY_TSV="$OUTPUT_DIR/rustc-sdk-smoke-$TIMESTAMP-summary.tsv"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-rustc"
if [[ -n "${POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR:-}" ]]; then
  SDK_TARGET_DIR="$POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR"
else
  SDK_TARGET_DIR="$(mktemp -d)"
  trap 'rm -rf "$SDK_TARGET_DIR"' EXIT
fi

cargo +nightly-2025-06-10 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-rustc

cargo +1.93.0 run \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --bin polkadot-linter \
  -- \
  --config "$ROOT_DIR/config/default.toml" \
  --format json \
  --rules SEC001,SEC008 \
  --rustc-package "$PACKAGE" \
  --rustc-lib \
  --rustc-no-default-features \
  --rustc-driver "$DRIVER" \
  --rustc-toolchain nightly-2025-06-10 \
  --rustc-target-dir "$SDK_TARGET_DIR" \
  --rustc-source-filter "$PACKAGE_FILE" \
  "$PACKAGE_DIR" \
  > "$RAW_JSON"

jq -r --arg package_file "$PACKAGE_FILE" '
  [.[] | select(.file | contains($package_file))]
  | unique_by([.rule_id, .file, .line, .message])
  | sort_by(.rule_id, .line, .message)
  | .[]
  | [.rule_id, .file, (.line | tostring), .message]
  | @tsv
' "$RAW_JSON" > "$SUMMARY_TSV"

package_findings="$(wc -l < "$SUMMARY_TSV" | tr -d '[:space:]')"
echo "rustc SDK smoke package findings: $package_findings"
cat "$SUMMARY_TSV"

test "$package_findings" -gt 0
grep -q '^SEC001	' "$SUMMARY_TSV"
grep -q '^SEC008	' "$SUMMARY_TSV"

if ! diff -u <(sort "$BASELINE") <(sort "$SUMMARY_TSV"); then
  echo "rustc SDK smoke findings differ from validated baseline" >&2
  exit 1
fi

echo "rustc SDK smoke findings match validated baseline"

echo "Wrote:"
echo "  $RAW_JSON"
echo "  $SUMMARY_TSV"
