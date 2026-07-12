#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sdk-sec010.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
OUTPUT_DIR="${2:-$ROOT_DIR/.benchmarks}"
[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
[[ "$OUTPUT_DIR" = /* ]] || OUTPUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
PACKAGE="pallet-people"
PACKAGE_DIR="$SDK_DIR/substrate/frame/people"
PACKAGE_FILE="substrate/frame/people/src/lib.rs"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$OUTPUT_DIR"
SYNTAX_JSON="$OUTPUT_DIR/rustc-sdk-sec010-syntax-$TIMESTAMP.json"
RUSTC_JSON="$OUTPUT_DIR/rustc-sdk-sec010-$TIMESTAMP.json"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-rustc"
if [[ -n "${POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR:-}" ]]; then
  SDK_TARGET_DIR="$POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR"
else
  SDK_TARGET_DIR="$(mktemp -d)"
  trap 'rm -rf "$SDK_TARGET_DIR"' EXIT
fi

cargo +nightly-2025-06-10 build --manifest-path "$ROOT_DIR/Cargo.toml" --features rustc-driver --bin polkadot-linter-rustc

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --format json --rules SEC010 --no-rustc "$PACKAGE_DIR" > "$SYNTAX_JSON"

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --format json --rules SEC010 --rustc-package "$PACKAGE" --rustc-lib \
  --rustc-no-default-features --rustc-driver "$DRIVER" \
  --rustc-toolchain nightly-2025-06-10 --rustc-target-dir "$SDK_TARGET_DIR" \
  --rustc-source-filter "$PACKAGE_FILE" "$PACKAGE_DIR" > "$RUSTC_JSON"

# The CLI preserves its historical empty-output behavior when no diagnostics exist.
[[ -s "$SYNTAX_JSON" ]] || printf '[]\n' > "$SYNTAX_JSON"
[[ -s "$RUSTC_JSON" ]] || printf '[]\n' > "$RUSTC_JSON"

syntax_findings="$(jq '[.[] | select(.rule_id == "SEC010")] | length' "$SYNTAX_JSON")"
rustc_findings="$(jq '[.[] | select(.rule_id == "SEC010")] | length' "$RUSTC_JSON")"
echo "SEC010 syntax package findings: $syntax_findings"
echo "SEC010 rustc package findings: $rustc_findings"

# pallet-people exercises lifecycle hooks that invoke with_storage_layer. The
# resolved zero baseline ensures the compiler path does not reintroduce the
# previous token-based transactional false positives.
test "$syntax_findings" -eq 0
test "$rustc_findings" -eq 0

echo "rustc SEC010 SDK zero baseline matched"
