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
use crate::frame_support::traits::EnsureOrigin;

pub type Payload = Vec<u8>;
pub type TuplePayload = (Vec<u8>,);
pub struct BoundedVec<T, const N: usize>(T);
pub type BoundedPayload = BoundedVec<u8, 32>;
pub struct EncodedInput;
pub struct Origin;

pub trait Config {
    type AdminOrigin: frame_support::traits::EnsureOrigin;
    type AnyOrigin: frame_support::traits::EnsureOrigin;
}

pub trait Encode {
    fn using_encoded<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R;
}

impl Encode for EncodedInput {
    fn using_encoded<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        f(&[])
    }
}

#[pallet::event] pub enum Event {
    Submitted { payload: Payload },
    Bounded { payload: BoundedPayload },
}

pub mod frame_support {
    pub mod storage {
        pub trait IterableStorageMap {
            fn iter() -> std::vec::IntoIter<u8>;
        }

        pub mod types {
            pub struct StorageValue<K, V>(K, V);
            pub struct StorageMap;

            impl StorageMap {
                pub fn iter() -> std::vec::IntoIter<u8> {
                    Vec::new().into_iter()
                }

                pub fn clear_prefix<K>(_key: K, _limit: Option<u32>) {}
            }

            impl super::IterableStorageMap for StorageMap {
                fn iter() -> std::vec::IntoIter<u8> {
                    Vec::new().into_iter()
                }
            }
        }
    }

    pub mod traits {
        use super::super::Origin;

        pub trait EnsureOrigin {
            fn ensure_origin(_origin: Origin) -> Result<(), ()>;
        }

        pub trait Hooks {
            fn on_runtime_upgrade();
        }

        pub trait OnRuntimeUpgrade {
            fn on_runtime_upgrade();
        }

        pub trait UncheckedOnRuntimeUpgrade {
            fn on_runtime_upgrade();
        }

        pub mod members {
            pub trait ChangeMembers {
                fn change_members_sorted();
            }
        }
    }
}

pub mod frame_system {
    use super::Origin;

    pub fn ensure_root(_origin: Origin) -> Result<(), ()> {
        Ok(())
    }
}

pub mod frame {
    pub mod traits {
        pub trait OnRuntimeUpgrade {
            fn on_runtime_upgrade();
        }
    }
}

pub mod polkadot_sdk_frame {
    pub mod prelude {
        pub trait OnRuntimeUpgrade {
            fn on_runtime_upgrade();
        }
    }
}

pub mod xcm_executor {
    pub mod traits {
        pub trait OnResponse {
            fn on_response(data: &[u8]);
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

#[pallet::call_index(0)] #[pallet::weight(WeightInfo::submit_missing())]
pub fn submit_alias(payload: Payload) {
    let _ = payload;
    helper_vec(Vec::new());
}

#[pallet::call_index(6)] #[pallet::weight(WeightInfo::submit_bounded(payload.len() as u32))]
pub fn bounded_input_vec(payload: Payload) {
    if payload.len() > 32 {
        return;
    }
    let _ = payload;
}

pub fn submit_bounded(payload: BoundedVec<u8, 32>) {
    let _ = payload;
}

#[pallet::call_index(2)] #[pallet::weight(WeightInfo::privileged_root())]
pub fn privileged_root_vec(origin: Origin, payload: Payload) {
    let _ = frame_system::ensure_root(origin);
    let _ = payload;
}

#[pallet::call_index(3)] #[pallet::weight(WeightInfo::privileged_config())]
pub fn privileged_config_vec<T: Config>(origin: Origin, payload: Payload) {
    let _ = T::AdminOrigin::ensure_origin(origin);
    let _ = payload;
}

#[pallet::call_index(5)] #[pallet::weight(WeightInfo::unknown_origin())]
pub fn unknown_config_origin_vec<T: Config>(origin: Origin, payload: Payload) {
    let _ = T::AnyOrigin::ensure_origin(origin);
    let _ = payload;
}

#[pallet::call_index(4)]
#[pallet::weight(WeightInfo::comment_only(/* commented_weight_input.len() */))]
pub fn comment_only_weight_input(origin: Origin, commented_weight_input: Payload) {
    let _ = frame_system::ensure_root(origin);
    let _ = commented_weight_input;
}

#[pallet::weight(WeightInfo::submit_bounded())]
pub fn weighted_bounded(payload: BoundedPayload) {
    let _ = payload;
}

#[pallet::weight(WeightInfo::submit_tuple(payload.0.len() as u32))]
pub fn weighted_tuple(payload: TuplePayload) {
    let _ = payload;
}

fn helper_vec(payload: Vec<u8>) {
    let _ = payload;
}

pub fn storage_iteration() {
    let _ = frame_support::storage::types::StorageMap::iter();
}

pub fn bounded_storage_iteration() {
    let bounded_storage_iteration = frame_support::storage::types::StorageMap::iter().take(10);
    let _ = bounded_storage_iteration;
}

pub fn dynamically_capped_storage_iteration(limit: usize) {
    let dynamically_capped_storage_iteration =
        frame_support::storage::types::StorageMap::iter().take(limit);
    let _ = dynamically_capped_storage_iteration;
}

pub fn storage_iteration_via_private_helper() {
    reachable_private_storage_iteration();
}

fn reachable_private_storage_iteration() {
    let _ = frame_support::storage::types::StorageMap::iter();
}

pub fn storage_iteration_via_trait() {
    let _ = <frame_support::storage::types::StorageMap as frame_support::storage::IterableStorageMap>::iter();
}

pub struct RuntimeUpgrade;

impl frame_support::traits::Hooks for RuntimeUpgrade {
    fn on_runtime_upgrade() {
        let _ = frame_support::storage::types::StorageMap::iter();
    }
}

pub struct Migration;

impl frame_support::traits::OnRuntimeUpgrade for Migration {
    fn on_runtime_upgrade() {
        let _ = frame_support::storage::types::StorageMap::iter();
    }
}

pub struct RenamedFrameMigration;

impl crate::frame::traits::OnRuntimeUpgrade for RenamedFrameMigration {
    fn on_runtime_upgrade() {
        let _ = frame_support::storage::types::StorageMap::iter();
    }
}

pub struct FacadeMigration;

impl crate::polkadot_sdk_frame::prelude::OnRuntimeUpgrade for FacadeMigration {
    fn on_runtime_upgrade() {
        let _ = frame_support::storage::types::StorageMap::iter();
    }
}

pub struct UncheckedMigration;

impl frame_support::traits::UncheckedOnRuntimeUpgrade for UncheckedMigration {
    fn on_runtime_upgrade() {
        let _ = frame_support::storage::types::StorageMap::iter();
    }
}

pub fn local_iteration() {
    let _ = Domain::iter();
}

pub fn storage_clear_prefix_unbounded() {
    frame_support::storage::types::StorageMap::clear_prefix((), Some(u32::MAX));
}

pub fn storage_clear_prefix_via_private_helper() {
    reachable_private_clear_prefix();
}

fn reachable_private_clear_prefix() {
    frame_support::storage::types::StorageMap::clear_prefix((), Some(u32::MAX));
}

#[allow(dead_code)]
fn private_storage_clear_prefix() {
    frame_support::storage::types::StorageMap::clear_prefix((), Some(u32::MAX));
}

pub struct Callback;

impl frame_support::traits::members::ChangeMembers for Callback {
    fn change_members_sorted() {
        frame_support::storage::types::StorageMap::clear_prefix((), Some(u32::MAX));
    }
}

pub mod unrelated {
    pub trait ChangeMembers {
        fn change_members_sorted();
    }
}

pub struct UnrelatedCallback;

impl unrelated::ChangeMembers for UnrelatedCallback {
    fn change_members_sorted() {
        frame_support::storage::types::StorageMap::clear_prefix((), Some(u32::MAX));
    }
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

pub fn decode_alias_in_encoded(payload: EncodedInput) -> Result<RuntimeCall, ()> {
    payload.using_encoded(|mut encoded| AliasCall::decode(&mut encoded))
}

pub fn decode_alias_from_match(data: &[u8]) -> Result<RuntimeCall, ()> {
    match (data, ()) {
        (mut data, ()) => AliasCall::decode(&mut data),
    }
}

pub struct ResponseHandler;

impl crate::xcm_executor::traits::OnResponse for ResponseHandler {
    fn on_response(mut data: &[u8]) {
        let _ = RuntimeCall::decode(&mut data);
    }
}

pub fn decode_via_private_helper(data: &[u8]) -> Result<RuntimeCall, ()> {
    decode_private(data)
}

pub fn decode_via_private_helper_alias(data: &[u8]) -> Result<RuntimeCall, ()> {
    let forwarded = data;
    decode_private_alias(forwarded)
}

pub fn decode_via_private_helper_assignment(data: &[u8]) -> Result<RuntimeCall, ()> {
    let forwarded: &[u8];
    forwarded = data;
    decode_private_assignment(forwarded)
}

pub fn decode_via_match_helper(
    data: &[u8],
    select_input: bool,
) -> Result<RuntimeCall, ()> {
    match (data, select_input) {
        (data, true) => decode_private_match(data),
        _ => Ok(RuntimeCall),
    }
}

pub fn decode_after_clean_assignment(mut data: &[u8]) -> Result<RuntimeCall, ()> {
    let _was_empty = data.is_empty();
    data = &[];
    RuntimeCall::decode(&mut data)
}

pub fn decode_after_conditional_clean_assignment(
    mut data: &[u8],
    clear: bool,
) -> Result<RuntimeCall, ()> {
    if clear {
        data = &[];
    }
    let decoded_after_conditional = RuntimeCall::decode(&mut data);
    decoded_after_conditional
}

pub fn decode_after_branch_selected_input(
    data: &[u8],
    select_input: bool,
) -> Result<RuntimeCall, ()> {
    let mut selected = if select_input { data } else { &[][..] };
    let decoded_after_branch_selected_input = RuntimeCall::decode(&mut selected);
    decoded_after_branch_selected_input
}

pub fn decode_after_match_selected_input(
    data: &[u8],
    select_input: bool,
) -> Result<RuntimeCall, ()> {
    let mut selected = match (data, select_input) {
        (data, true) => data,
        _ => &[][..],
    };
    let decoded_after_match_selected_input = RuntimeCall::decode(&mut selected);
    decoded_after_match_selected_input
}

fn decode_private(mut data: &[u8]) -> Result<RuntimeCall, ()> {
    RuntimeCall::decode(&mut data)
}

fn decode_private_alias(mut data: &[u8]) -> Result<RuntimeCall, ()> {
    RuntimeCall::decode(&mut data)
}

fn decode_private_assignment(mut data: &[u8]) -> Result<RuntimeCall, ()> {
    RuntimeCall::decode(&mut data)
}

fn decode_private_match(mut data: &[u8]) -> Result<RuntimeCall, ()> {
    let decoded_from_match_helper = RuntimeCall::decode(&mut data);
    decoded_from_match_helper
}

#[allow(dead_code)]
fn unreachable_decode(mut data: &[u8]) -> Result<RuntimeCall, ()> {
    RuntimeCall::decode(&mut data)
}

pub fn decode_from_internal() -> Result<RuntimeCall, ()> {
    let mut data = &[][..];
    RuntimeCall::decode(&mut data)
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

macro_rules! nested_debug_assert {
    ($value:expr) => {
        debug_assert!($value, "nested debug assertion should be linted");
    };
}

macro_rules! ensure {
    ($condition:expr, $error:expr) => {
        if !$condition {
            return Err($error);
        }
    };
}

pub fn active_debug_assert(value: u32) {
    let note = "debug_assert! in a string is not an assertion";
    let _ = note;
    // debug_assert! in a comment is not an assertion.
    debug_assert!(value > 0, "active debug assertion should be linted");
}

pub fn nested_debug_assert(value: u32) {
    nested_debug_assert!(value > 0);
}

pub fn debug_assert_via_private_helper(value: u32) {
    reachable_private_debug_assert(value);
}

fn reachable_private_debug_assert(value: u32) {
    debug_assert!(value > 0, "private helper is reachable from a public entry point");
}

#[allow(dead_code)]
fn private_debug_assert(value: u32) {
    debug_assert!(value > 0, "private helper is not a callable entry point");
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

pub fn unwrap_known_ok() -> u32 {
    let known_ok: Result<u32, ()> = Ok(7);
    known_ok.unwrap()
}

pub fn expect_known_some() -> u32 {
    let known_some = Some(7);
    known_some.expect("constructed as Some")
}

pub fn unwrap_after_unknown_overwrite() -> u32 {
    let mut value: Result<u32, ()> = Ok(7);
    let _was_ok = value.is_ok();
    value = fallible_result(false);
    value.expect("unknown overwrite can fail")
}

pub fn unwrap_after_option_guard(value: Option<u32>) -> Result<u32, ()> {
    ensure!(value.is_some(), ());
    Ok(value.unwrap())
}

pub fn unwrap_after_result_guard(value: Result<u32, ()>) -> Result<u32, ()> {
    if value.is_err() {
        return Err(());
    }
    Ok(value.expect("the error path returned early"))
}

pub fn unwrap_inside_some_branch(value: Option<u32>) -> u32 {
    if value.is_some() {
        value.unwrap()
    } else {
        0
    }
}

pub fn unwrap_after_guarded_overwrite(mut value: Option<u32>) -> Result<u32, ()> {
    ensure!(value.is_some(), ());
    value = None;
    Ok(value.unwrap())
}

pub fn unwrap_via_private_helper(flag: bool) -> u32 {
    reachable_private_unwrap_fallible_result(flag)
}

fn reachable_private_unwrap_fallible_result(flag: bool) -> u32 {
    fallible_result(flag).expect("private helper is reachable from a public entry point")
}

#[allow(dead_code)]
fn private_unwrap_fallible_result(flag: bool) -> u32 {
    fallible_result(flag).expect("private helper is not a callable entry point")
}

pub fn raw_integer(a: u32, b: u32, c: u32) -> Result<u32, ()> {
    Ok((a + b) - c)
}

pub fn guarded_subtraction(a: u32, b: u32) -> Result<u32, ()> {
    if a >= b {
        return Ok(a - b);
    }
    Err(())
}

pub fn guarded_subtraction_with_ensure(a: u32, b: u32) -> Result<u32, ()> {
    ensure!(a >= b, ());
    Ok(a - b)
}

pub fn guarded_subtraction_with_early_return(a: u32, b: u32) -> Result<u32, ()> {
    if a < b {
        return Err(());
    }
    Ok(a - b)
}

pub fn raw_division(a: u32, b: u32) -> Result<u32, ()> {
    Ok(a / b)
}

pub fn guarded_division(a: u32, b: u32) -> Result<u32, ()> {
    if b != 0 {
        return Ok(a / b);
    }
    Err(())
}

pub fn guarded_division_with_ensure(a: u32, b: u32) -> Result<u32, ()> {
    ensure!(b != 0, ());
    Ok(a / b)
}

pub fn raw_integer_via_private_helper(a: u32, b: u32) -> Result<u32, ()> {
    Ok(reachable_private_raw_integer(a, b))
}

fn reachable_private_raw_integer(a: u32, b: u32) -> u32 {
    a + b
}

#[allow(dead_code)]
fn private_raw_integer(a: u32, b: u32) -> Result<u32, ()> {
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

pub fn public_helper(payload: Payload) {
    let _ = payload;
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
RUSTC_FILTERED_JSON="$WORK_DIR/rustc-hard-rules-filtered.json"
RUSTC_FILTERED_EMPTY_JSON="$WORK_DIR/rustc-hard-rules-filtered-empty.json"
RUSTC_RULE_FILTERED_JSON="$WORK_DIR/rustc-hard-rules-rule-filtered.json"

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

POLKADOT_LINTER_RUSTC_FILE_CONTAINS="hard-rules-fixture/src/lib.rs" \
  cargo +nightly-2025-06-10 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
    --features rustc-driver \
    --bin polkadot-linter-rustc -- \
    "$FIXTURE" \
    --crate-type lib \
    --edition 2021 \
    --emit metadata \
    --out-dir "$RUSTC_TARGET_DIR" > "$RUSTC_FILTERED_JSON"

POLKADOT_LINTER_RUSTC_FILE_CONTAINS="does-not-match.rs" \
  cargo +nightly-2025-06-10 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
    --features rustc-driver \
    --bin polkadot-linter-rustc -- \
    "$FIXTURE" \
    --crate-type lib \
    --edition 2021 \
    --emit metadata \
    --out-dir "$RUSTC_TARGET_DIR" > "$RUSTC_FILTERED_EMPTY_JSON"

POLKADOT_LINTER_RUSTC_RULES="SEC008,SEC009" \
  cargo +nightly-2025-06-10 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
    --features rustc-driver \
    --bin polkadot-linter-rustc -- \
    "$FIXTURE" \
    --crate-type lib \
    --edition 2021 \
    --emit metadata \
    --out-dir "$RUSTC_TARGET_DIR" > "$RUSTC_RULE_FILTERED_JSON"

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
guarded_subtraction_line="$(grep -n 'return Ok(a - b);' "$FIXTURE" | cut -d: -f1)"
rustc_sec009_guarded_subtraction_count="$(jq --argjson line "$guarded_subtraction_line" '[.[] | select(.rule_id == "SEC009" and .line == $line)] | length' "$RUSTC_JSON")"
ensure_guarded_subtraction_line="$(grep -n 'Ok(a - b)' "$FIXTURE" | tail -n2 | head -n1 | cut -d: -f1)"
rustc_sec009_ensure_guarded_subtraction_count="$(jq --argjson line "$ensure_guarded_subtraction_line" '[.[] | select(.rule_id == "SEC009" and .line == $line)] | length' "$RUSTC_JSON")"
early_return_guarded_subtraction_line="$(grep -n 'Ok(a - b)' "$FIXTURE" | tail -n1 | cut -d: -f1)"
rustc_sec009_early_return_guarded_subtraction_count="$(jq --argjson line "$early_return_guarded_subtraction_line" '[.[] | select(.rule_id == "SEC009" and .line == $line)] | length' "$RUSTC_JSON")"
raw_division_line="$(grep -n 'Ok(a / b)' "$FIXTURE" | head -n1 | cut -d: -f1)"
rustc_sec009_raw_division_count="$(jq --argjson line "$raw_division_line" '[.[] | select(.rule_id == "SEC009" and .line == $line)] | length' "$RUSTC_JSON")"
guarded_division_line="$(grep -n 'return Ok(a / b);' "$FIXTURE" | cut -d: -f1)"
rustc_sec009_guarded_division_count="$(jq --argjson line "$guarded_division_line" '[.[] | select(.rule_id == "SEC009" and .line == $line)] | length' "$RUSTC_JSON")"
ensure_guarded_division_line="$(grep -n 'Ok(a / b)' "$FIXTURE" | tail -n1 | cut -d: -f1)"
rustc_sec009_ensure_guarded_division_count="$(jq --argjson line "$ensure_guarded_division_line" '[.[] | select(.rule_id == "SEC009" and .line == $line)] | length' "$RUSTC_JSON")"
known_ok_line="$(grep -n 'known_ok.unwrap' "$FIXTURE" | cut -d: -f1)"
known_some_line="$(grep -n 'known_some.expect' "$FIXTURE" | cut -d: -f1)"
unknown_overwrite_line="$(grep -n 'value.expect("unknown overwrite can fail")' "$FIXTURE" | cut -d: -f1)"
rustc_sec008_known_ok_count="$(jq --argjson line "$known_ok_line" '[.[] | select(.rule_id == "SEC008" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec008_known_some_count="$(jq --argjson line "$known_some_line" '[.[] | select(.rule_id == "SEC008" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec008_unknown_overwrite_count="$(jq --argjson line "$unknown_overwrite_line" '[.[] | select(.rule_id == "SEC008" and .line == $line)] | length' "$RUSTC_JSON")"
option_guarded_unwrap_line="$(grep -n 'Ok(value.unwrap())' "$FIXTURE" | head -n1 | cut -d: -f1)"
rustc_sec008_option_guarded_unwrap_count="$(jq --argjson line "$option_guarded_unwrap_line" '[.[] | select(.rule_id == "SEC008" and .line == $line)] | length' "$RUSTC_JSON")"
result_guarded_expect_line="$(grep -n 'value.expect("the error path returned early")' "$FIXTURE" | cut -d: -f1)"
rustc_sec008_result_guarded_expect_count="$(jq --argjson line "$result_guarded_expect_line" '[.[] | select(.rule_id == "SEC008" and .line == $line)] | length' "$RUSTC_JSON")"
some_branch_unwrap_line="$(grep -n 'value.unwrap()' "$FIXTURE" | tail -n2 | head -n1 | cut -d: -f1)"
rustc_sec008_some_branch_unwrap_count="$(jq --argjson line "$some_branch_unwrap_line" '[.[] | select(.rule_id == "SEC008" and .line == $line)] | length' "$RUSTC_JSON")"
guarded_overwrite_unwrap_line="$(grep -n 'Ok(value.unwrap())' "$FIXTURE" | tail -n1 | cut -d: -f1)"
rustc_sec008_guarded_overwrite_unwrap_count="$(jq --argjson line "$guarded_overwrite_unwrap_line" '[.[] | select(.rule_id == "SEC008" and .line == $line)] | length' "$RUSTC_JSON")"
clean_assignment_line="$(grep -n 'pub fn decode_after_clean_assignment' "$FIXTURE" | cut -d: -f1)"
rustc_sec003_clean_assignment_count="$(jq --argjson line "$clean_assignment_line" '[.[] | select(.rule_id == "SEC003" and .line == $line)] | length' "$RUSTC_JSON")"
conditional_clean_assignment_decode_line="$(grep -n 'decoded_after_conditional = RuntimeCall::decode' "$FIXTURE" | cut -d: -f1)"
rustc_sec003_conditional_clean_assignment_count="$(jq --argjson line "$conditional_clean_assignment_decode_line" '[.[] | select(.rule_id == "SEC003" and .line == $line)] | length' "$RUSTC_JSON")"
branch_selected_input_decode_line="$(grep -n 'decoded_after_branch_selected_input = RuntimeCall::decode' "$FIXTURE" | cut -d: -f1)"
rustc_sec003_branch_selected_input_count="$(jq --argjson line "$branch_selected_input_decode_line" '[.[] | select(.rule_id == "SEC003" and .line == $line)] | length' "$RUSTC_JSON")"
match_selected_input_decode_line="$(grep -n 'decoded_after_match_selected_input = RuntimeCall::decode' "$FIXTURE" | cut -d: -f1)"
rustc_sec003_match_selected_input_count="$(jq --argjson line "$match_selected_input_decode_line" '[.[] | select(.rule_id == "SEC003" and .line == $line)] | length' "$RUSTC_JSON")"
match_helper_decode_line="$(grep -n 'decoded_from_match_helper = RuntimeCall::decode' "$FIXTURE" | cut -d: -f1)"
rustc_sec003_match_helper_count="$(jq --argjson line "$match_helper_decode_line" '[.[] | select(.rule_id == "SEC003" and .line == $line)] | length' "$RUSTC_JSON")"
privileged_root_line="$(grep -n 'pub fn privileged_root_vec' "$FIXTURE" | cut -d: -f1)"
privileged_config_line="$(grep -n 'pub fn privileged_config_vec' "$FIXTURE" | cut -d: -f1)"
unknown_config_origin_line="$(grep -n 'pub fn unknown_config_origin_vec' "$FIXTURE" | cut -d: -f1)"
bounded_input_line="$(grep -n 'pub fn bounded_input_vec' "$FIXTURE" | cut -d: -f1)"
comment_only_weight_input_line="$(grep -n 'pub fn comment_only_weight_input' "$FIXTURE" | cut -d: -f1)"
weighted_tuple_line="$(grep -n 'pub fn weighted_tuple' "$FIXTURE" | cut -d: -f1)"
rustc_sec001_privileged_root_count="$(jq --argjson line "$privileged_root_line" '[.[] | select(.rule_id == "SEC001" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec001_privileged_config_count="$(jq --argjson line "$privileged_config_line" '[.[] | select(.rule_id == "SEC001" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec001_unknown_config_origin_count="$(jq --argjson line "$unknown_config_origin_line" '[.[] | select(.rule_id == "SEC001" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec001_bounded_input_count="$(jq --argjson line "$bounded_input_line" '[.[] | select(.rule_id == "SEC001" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec018_privileged_root_count="$(jq --argjson line "$privileged_root_line" '[.[] | select(.rule_id == "SEC018" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec018_privileged_config_count="$(jq --argjson line "$privileged_config_line" '[.[] | select(.rule_id == "SEC018" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec018_unknown_config_origin_count="$(jq --argjson line "$unknown_config_origin_line" '[.[] | select(.rule_id == "SEC018" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec018_comment_only_weight_input_count="$(jq --argjson line "$comment_only_weight_input_line" '[.[] | select(.rule_id == "SEC018" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_sec018_weighted_tuple_count="$(jq --argjson line "$weighted_tuple_line" '[.[] | select(.rule_id == "SEC018" and .line == $line)] | length' "$RUSTC_JSON")"
bounded_storage_iteration_line="$(grep -n 'let bounded_storage_iteration =' "$FIXTURE" | cut -d: -f1)"
rustc_sec011_bounded_iteration_count="$(jq --argjson line "$bounded_storage_iteration_line" '[.[] | select(.rule_id == "SEC011" and .line == $line)] | length' "$RUSTC_JSON")"
dynamically_capped_storage_iteration_line="$(grep -n 'frame_support::storage::types::StorageMap::iter().take(limit)' "$FIXTURE" | cut -d: -f1)"
rustc_sec011_dynamic_iteration_count="$(jq --argjson line "$dynamically_capped_storage_iteration_line" '[.[] | select(.rule_id == "SEC011" and .line == $line)] | length' "$RUSTC_JSON")"
rustc_filtered_count="$(jq 'length' "$RUSTC_FILTERED_JSON")"
rustc_filtered_empty_count="$(jq 'length' "$RUSTC_FILTERED_EMPTY_JSON")"
rustc_rule_filtered_count="$(jq 'length' "$RUSTC_RULE_FILTERED_JSON")"
rustc_rule_filtered_sec008_count="$(jq '[.[] | select(.rule_id == "SEC008")] | length' "$RUSTC_RULE_FILTERED_JSON")"
rustc_rule_filtered_sec009_count="$(jq '[.[] | select(.rule_id == "SEC009")] | length' "$RUSTC_RULE_FILTERED_JSON")"
rustc_rule_filtered_other_count="$(jq '[.[] | select(.rule_id != "SEC008" and .rule_id != "SEC009")] | length' "$RUSTC_RULE_FILTERED_JSON")"
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
echo "rustc filtered findings: $rustc_filtered_count"
echo "rustc filtered empty findings: $rustc_filtered_empty_count"
echo "rustc rule-filtered findings: $rustc_rule_filtered_count"

test "$syn_sec001_count" = "0"
test "$rustc_sec001_count" = "2"
test "$rustc_sec001_privileged_root_count" = "0"
test "$rustc_sec001_privileged_config_count" = "0"
test "$rustc_sec001_unknown_config_origin_count" = "1"
test "$rustc_sec001_bounded_input_count" = "0"
test "$syn_sec002_count" = "4"
test "$rustc_sec002_count" = "3"
test "$syn_sec003_count" = "11"
test "$rustc_sec003_count" = "11"
test "$rustc_sec003_clean_assignment_count" = "0"
test "$rustc_sec003_conditional_clean_assignment_count" = "1"
test "$rustc_sec003_branch_selected_input_count" = "1"
test "$rustc_sec003_match_selected_input_count" = "1"
test "$rustc_sec003_match_helper_count" = "1"
test "$syn_sec008_count" = "11"
test "$rustc_sec008_count" = "4"
test "$rustc_sec008_known_ok_count" = "0"
test "$rustc_sec008_known_some_count" = "0"
test "$rustc_sec008_unknown_overwrite_count" = "1"
test "$rustc_sec008_option_guarded_unwrap_count" = "0"
test "$rustc_sec008_result_guarded_expect_count" = "0"
test "$rustc_sec008_some_branch_unwrap_count" = "0"
test "$rustc_sec008_guarded_overwrite_unwrap_count" = "1"
test "$syn_sec009_count" = "5"
test "$rustc_sec009_count" = "3"
test "$rustc_sec009_guarded_subtraction_count" = "0"
test "$rustc_sec009_ensure_guarded_subtraction_count" = "0"
test "$rustc_sec009_early_return_guarded_subtraction_count" = "0"
test "$rustc_sec009_raw_division_count" = "1"
test "$rustc_sec009_guarded_division_count" = "0"
test "$rustc_sec009_ensure_guarded_division_count" = "0"
test "$syn_sec011_count" = "1"
test "$rustc_sec011_count" = "9"
test "$rustc_sec011_bounded_iteration_count" = "0"
test "$rustc_sec011_dynamic_iteration_count" = "1"
test "$syn_sec012_count" = "6"
test "$rustc_sec012_count" = "3"
test "$syn_sec013_count" = "0"
test "$rustc_sec013_count" = "1"
test "$syn_sec017_count" = "0"
test "$rustc_sec017_count" = "1"
test "$syn_sec018_count" = "0"
test "$rustc_sec018_count" = "5"
test "$rustc_sec018_privileged_root_count" = "1"
test "$rustc_sec018_privileged_config_count" = "1"
test "$rustc_sec018_unknown_config_origin_count" = "1"
test "$rustc_sec018_comment_only_weight_input_count" = "1"
test "$rustc_sec018_weighted_tuple_count" = "0"
test "$rustc_filtered_count" = "42"
test "$rustc_filtered_empty_count" = "0"
test "$rustc_rule_filtered_count" = "7"
test "$rustc_rule_filtered_sec008_count" = "4"
test "$rustc_rule_filtered_sec009_count" = "3"
test "$rustc_rule_filtered_other_count" = "0"
