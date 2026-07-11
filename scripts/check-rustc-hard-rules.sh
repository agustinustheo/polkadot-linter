#!/usr/bin/env bash

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for scripts/check-rustc-hard-rules.sh" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
CONFIG_FILE="$(mktemp)"
RUSTC_TARGET_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR" "$RUSTC_TARGET_DIR"; rm -f "$CONFIG_FILE"' EXIT

FIXTURE="$WORK_DIR/substrate/frame/hard-rules-fixture/src/lib.rs"
SYNTAX_FIXTURE="$WORK_DIR/substrate/frame/sec001-syntax-fixture/src/lib.rs"
mkdir -p "$(dirname "$FIXTURE")"
mkdir -p "$(dirname "$SYNTAX_FIXTURE")"

cat > "$SYNTAX_FIXTURE" <<'RS'
pub type Payload = Vec<u8>;
pub type BoundedPayload = BoundedVec<u8, 32>;
pub struct BoundedVec<T, const N: usize>(T);

pub mod frame_support {
    pub mod storage {
        pub mod types {
            pub struct StorageValue<K, V>(K, V);
        }
    }
}

#[pallet::storage]
pub type AliasedStorage = frame_support::storage::types::StorageValue<(), Payload>;

#[pallet::storage]
pub type BoundedStorage = frame_support::storage::types::StorageValue<(), BoundedPayload>;

#[pallet::event]
pub enum Event {
    Submitted { payload: Payload },
}

pub struct Domain;

impl Domain {
    pub fn iter() -> std::vec::IntoIter<u8> {
        Vec::new().into_iter()
    }

    pub fn clear_prefix<K>(_key: K, _limit: Option<u32>) {}
}

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::submit_alias())]
    pub fn submit_alias(origin: OriginFor<T>, payload: Payload) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        let _ = payload;
        Ok(())
    }

    #[pallet::call_index(1)]
    pub fn local_iteration(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        let _ = Domain::iter();
        Ok(())
    }

    #[pallet::call_index(2)]
    pub fn local_clear_prefix(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Domain::clear_prefix((), None);
        Ok(())
    }
}
RS

cat > "$FIXTURE" <<'RS'
#![feature(register_tool)]
#![register_tool(pallet)]

use std::ops::Add;
use std::convert::Infallible;

pub type Payload = Vec<u8>;
pub struct BoundedVec<T, const N: usize>(T);
pub type BoundedPayload = BoundedVec<u8, 32>;

pub enum Event {
    Submitted { payload: Payload },
    Bounded { payload: BoundedPayload },
}

pub mod frame_support {
    pub mod storage {
        pub mod types {
            pub struct StorageValue<K, V>(K, V);
            pub struct StorageMap;

            impl StorageMap {
                pub fn iter() -> std::vec::IntoIter<u8> {
                    Vec::new().into_iter()
                }

                pub fn clear_prefix<K>(_key: K, _limit: Option<u32>) {}
            }
        }
    }
}

#[pallet::storage]
pub type AliasedStorage = frame_support::storage::types::StorageValue<(), Payload>;

#[pallet::storage]
pub type BoundedStorage = frame_support::storage::types::StorageValue<(), BoundedPayload>;

pub struct Domain;

impl Domain {
    pub fn iter() -> std::vec::IntoIter<u8> {
        Vec::new().into_iter()
    }

    pub fn clear_prefix<K>(_key: K, _limit: Option<u32>) {}
}

pub struct RuntimeCall;
pub type AliasCall = RuntimeCall;
pub struct MigrationState;

impl RuntimeCall {
    pub fn decode(_input: &mut &[u8]) -> Result<Self, ()> {
        Ok(RuntimeCall)
    }

    pub fn decode_with_depth_limit(_limit: usize, _input: &mut &[u8]) -> Result<Self, ()> {
        Ok(RuntimeCall)
    }
}

impl MigrationState {
    pub fn decode(_input: &mut &[u8]) -> Result<Self, ()> {
        Ok(MigrationState)
    }
}

#[pallet::weight(WeightInfo::submit_missing())]
pub fn submit_alias(payload: Payload) {
    let _ = payload;
    helper_vec(Vec::new());
}

pub fn submit_bounded(payload: BoundedVec<u8, 32>) {
    let _ = payload;
}

#[pallet::weight(WeightInfo::submit_bounded())]
pub fn weighted_bounded(payload: BoundedPayload) {
    let _ = payload;
}

fn helper_vec(payload: Vec<u8>) {
    let _ = payload;
}

pub fn storage_iteration() {
    let _ = frame_support::storage::types::StorageMap::iter();
}

pub fn local_iteration() {
    let _ = Domain::iter();
}

pub fn storage_clear_prefix_unbounded() {
    frame_support::storage::types::StorageMap::clear_prefix((), Some(u32::MAX));
}

pub fn storage_clear_prefix_bounded() {
    frame_support::storage::types::StorageMap::clear_prefix((), Some(10));
}

#[allow(dead_code)]
fn private_storage_iteration() {
    let _ = frame_support::storage::types::StorageMap::iter();
}

pub fn decode_alias_call(mut data: &[u8]) -> Result<RuntimeCall, ()> {
    AliasCall::decode(&mut data)
}

pub fn decode_with_limit(mut data: &[u8]) -> Result<RuntimeCall, ()> {
    RuntimeCall::decode_with_depth_limit(64, &mut data)
}

pub fn decode_internal(mut data: &[u8]) -> Result<MigrationState, ()> {
    MigrationState::decode(&mut data)
}

#[cfg(any())]
pub fn disabled_debug_assert(value: u32) {
    debug_assert!(value > 0, "disabled debug assertion should not be linted");
}

pub fn active_debug_assert(value: u32) {
    debug_assert!(value > 0, "active debug assertion should be linted");
}

pub fn infallible_result() -> Result<u32, Infallible> {
    Ok(7)
}

pub fn fallible_result(flag: bool) -> Result<u32, ()> {
    if flag {
        Ok(7)
    } else {
        Err(())
    }
}

pub fn unwrap_infallible_result() -> u32 {
    infallible_result().unwrap()
}

pub fn unwrap_fallible_result(flag: bool) -> u32 {
    fallible_result(flag).expect("flag controls the error path")
}

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
SEC001 = true
SEC002 = true
SEC003 = true
SEC008 = true
SEC009 = true
SEC011 = true
SEC012 = true
SEC013 = true
SEC017 = true
SEC018 = true
TOML

SYN_JSON="$WORK_DIR/syn-hard-rules.json"
RUSTC_JSON="$WORK_DIR/rustc-hard-rules.json"

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  -c "$CONFIG_FILE" \
  "$WORK_DIR" \
  --rules SEC001,SEC002,SEC003,SEC008,SEC009,SEC011,SEC012,SEC013,SEC017,SEC018 \
  -f json > "$SYN_JSON"

cargo +nightly-2025-06-10 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-rustc -- \
  "$FIXTURE" \
  --crate-type lib \
  --edition 2021 \
  --emit metadata \
  --out-dir "$RUSTC_TARGET_DIR" > "$RUSTC_JSON"

syn_sec001_count="$(jq '[.[] | select(.rule_id == "SEC001")] | length' "$SYN_JSON")"
syn_sec002_count="$(jq '[.[] | select(.rule_id == "SEC002")] | length' "$SYN_JSON")"
syn_sec003_count="$(jq '[.[] | select(.rule_id == "SEC003")] | length' "$SYN_JSON")"
syn_sec008_count="$(jq '[.[] | select(.rule_id == "SEC008")] | length' "$SYN_JSON")"
syn_sec009_count="$(jq '[.[] | select(.rule_id == "SEC009")] | length' "$SYN_JSON")"
syn_sec011_count="$(jq '[.[] | select(.rule_id == "SEC011")] | length' "$SYN_JSON")"
syn_sec012_count="$(jq '[.[] | select(.rule_id == "SEC012")] | length' "$SYN_JSON")"
syn_sec013_count="$(jq '[.[] | select(.rule_id == "SEC013")] | length' "$SYN_JSON")"
syn_sec017_count="$(jq '[.[] | select(.rule_id == "SEC017")] | length' "$SYN_JSON")"
syn_sec018_count="$(jq '[.[] | select(.rule_id == "SEC018")] | length' "$SYN_JSON")"
rustc_sec001_count="$(jq '[.[] | select(.rule_id == "SEC001")] | length' "$RUSTC_JSON")"
rustc_sec002_count="$(jq '[.[] | select(.rule_id == "SEC002")] | length' "$RUSTC_JSON")"
rustc_sec003_count="$(jq '[.[] | select(.rule_id == "SEC003")] | length' "$RUSTC_JSON")"
rustc_sec008_count="$(jq '[.[] | select(.rule_id == "SEC008")] | length' "$RUSTC_JSON")"
rustc_sec009_count="$(jq '[.[] | select(.rule_id == "SEC009")] | length' "$RUSTC_JSON")"
rustc_sec011_count="$(jq '[.[] | select(.rule_id == "SEC011")] | length' "$RUSTC_JSON")"
rustc_sec012_count="$(jq '[.[] | select(.rule_id == "SEC012")] | length' "$RUSTC_JSON")"
rustc_sec013_count="$(jq '[.[] | select(.rule_id == "SEC013")] | length' "$RUSTC_JSON")"
rustc_sec017_count="$(jq '[.[] | select(.rule_id == "SEC017")] | length' "$RUSTC_JSON")"
rustc_sec018_count="$(jq '[.[] | select(.rule_id == "SEC018")] | length' "$RUSTC_JSON")"
rustc_sec001_line="$(jq -r '.[] | select(.rule_id == "SEC001") | .line' "$RUSTC_JSON")"
rustc_sec002_line="$(jq -r '.[] | select(.rule_id == "SEC002") | .line' "$RUSTC_JSON")"
rustc_sec003_line="$(jq -r '.[] | select(.rule_id == "SEC003") | .line' "$RUSTC_JSON")"
rustc_sec008_line="$(jq -r '.[] | select(.rule_id == "SEC008") | .line' "$RUSTC_JSON")"
rustc_sec009_line="$(jq -r '.[] | select(.rule_id == "SEC009") | .line' "$RUSTC_JSON")"
rustc_sec011_line="$(jq -r '.[] | select(.rule_id == "SEC011") | .line' "$RUSTC_JSON")"
rustc_sec012_line="$(jq -r '.[] | select(.rule_id == "SEC012") | .line' "$RUSTC_JSON")"
rustc_sec013_line="$(jq -r '.[] | select(.rule_id == "SEC013") | .line' "$RUSTC_JSON")"
rustc_sec017_line="$(jq -r '.[] | select(.rule_id == "SEC017") | .line' "$RUSTC_JSON")"
rustc_sec018_line="$(jq -r '.[] | select(.rule_id == "SEC018") | .line' "$RUSTC_JSON")"

echo "syntax SEC001 findings: $syn_sec001_count"
echo "rustc SEC001 findings: $rustc_sec001_count"
echo "syntax SEC002 findings: $syn_sec002_count"
echo "rustc SEC002 findings: $rustc_sec002_count"
echo "syntax SEC003 findings: $syn_sec003_count"
echo "rustc SEC003 findings: $rustc_sec003_count"
echo "syntax SEC008 findings: $syn_sec008_count"
echo "rustc SEC008 findings: $rustc_sec008_count"
echo "syntax SEC009 findings: $syn_sec009_count"
echo "rustc SEC009 findings: $rustc_sec009_count"
echo "syntax SEC011 findings: $syn_sec011_count"
echo "rustc SEC011 findings: $rustc_sec011_count"
echo "syntax SEC012 findings: $syn_sec012_count"
echo "rustc SEC012 findings: $rustc_sec012_count"
echo "syntax SEC013 findings: $syn_sec013_count"
echo "rustc SEC013 findings: $rustc_sec013_count"
echo "syntax SEC017 findings: $syn_sec017_count"
echo "rustc SEC017 findings: $rustc_sec017_count"
echo "syntax SEC018 findings: $syn_sec018_count"
echo "rustc SEC018 findings: $rustc_sec018_count"

test "$syn_sec001_count" = "0"
test "$rustc_sec001_count" = "1"
test "$rustc_sec001_line" = "70"
test "$syn_sec002_count" = "2"
test "$rustc_sec002_count" = "1"
test "$rustc_sec002_line" = "127"
test "$syn_sec003_count" = "0"
test "$rustc_sec003_count" = "1"
test "$rustc_sec003_line" = "110"
test "$syn_sec008_count" = "2"
test "$rustc_sec008_count" = "1"
test "$rustc_sec008_line" = "147"
test "$syn_sec009_count" = "2"
test "$rustc_sec009_count" = "1"
test "$rustc_sec009_line" = "151"
test "$syn_sec011_count" = "1"
test "$rustc_sec011_count" = "1"
test "$rustc_sec011_line" = "89"
test "$syn_sec012_count" = "2"
test "$rustc_sec012_count" = "1"
test "$rustc_sec012_line" = "97"
test "$syn_sec013_count" = "0"
test "$rustc_sec013_count" = "1"
test "$rustc_sec013_line" = "34"
test "$syn_sec017_count" = "0"
test "$rustc_sec017_count" = "1"
test "$rustc_sec017_line" = "12"
test "$syn_sec018_count" = "0"
test "$rustc_sec018_count" = "1"
test "$rustc_sec018_line" = "70"
