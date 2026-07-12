#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${1:-$ROOT_DIR/.repos/polkadot-sdk}"
[[ "$SDK_DIR" = /* ]] || SDK_DIR="$ROOT_DIR/$SDK_DIR"
PACKAGE_DIR="$SDK_DIR/substrate/frame/collective"
TARGET_DIR="$(mktemp -d)"
trap 'rm -rf "$TARGET_DIR"' EXIT
cargo +nightly-2025-06-10 build --manifest-path "$ROOT_DIR/Cargo.toml" --features rustc-driver --bin polkadot-linter-rustc
OUTPUT="$(cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- --format json --rules SEC005 --rustc-cargo-manifest "$PACKAGE_DIR/Cargo.toml" --rustc-package pallet-collective --rustc-lib --rustc-no-default-features --rustc-driver "$ROOT_DIR/target/debug/polkadot-linter-rustc" --rustc-toolchain nightly-2025-06-10 --rustc-target-dir "$TARGET_DIR" --rustc-source-filter substrate/frame/collective/src/lib.rs "$PACKAGE_DIR")"
printf '%s\n' "$OUTPUT"
test "$(jq '[.[] | select(.rule_id == "SEC005")] | length' <<<"$OUTPUT")" -eq 2
test "$(jq '[.[] | select(.rule_id == "SEC005" and (.line == 633 or .line == 685))] | length' <<<"$OUTPUT")" -eq 2
