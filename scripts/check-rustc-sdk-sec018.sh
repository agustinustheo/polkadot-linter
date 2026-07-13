#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sdk-sec018.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
OUTPUT_DIR="${2:-$ROOT_DIR/.benchmarks}"
[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
[[ "$OUTPUT_DIR" = /* ]] || OUTPUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
BASELINE="${3:-$ROOT_DIR/benchmarks/polkadot-sdk-rustc-contracts-sec018-baseline.tsv}"
PACKAGE="pallet-contracts"
PACKAGE_DIR="$SDK_DIR/substrate/frame/contracts"
PACKAGE_FILE="substrate/frame/contracts/src/lib.rs"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$OUTPUT_DIR"

SYNTAX_JSON="$OUTPUT_DIR/rustc-sdk-sec018-syntax-$TIMESTAMP.json"
RUSTC_JSON="$OUTPUT_DIR/rustc-sdk-sec018-$TIMESTAMP.json"
RUSTC_SUMMARY_TSV="$OUTPUT_DIR/rustc-sdk-sec018-$TIMESTAMP-summary.tsv"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
if [[ -n "${POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR:-}" ]]; then
  SDK_TARGET_DIR="$POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR"
else
  SDK_TARGET_DIR="$(mktemp -d)"
  trap 'rm -rf "$SDK_TARGET_DIR"' EXIT
fi

cargo +nightly-2025-09-01 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

cargo +1.93.0 run \
  --quiet \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --bin polkadot-linter \
  -- \
  --config "$ROOT_DIR/config/default.toml" \
  --format json \
  --rules SEC018 \
  --syntax-only \
  "$PACKAGE_DIR" \
  > "$SYNTAX_JSON"

cargo +1.93.0 run \
  --quiet \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --bin polkadot-linter \
  -- \
  --config "$ROOT_DIR/config/default.toml" \
  --format json \
  --rules SEC018 \
  --package "$PACKAGE" \
  --lib \
  --no-default-features \
  --driver-path "$DRIVER" \
  --toolchain nightly-2025-09-01 \
  --target-dir "$SDK_TARGET_DIR" \
  --source-filter "$PACKAGE_FILE" \
  "$PACKAGE_DIR" \
  > "$RUSTC_JSON"

jq -r --arg package_file "$PACKAGE_FILE" '
  [.[] | select(.file | contains($package_file))]
  | unique_by([.rule_id, .file, .line, .message])
  | sort_by(.rule_id, .line, .message)
  | .[]
  | [.rule_id, .file, (.line | tostring), .message]
  | @tsv
' "$RUSTC_JSON" > "$RUSTC_SUMMARY_TSV"

syntax_findings="$(jq '[.[] | select(.rule_id == "SEC018")] | length' "$SYNTAX_JSON")"
rustc_findings="$(wc -l < "$RUSTC_SUMMARY_TSV" | tr -d '[:space:]')"
echo "SEC018 syntax package findings: $syntax_findings"
echo "SEC018 rustc package findings: $rustc_findings"
cat "$RUSTC_SUMMARY_TSV"

test "$syntax_findings" -eq 0
test "$rustc_findings" -eq 1

if ! diff -u <(sort "$BASELINE") <(sort "$RUSTC_SUMMARY_TSV"); then
  echo "rustc SEC018 SDK findings differ from validated baseline" >&2
  exit 1
fi

echo "rustc SEC018 SDK findings match validated baseline"
