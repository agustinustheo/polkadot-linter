#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-source-cfg-attributes.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
FIXTURE_DIR="$WORK_DIR/cfg-attribute-fixture"

mkdir -p "$FIXTURE_DIR/src"
cat > "$FIXTURE_DIR/Cargo.toml" <<'TOML'
[package]
name = "cfg-attribute-fixture"
version = "0.1.0"
edition = "2021"

[features]
default = ["default-weight"]
default-weight = []
explicit-weight = []
TOML

cat > "$FIXTURE_DIR/src/lib.rs" <<'RS'
#[cfg_attr(feature = "default-weight", pallet::weight(Weight::zero()))]
pub fn default_weight() {}

#[cfg_attr(feature = "explicit-weight", pallet::weight(Weight::zero()))]
pub fn explicit_weight() {}

#[cfg_attr(not(custom_cfg), pallet::weight(Weight::zero()))]
pub fn custom_cfg_weight() {}
RS

run_case() {
  local output="$1"
  shift
  cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
    --config "$ROOT_DIR/config/default.toml" \
    --syntax-only \
    --format json \
    --rules SEM011 \
    "$@" \
    "$FIXTURE_DIR" > "$output"
}

DEFAULT_OUTPUT="$WORK_DIR/default.json"
NO_DEFAULT_OUTPUT="$WORK_DIR/no-default.json"
EXPLICIT_OUTPUT="$WORK_DIR/explicit.json"

run_case "$DEFAULT_OUTPUT"
run_case "$NO_DEFAULT_OUTPUT" --no-default-features
run_case "$EXPLICIT_OUTPUT" --no-default-features --features explicit-weight

default_count="$(jq '[.[] | select(.rule_id == "SEM011")] | length' "$DEFAULT_OUTPUT")"
no_default_count="$(jq '[.[] | select(.rule_id == "SEM011")] | length' "$NO_DEFAULT_OUTPUT")"
explicit_count="$(jq '[.[] | select(.rule_id == "SEM011")] | length' "$EXPLICIT_OUTPUT")"

echo "source cfg_attr default findings: $default_count"
echo "source cfg_attr no-default findings: $no_default_count"
echo "source cfg_attr explicit findings: $explicit_count"

test "$default_count" -eq 1
test "$no_default_count" -eq 0
test "$explicit_count" -eq 1

echo "source cfg_attr analysis matches Cargo default and explicit feature selection"
