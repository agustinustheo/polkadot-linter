#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-val003.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

FIXTURE_DIR="$WORK_DIR/val003-fixture"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-driver"
OUTPUT="$WORK_DIR/diagnostics.json"

mkdir -p "$FIXTURE_DIR/src"

cat > "$FIXTURE_DIR/Cargo.toml" <<'TOML'
[package]
name = "val003-fixture"
version = "0.1.0"
edition = "2021"
TOML

cat > "$FIXTURE_DIR/src/lib.rs" <<'RS'
pub mod frame_support {
    pub mod storage {
        pub mod types {
            pub struct StorageValue;

            impl StorageValue {
                pub fn put(_value: u32) {}
            }
        }
    }
}

fn validate(origin: bool) -> Result<(), ()> {
    origin.then_some(()).ok_or(())
}

pub fn write_before_validation(origin: bool) -> Result<(), ()> {
    frame_support::storage::types::StorageValue::put(1);
    validate(origin)?;
    Ok(())
}

pub fn validate_before_write(origin: bool) -> Result<(), ()> {
    validate(origin)?;
    frame_support::storage::types::StorageValue::put(1);
    Ok(())
}
RS

cargo +nightly-2025-09-01 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --format json \
  --rules VAL003 \
  --driver-path "$DRIVER" \
  "$FIXTURE_DIR" > "$OUTPUT"

write_count="$(jq '[.[] | select(.rule_id == "VAL003" and .line == 18)] | length' "$OUTPUT")"
valid_count="$(jq '[.[] | select(.rule_id == "VAL003" and .line == 24)] | length' "$OUTPUT")"

echo "VAL003 write-before-validation findings: $write_count"
echo "VAL003 validation-before-write findings: $valid_count"

test "$write_count" -eq 1
test "$valid_count" -eq 0

echo "rustc VAL003 resolves storage writes and fallible validation order"
