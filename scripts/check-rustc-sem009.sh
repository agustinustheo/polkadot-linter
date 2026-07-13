#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-sem009.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

FIXTURE_DIR="$WORK_DIR/sem009-fixture"
DRIVER="$ROOT_DIR/target/debug/polkadot-linter-rustc"
OUTPUT="$WORK_DIR/diagnostics.json"

mkdir -p "$FIXTURE_DIR/src"

cat > "$FIXTURE_DIR/Cargo.toml" <<'TOML'
[package]
name = "sem009-fixture"
version = "0.1.0"
edition = "2021"
TOML

cat > "$FIXTURE_DIR/src/lib.rs" <<'RS'
pub mod frame_support {
    pub mod storage {
        pub mod types {
            pub struct StorageMap<T>(core::marker::PhantomData<T>);

            impl<T> StorageMap<T> {
                pub fn contains_key(_key: &u32) -> bool { true }
                pub fn remove(_key: &u32) {}
                pub fn take(_key: &u32) -> Option<u32> { None }
            }
        }
    }
}

pub fn redundant_remove(key: &u32) {
    if frame_support::storage::types::StorageMap::<()>::contains_key(key) {
        frame_support::storage::types::StorageMap::<()>::remove(key);
    }
}

pub fn redundant_take(key: &u32) {
    if frame_support::storage::types::StorageMap::<()>::contains_key(key) {
        let _ = frame_support::storage::types::StorageMap::<()>::take(key);
    }
}

pub fn different_key(key: &u32, other: &u32) {
    if frame_support::storage::types::StorageMap::<()>::contains_key(key) {
        frame_support::storage::types::StorageMap::<()>::remove(other);
    }
}
RS

cargo +nightly-2025-06-10 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-rustc

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/default.toml" \
  --format json \
  --rules SEM009 \
  --rustc-driver "$DRIVER" \
  "$FIXTURE_DIR" > "$OUTPUT"

cat "$OUTPUT"

remove_count="$(jq '[.[] | select(.rule_id == "SEM009" and .line == 16)] | length' "$OUTPUT")"
take_count="$(jq '[.[] | select(.rule_id == "SEM009" and .line == 22)] | length' "$OUTPUT")"
different_key_count="$(jq '[.[] | select(.rule_id == "SEM009" and .line == 28)] | length' "$OUTPUT")"

echo "SEM009 redundant same-key findings: $((remove_count + take_count))"
echo "SEM009 different-key findings: $different_key_count"

test "$remove_count" -eq 1
test "$take_count" -eq 1
test "$different_key_count" -eq 0

echo "rustc SEM009 resolves FRAME storage owners and key bindings before reporting"
