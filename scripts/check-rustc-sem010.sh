#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sem010.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

FIXTURE_DIR="$WORK_DIR/sem010-fixture"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
OUTPUT="$WORK_DIR/diagnostics.json"

mkdir -p "$FIXTURE_DIR/src"

cat > "$FIXTURE_DIR/Cargo.toml" <<'TOML'
[package]
name = "sem010-fixture"
version = "0.1.0"
edition = "2021"
TOML

cat > "$FIXTURE_DIR/src/lib.rs" <<'RS'
const BAD_CONST: u128 = 10 ^ 18;
const GOOD_CONST: u128 = 10_u128.pow(18);

pub fn bad_expression() -> u128 {
    2u128 ^ 16
}

pub fn normal_xor() -> u128 {
    0b1010u128 ^ 0b0101
}
RS

cargo +nightly-2025-09-01 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

set +e
cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/default.toml" \
  --format json \
  --rules SEM010 \
  --driver-path "$DRIVER" \
  "$FIXTURE_DIR" > "$OUTPUT"
lint_status="$?"
set -e

bad_const_count="$(jq '[.[] | select(.rule_id == "SEM010" and .line == 1)] | length' "$OUTPUT")"
bad_expression_count="$(jq '[.[] | select(.rule_id == "SEM010" and .line == 5)] | length' "$OUTPUT")"
good_count="$(jq '[.[] | select(.rule_id == "SEM010" and (.line == 2 or .line == 9))] | length' "$OUTPUT")"

echo "SEM010 suspicious integer XOR findings: $((bad_const_count + bad_expression_count))"
echo "SEM010 valid exponentiation/XOR findings: $good_count"

test "$bad_const_count" -eq 1
test "$bad_expression_count" -eq 1
test "$good_count" -eq 0
test "$lint_status" -eq 1

echo "rustc SEM010 resolves integer XOR before reporting suspicious exponentiation syntax"
