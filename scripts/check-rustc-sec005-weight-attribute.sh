#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURE_DIR="$ROOT_DIR/tests/fixtures/rustc-weight-attribute"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-rustc"
TARGET_DIR="$(mktemp -d)"
trap 'rm -rf "$TARGET_DIR"' EXIT

cargo +nightly-2025-06-10 build --manifest-path "$ROOT_DIR/Cargo.toml" --features rustc-driver --bin polkadot-linter-rustc

OUTPUT="$(cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --format json --rules SEC005 \
  --rustc-cargo-manifest "$FIXTURE_DIR/Cargo.toml" \
  --rustc-package rustc-weight-attribute-fixture --rustc-lib \
  --rustc-driver "$DRIVER" --rustc-toolchain nightly-2025-06-10 \
  --rustc-target-dir "$TARGET_DIR" "$FIXTURE_DIR/src")"

printf '%s\n' "$OUTPUT"
test "$(jq '[.[] | select(.rule_id == "SEC005")] | length' <<<"$OUTPUT")" -eq 2
test "$(jq '[.[] | select(.rule_id == "SEC005" and .line == 41)] | length' <<<"$OUTPUT")" -eq 1
test "$(jq '[.[] | select(.rule_id == "SEC005" and .line == 46)] | length' <<<"$OUTPUT")" -eq 1
