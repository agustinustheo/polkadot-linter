#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-cli-default.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

FIXTURE_DIR="$WORK_DIR/default-routing-fixture"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
SYNTAX_JSON="$WORK_DIR/syntax.json"
DEFAULT_JSON="$WORK_DIR/default.json"

mkdir -p "$FIXTURE_DIR/src"

cat > "$FIXTURE_DIR/Cargo.toml" <<'TOML'
[package]
name = "default-routing-fixture"
version = "0.1.0"
edition = "2021"
TOML

cat > "$FIXTURE_DIR/src/lib.rs" <<'RS'
pub const DECIMALS: u128 = 10 ^ 18;
RS

cargo +nightly-2025-09-01 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/all-rules.toml" \
  --format json \
  --rules SEM010 \
  --syntax-only \
  "$FIXTURE_DIR" > "$SYNTAX_JSON"

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/all-rules.toml" \
  --format json \
  --rules SEM010 \
  --driver-path "$DRIVER" \
  "$FIXTURE_DIR" > "$DEFAULT_JSON" || test "$?" -eq 1 # SEM010 is error-level, so the CLI exits 1

syntax_findings="$(jq '[.[] | select(.rule_id == "SEM010")] | length' "$SYNTAX_JSON")"
default_findings="$(jq '[.[] | select(.rule_id == "SEM010")] | length' "$DEFAULT_JSON")"
default_file="$(jq -r '[.[] | select(.rule_id == "SEM010")][0].file' "$DEFAULT_JSON")"

echo "SEM010 syntax findings: $syntax_findings"
echo "SEM010 default CLI findings: $default_findings"
echo "SEM010 default CLI file: $default_file"

test "$syntax_findings" -eq 0
test "$default_findings" -eq 1
case "$default_file" in
  src/lib.rs|*default-routing-fixture/src/lib.rs) ;;
  *)
    echo "default compiler-backed diagnostic did not resolve to the fixture source" >&2
    exit 1
    ;;
esac

echo "default CLI routing uses the rustc-backed SEM010 rule"
