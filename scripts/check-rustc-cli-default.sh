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
#![feature(register_tool)]
#![register_tool(pallet)]

pub type Payload = Vec<u8>;

pub mod frame_support {
    pub mod storage {
        pub mod types {
            pub struct StorageValue<K, V>(K, V);
        }
    }
}

#[pallet::storage]
pub type AliasedStorage = frame_support::storage::types::StorageValue<(), Payload>;
RS

cargo +nightly-2025-09-01 build \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/default.toml" \
  --format json \
  --rules SEC013 \
  --syntax-only \
  "$FIXTURE_DIR" > "$SYNTAX_JSON"

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  --config "$ROOT_DIR/config/default.toml" \
  --format json \
  --rules SEC013 \
  --driver-path "$DRIVER" \
  "$FIXTURE_DIR" > "$DEFAULT_JSON"

syntax_findings="$(jq '[.[] | select(.rule_id == "SEC013")] | length' "$SYNTAX_JSON")"
default_findings="$(jq '[.[] | select(.rule_id == "SEC013")] | length' "$DEFAULT_JSON")"
default_file="$(jq -r '[.[] | select(.rule_id == "SEC013")][0].file' "$DEFAULT_JSON")"

echo "SEC013 syntax findings: $syntax_findings"
echo "SEC013 default CLI findings: $default_findings"
echo "SEC013 default CLI file: $default_file"

test "$syntax_findings" -eq 0
test "$default_findings" -eq 1
case "$default_file" in
  src/lib.rs|*default-routing-fixture/src/lib.rs) ;;
  *)
    echo "default compiler-backed diagnostic did not resolve to the fixture source" >&2
    exit 1
    ;;
esac

echo "default CLI routing uses the rustc-backed SEC013 rule"
