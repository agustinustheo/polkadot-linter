#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sec009.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
CONFIG_FILE="$(mktemp)"
RUSTC_TARGET_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR" "$RUSTC_TARGET_DIR"; rm -f "$CONFIG_FILE"' EXIT

FIXTURE="$WORK_DIR/substrate/frame/sec009-fixture/src/lib.rs"
mkdir -p "$(dirname "$FIXTURE")"
cat > "$FIXTURE" <<'RS'
use std::ops::Add;

pub fn raw_integer(a: u32, b: u32) -> Result<u32, ()> {
    Ok(a + b)
}

pub struct Field;

impl Add for Field {
    type Output = Field;

    fn add(self, _rhs: Field) -> Field {
        Field
    }
}

pub fn overloaded(a: Field, b: Field) -> Result<Field, ()> {
    Ok(a + b)
}

pub fn infallible(a: u32, b: u32) -> u32 {
    a + b
}
RS

cat > "$CONFIG_FILE" <<'TOML'
[rules.enabled]
SEC009 = true
TOML

SYN_JSON="$WORK_DIR/syn-sec009.json"
RUSTC_JSON="$WORK_DIR/rustc-sec009.json"

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  -c "$CONFIG_FILE" \
  "$WORK_DIR" \
  --rules SEC009 \
  -f json > "$SYN_JSON"

cargo +nightly-2025-06-10 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-rustc -- \
  "$FIXTURE" \
  --crate-type lib \
  --edition 2021 \
  --emit metadata \
  --out-dir "$RUSTC_TARGET_DIR" > "$RUSTC_JSON"

syn_count="$(jq 'length' "$SYN_JSON")"
rustc_count="$(jq 'length' "$RUSTC_JSON")"
rustc_line="$(jq -r '.[0].line // empty' "$RUSTC_JSON")"
rustc_rule="$(jq -r '.[0].rule_id // empty' "$RUSTC_JSON")"

echo "syntax SEC009 findings: $syn_count"
echo "rustc SEC009 findings: $rustc_count"

test "$syn_count" = "2"
test "$rustc_count" = "1"
test "$rustc_rule" = "SEC009"
test "$rustc_line" = "4"
