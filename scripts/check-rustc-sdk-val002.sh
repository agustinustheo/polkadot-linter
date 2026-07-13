#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sdk-val002.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
OUTPUT_DIR="${2:-$ROOT_DIR/.benchmarks}"
BASELINE="${3:-$ROOT_DIR/benchmarks/polkadot-sdk-rustc-val002-baseline.tsv}"
TOOLCHAIN="nightly-2025-09-01"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
[[ "$OUTPUT_DIR" = /* ]] || OUTPUT_DIR="$ROOT_DIR/$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

RAW_JSON="$OUTPUT_DIR/rustc-sdk-val002-$TIMESTAMP.jsonl"
SUMMARY="$OUTPUT_DIR/rustc-sdk-val002-$TIMESTAMP-summary.tsv"
if [[ -n "${POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR:-}" ]]; then
  SDK_TARGET_DIR="$POLKADOT_LINTER_SDK_RUSTC_TARGET_DIR"
else
  SDK_TARGET_DIR="$(mktemp -d)"
  trap 'rm -rf "$SDK_TARGET_DIR"' EXIT
fi

declare -a PACKAGES=(
  "aura:pallet-aura"
  "babe:pallet-babe"
  "broker:pallet-broker"
  "collective:pallet-collective"
  "revive:pallet-revive"
  "sassafras:pallet-sassafras"
  "staking-async:pallet-staking-async"
)

cargo +"$TOOLCHAIN" build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

: > "$RAW_JSON"
for package_spec in "${PACKAGES[@]}"; do
  package_dir="${package_spec%%:*}"
  package_name="${package_spec##*:}"
  manifest="$SDK_DIR/substrate/frame/$package_dir/Cargo.toml"

  cargo +"$TOOLCHAIN" clean --quiet \
    --manifest-path "$manifest" \
    --package "$package_name" \
    --target-dir "$SDK_TARGET_DIR"

  RUSTFLAGS='--cap-lints warn' \
    POLKADOT_LINTER_DRIVER_RULES=VAL002 \
    POLKADOT_LINTER_DRIVER_JSONL="$RAW_JSON" \
    POLKADOT_LINTER_DRIVER_MANIFEST_ROOT="$SDK_DIR/substrate/frame/$package_dir" \
    RUSTC_WORKSPACE_WRAPPER="$DRIVER" \
    DYLD_FALLBACK_LIBRARY_PATH="$(rustup run "$TOOLCHAIN" rustc --print sysroot)/lib" \
    CARGO_TARGET_DIR="$SDK_TARGET_DIR" \
    cargo +"$TOOLCHAIN" check --quiet --locked \
      --manifest-path "$manifest" \
      --package "$package_name" \
      --lib \
      --no-default-features
done

jq -r '
  select(.rule_id == "VAL002")
  | [.rule_id, .file, (.line | tostring), .message]
  | @tsv
' "$RAW_JSON" | sort -u > "$SUMMARY"

if ! diff -u "$BASELINE" "$SUMMARY"; then
  echo "rustc VAL002 SDK findings differ from the pinned baseline" >&2
  exit 1
fi

echo "rustc VAL002 SDK findings match the pinned baseline"
echo "Wrote:"
echo "  $RAW_JSON"
echo "  $SUMMARY"
