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
SYNTAX_FIXTURE="$WORK_DIR/substrate/frame/syntax-fixture/src/lib.rs"
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
use crate::frame_support::dispatch::Dispatchable;
use crate::frame_support::traits::Get;
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

#[pallet :: event] pub enum WhitespaceEvent {
    WhitespaceSubmitted { payload: Payload },
}

#[pallet::event] pub enum UnemittedEvent {
    Unemitted { payload: Payload },
}

#[pallet::event] pub enum InternalPayloadEvent {
    InternalPayload { payload: Payload },
}

#[pallet::event] pub enum HelperPayloadEvent {
    HelperPayload { payload: Payload },
}

#[pallet::event] pub enum AliasedHelperPayloadEvent {
    AliasedHelperPayload { payload: Payload },
}

#[pallet::event] pub enum OverwrittenAliasedHelperPayloadEvent {
    OverwrittenAliasedHelperPayload { payload: Payload },
}

#[pallet::event] pub enum ConditionalAliasedHelperPayloadEvent {
    ConditionalAliasedHelperPayload { payload: Payload },
}

#[pallet::event] pub enum MatchAliasedHelperPayloadEvent {
    MatchAliasedHelperPayload { payload: Payload },
}

#[pallet::event] pub enum WeightAccountedPayloadEvent {
    WeightAccountedPayload { payload: Payload },
}

#[pallet::event] pub enum WeightAccountedHelperPayloadEvent {
    WeightAccountedHelperPayload { payload: Payload },
}

#[pallet::event] pub enum MixedWeightHelperPayloadEvent {
    MixedWeightHelperPayload { payload: Payload },
}

pub fn emit_event(payload: Payload) {
    let _ = Event::Submitted { payload };
}

pub struct DiscardError;

pub mod result_currency {
    use super::DiscardError;

    pub struct Currency;

    impl Currency {
        pub fn transfer() -> Result<(), DiscardError> {
            Ok(())
        }

        pub fn try_mutate() -> Result<(), ()> {
            Ok(())
        }
    }
}

pub mod unrelated_currency_result {
    pub struct Currency;

    impl Currency {
        pub fn transfer() -> u32 {
            0
        }
    }
}




pub fn discarded_repatriate_reserved() {
    let _ = frame_support::traits::Currency::repatriate_reserved();
}

#[allow(unused_must_use)]
pub fn standalone_discarded_repatriate_reserved() {
    frame_support::traits::Currency::repatriate_reserved();
}

pub fn checked_repatriate_reserved() -> Result<(), ()> {
    let remaining = frame_support::traits::Currency::repatriate_reserved()?;
    if !remaining.is_zero() {
        return Err(());
    }
    Ok(())
}

pub mod unrelated_currency {
    pub struct Currency;

    impl Currency {
        pub fn repatriate_reserved() -> Result<u32, ()> {
            Ok(0)
        }
    }
}

pub fn unrelated_repatriate_reserved() {
    let _ = unrelated_currency::Currency::repatriate_reserved();
}

#[pallet::call_index(29)]
#[pallet::weight(WeightInfo::emit_weight_accounted(payload.len() as u32))]
pub fn emit_weight_accounted_event(payload: Payload) {
    if payload.len() > 32 {
        return;
    }
    let emitted_payload = payload;
    let _ = WeightAccountedPayloadEvent::WeightAccountedPayload {
        payload: emitted_payload,
    };
}

fn emit_weight_accounted_helper_event(payload: Payload) {
    let _ = WeightAccountedHelperPayloadEvent::WeightAccountedHelperPayload { payload };
}


fn emit_mixed_weight_helper_event(payload: Payload) {
    let _ = MixedWeightHelperPayloadEvent::MixedWeightHelperPayload { payload };
}


pub fn emit_mixed_weight_helper_unaccounted(payload: Payload) {
    emit_mixed_weight_helper_event(payload);
}

pub fn emit_whitespace_event(payload: Payload) {
    let _ = WhitespaceEvent::WhitespaceSubmitted { payload };
}

fn emit_internal_payload_event(payload: Payload) {
    let _ = InternalPayloadEvent::InternalPayload { payload };
}

pub fn emit_internal_payload_event_from_static_value() {
    emit_internal_payload_event(Vec::new());
}

fn emit_helper_payload_event(payload: Payload) {
    let _ = HelperPayloadEvent::HelperPayload { payload };
}

pub fn emit_helper_payload_event_from_input(payload: Payload) {
    emit_helper_payload_event(payload);
}

fn emit_aliased_helper_payload_event(payload: Payload) {
    let _ = AliasedHelperPayloadEvent::AliasedHelperPayload { payload };
}

fn emit_overwritten_aliased_helper_payload_event(payload: Payload) {
    let _ = OverwrittenAliasedHelperPayloadEvent::OverwrittenAliasedHelperPayload { payload };
}

fn emit_conditional_aliased_helper_payload_event(payload: Payload) {
    let _ = ConditionalAliasedHelperPayloadEvent::ConditionalAliasedHelperPayload { payload };
}

fn emit_match_aliased_helper_payload_event(payload: Payload) {
    let _ = MatchAliasedHelperPayloadEvent::MatchAliasedHelperPayload { payload };
}

fn discard_aliased_helper_payload_event(_payload: Payload) {}

pub fn emit_helper_payload_event_from_function_value(payload: Payload) {
    let emit = emit_aliased_helper_payload_event;
    emit(payload);
}

#[allow(unused_assignments)]
pub fn emit_helper_payload_event_from_overwritten_function_value(payload: Payload) {
    let mut emit: fn(Payload) = emit_overwritten_aliased_helper_payload_event;
    emit = discard_aliased_helper_payload_event;
    emit(payload);
}

pub fn emit_helper_payload_event_from_conditionally_overwritten_function_value(
    payload: Payload,
    discard: bool,
) {
    let mut emit: fn(Payload) = emit_conditional_aliased_helper_payload_event;
    if discard {
        emit = discard_aliased_helper_payload_event;
    }
    emit(payload);
}

pub fn emit_helper_payload_event_from_match_overwritten_function_value(
    payload: Payload,
    emit_event: bool,
) {
    let mut emit: fn(Payload) = discard_aliased_helper_payload_event;
    match emit_event {
        true => emit = emit_match_aliased_helper_payload_event,
        false => {},
    }
    emit(payload);
}

pub mod unrelated_event {
    pub enum Event {
        Raw(crate::Payload),
    }
}

pub mod frame_support {
    pub struct Identity;
    pub struct Blake2_128Concat;

    pub mod dispatch {
        pub trait Dispatchable {
            fn dispatch_bypass_filter(&self);
        }
    }

    pub mod storage {
        pub fn with_storage_layer<T, E, F>(f: F) -> Result<T, E>
        where
            F: FnOnce() -> Result<T, E>,
        {
            f()
        }

        pub trait IterableStorageMap {
            fn iter() -> std::vec::IntoIter<u8>;
        }

        pub mod types {
            pub struct StorageValue<K, V>(K, V);
            pub struct StorageMap<P = (), H = (), K = (), V = ()>(P, H, K, V);
            pub struct StorageDoubleMap<P = (), H1 = (), K1 = (), H2 = (), K2 = (), V = ()>(
                P,
                H1,
                K1,
                H2,
                K2,
                V,
            );

            impl StorageMap {
                pub fn iter() -> std::vec::IntoIter<u8> {
                    Vec::new().into_iter()
                }

                pub fn clear_prefix<K>(_key: K, _limit: Option<u32>) {}

                pub fn put() {}

                pub fn insert() {}

                pub fn remove() {}

                pub fn kill() {}
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

        pub trait Get<T> {
            fn get() -> T;
        }

        pub struct Balance(u32);

        impl Balance {
            pub fn is_zero(&self) -> bool {
                self.0 == 0
            }
        }

        pub struct Currency;

        pub struct StorageVersion;

        impl StorageVersion {
            pub fn get() -> u16 {
                0
            }
        }

        impl Currency {
            pub fn repatriate_reserved() -> Result<Balance, ()> {
                Ok(Balance(0))
            }
        }

        pub trait EnsureOrigin {
            fn ensure_origin(_origin: Origin) -> Result<(), ()>;
        }

        pub trait Hooks {
            fn on_runtime_upgrade() {}
            fn on_initialize() {}
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
pub type BoundedStorage = frame_support::storage::types::StorageValue<(), BoundedPayload>;



pub type AliasIndex = u32;




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
pub struct RecursivePayload(pub Box<RecursivePayload>);

impl RuntimeCall {
    pub fn decode(_input: &mut &[u8]) -> Result<Self, ()> {
        Ok(RuntimeCall)
    }

    pub fn decode_with_depth_limit(_limit: usize, _input: &mut &[u8]) -> Result<Self, ()> {
        Ok(RuntimeCall)
    }
}

impl frame_support::dispatch::Dispatchable for RuntimeCall {
    fn dispatch_bypass_filter(&self) {}
}

pub mod unrelated_dispatch {
    pub struct Call;

    impl Call {
        pub fn dispatch_bypass_filter(&self) {}
    }
}

pub fn unguarded_dispatch_bypass(call: RuntimeCall) {
    call.dispatch_bypass_filter();
}

pub fn root_guarded_dispatch_bypass(origin: Origin, call: RuntimeCall) {
    if frame_system::ensure_root(origin).is_ok() {
        call.dispatch_bypass_filter();
    }
}

pub fn unrelated_dispatch_bypass(call: unrelated_dispatch::Call) {
    call.dispatch_bypass_filter();
}

impl MigrationState {
    pub fn decode(_input: &mut &[u8]) -> Result<Self, ()> {
        Ok(MigrationState)
    }
}

impl RecursivePayload {
    pub fn decode(_input: &mut &[u8]) -> Result<Self, ()> {
        Err(())
    }
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

pub fn bounded_storage_iteration() {
    let bounded_storage_iteration = frame_support::storage::types::StorageMap::iter().take(10);
    let _ = bounded_storage_iteration;
}

pub fn literal_bound_storage_iteration() {
    let limit = 10;
    let literal_bound_storage_iteration =
        frame_support::storage::types::StorageMap::iter().take(limit);
    let _ = literal_bound_storage_iteration;
}

pub fn dynamically_capped_storage_iteration(limit: usize) {
    let dynamically_capped_storage_iteration =
        frame_support::storage::types::StorageMap::iter().take(limit);
    let _ = dynamically_capped_storage_iteration;
}

pub fn conditionally_unbounded_storage_iteration(clear: bool, limit: usize) {
    let mut cap = 10;
    if clear {
        cap = limit;
    }
    let _ = frame_support::storage::types::StorageMap::iter().take(cap);
}

#[allow(unused_assignments)]
pub fn conditionally_bounded_storage_iteration(select_first: bool) {
    let mut cap = 10;
    if select_first {
        cap = 20;
    } else {
        cap = 30;
    }
    let _ = frame_support::storage::types::StorageMap::iter().take(cap);
}

pub fn match_conditionally_unbounded_storage_iteration(select_first: bool, limit: usize) {
    let mut cap = 10;
    match select_first {
        true => cap = limit,
        false => {},
    }
    let _ = frame_support::storage::types::StorageMap::iter().take(cap);
}

#[allow(unused_assignments)]
pub fn match_bounded_storage_iteration(select_first: bool) {
    let mut cap = 10;
    match select_first {
        true => cap = 20,
        false => cap = 30,
    }
    let _ = frame_support::storage::types::StorageMap::iter().take(cap);
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

fn hook_fallible_operation() -> Result<(), ()> {
    Err(())
}

pub struct UnprotectedTransactionalHook;

impl frame_support::traits::Hooks for UnprotectedTransactionalHook {
    fn on_initialize() {
        let _: Result<(), ()> = (|| {
            frame_support::storage::types::StorageMap::put();
            hook_fallible_operation()?;
            frame_support::storage::types::StorageMap::kill();
            Ok(())
        })();
    }
}

pub struct LayeredTransactionalHook;

impl frame_support::traits::Hooks for LayeredTransactionalHook {
    fn on_initialize() {
        let _: Result<(), ()> = frame_support::storage::with_storage_layer(|| {
            frame_support::storage::types::StorageMap::put();
            hook_fallible_operation()?;
            frame_support::storage::types::StorageMap::kill();
            Ok(())
        });
    }
}

pub struct AttributeTransactionalHook;

impl frame_support::traits::Hooks for AttributeTransactionalHook {
    #[pallet::transactional]
    fn on_initialize() {
        let _: Result<(), ()> = (|| {
            frame_support::storage::types::StorageMap::put();
            hook_fallible_operation()?;
            frame_support::storage::types::StorageMap::kill();
            Ok(())
        })();
    }
}

pub trait LocalHooks {
    fn on_initialize();
}

pub struct LocalHook;

impl LocalHooks for LocalHook {
    fn on_initialize() {
        let _: Result<(), ()> = (|| {
            frame_support::storage::types::StorageMap::put();
            hook_fallible_operation()?;
            frame_support::storage::types::StorageMap::kill();
            Ok(())
        })();
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



pub struct VersionGuardedStorageMigration;

impl frame_support::traits::OnRuntimeUpgrade for VersionGuardedStorageMigration {
    fn on_runtime_upgrade() {
        let _ = frame_support::traits::StorageVersion::get();
        frame_support::storage::types::StorageMap::put();
    }
}



pub struct UncheckedStorageMigration;

impl frame_support::traits::UncheckedOnRuntimeUpgrade for UncheckedStorageMigration {
    fn on_runtime_upgrade() {
        frame_support::storage::types::StorageMap::put();
    }
}

pub mod unrelated_migration {
    pub trait OnRuntimeUpgrade {
        fn on_runtime_upgrade();
    }
}

pub struct UnrelatedStorageMigration;

impl unrelated_migration::OnRuntimeUpgrade for UnrelatedStorageMigration {
    fn on_runtime_upgrade() {
        frame_support::storage::types::StorageMap::put();
    }
}

pub fn local_iteration() {
    let _ = Domain::iter();
}

pub fn storage_clear_prefix_unbounded() {
    frame_support::storage::types::StorageMap::clear_prefix((), Some(u32::MAX));
}

pub fn storage_clear_prefix_local_unbounded() {
    let limit = Some(u32::MAX);
    frame_support::storage::types::StorageMap::clear_prefix((), limit);
}

pub fn storage_clear_prefix_overwritten_bounded() {
    let mut limit = Some(u32::MAX);
    let _previous_limit = limit;
    limit = Some(10);
    frame_support::storage::types::StorageMap::clear_prefix((), limit);
}

pub fn storage_clear_prefix_conditionally_overwritten_bounded(clear: bool) {
    let mut limit = Some(u32::MAX);
    if clear {
        limit = Some(10);
    }
    frame_support::storage::types::StorageMap::clear_prefix((), limit);
}

pub fn storage_clear_prefix_match_overwritten_bounded(clear: bool) {
    let mut limit = Some(u32::MAX);
    match clear {
        true => limit = Some(10),
        false => {},
    }
    frame_support::storage::types::StorageMap::clear_prefix((), limit);
}

#[allow(unused_assignments)]
pub fn storage_clear_prefix_match_bounded(clear: bool) {
    let mut limit = Some(u32::MAX);
    match clear {
        true => limit = Some(10),
        false => limit = Some(20),
    }
    frame_support::storage::types::StorageMap::clear_prefix((), limit);
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

pub fn storage_clear_prefix_in_storage_layer() -> Result<(), ()> {
    frame_support::storage::with_storage_layer(|| {
        frame_support::storage::types::StorageMap::clear_prefix((), None);
        Ok(())
    })
}

pub fn storage_clear_prefix_bounded_in_storage_layer() -> Result<(), ()> {
    frame_support::storage::with_storage_layer(|| {
        frame_support::storage::types::StorageMap::clear_prefix((), Some(10));
        Ok(())
    })
}

#[allow(dead_code)]
fn private_storage_iteration() {
    let _ = frame_support::storage::types::StorageMap::iter();
}





pub struct ResponseHandler;


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






pub fn decode_via_private_helper_after_match_conditional_clean_assignment(
    mut data: &[u8],
    clear: bool,
) -> Result<RuntimeCall, ()> {
    match clear {
        true => data = &[][..],
        false => {}
    }
    decode_private_after_match_conditional(data)
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

fn decode_private_after_match_conditional(mut data: &[u8]) -> Result<RuntimeCall, ()> {
    RuntimeCall::decode(&mut data)
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


pub fn unwrap_known_ok() -> u32 {
    let known_ok: Result<u32, ()> = Ok(7);
    known_ok.unwrap()
}

pub fn expect_known_some() -> u32 {
    let known_some = Some(7);
    known_some.expect("constructed as Some")
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


pub fn unwrap_inside_some_match(value: Option<u32>) -> u32 {
    match value {
        Some(_) => value.unwrap(),
        None => 0,
    }
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


pub fn guarded_subtraction(a: u32, b: u32) -> Result<u32, ()> {
    if a >= b {
        return Ok(a - b);
    }
    Err(())
}

pub fn conjunction_guarded_subtraction(a: u32, b: u32, enabled: bool) -> Result<u32, ()> {
    if a >= b && enabled {
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



pub fn guarded_division(a: u32, b: u32) -> Result<u32, ()> {
    if b != 0 {
        return Ok(a / b);
    }
    Err(())
}

pub fn conjunction_guarded_division(a: u32, b: u32, enabled: bool) -> Result<u32, ()> {
    if b != 0 && enabled {
        return Ok(a / b);
    }
    Err(())
}



pub fn guarded_positive_division(a: i32, b: i32) -> Result<i32, ()> {
    if b > 0 {
        return Ok(a / b);
    }
    Err(())
}






pub trait GetConfig {
    type Period: frame_support::traits::Get<u32>;
}






pub fn raw_integer_via_private_helper(a: u32, b: u32) -> Result<u32, ()> {
    Ok(reachable_private_raw_integer(a, b))
}

fn reachable_private_raw_integer(a: u32, b: u32) -> u32 {
    a + b
}

pub fn raw_integer_via_function_value(a: u32, b: u32) -> Result<u32, ()> {
    let helper = function_value_raw_integer;
    Ok(helper(a, b))
}

fn function_value_raw_integer(a: u32, b: u32) -> u32 {
    a + b
}

pub fn raw_integer_via_function_value_alias(a: u32, b: u32) -> Result<u32, ()> {
    let mut helper = function_value_alias_raw_integer;
    let alias = helper;
    helper = alias;
    Ok(helper(a, b))
}

fn function_value_alias_raw_integer(a: u32, b: u32) -> u32 {
    a + b
}

pub fn raw_integer_via_conditional_function_value(
    a: u32,
    b: u32,
    replace: bool,
) -> Result<u32, ()> {
    let mut helper: fn(u32, u32) -> u32 = conditional_function_value_raw_integer;
    if replace {
        helper = conditional_function_value_safe_integer;
    }
    Ok(helper(a, b))
}

fn conditional_function_value_raw_integer(a: u32, b: u32) -> u32 {
    a + b
}

fn conditional_function_value_safe_integer(a: u32, b: u32) -> u32 {
    a.saturating_add(b)
}

pub fn raw_integer_via_match_function_value(
    a: u32,
    b: u32,
    replace: bool,
) -> Result<u32, ()> {
    let mut helper: fn(u32, u32) -> u32 = match_function_value_raw_integer;
    match replace {
        true => helper = match_function_value_safe_integer,
        false => {}
    }
    Ok(helper(a, b))
}

fn match_function_value_raw_integer(a: u32, b: u32) -> u32 {
    a + b
}

fn match_function_value_safe_integer(a: u32, b: u32) -> u32 {
    a.saturating_add(b)
}

#[allow(unused_assignments)]
pub fn overwritten_function_value_is_not_reachable(
    a: u32,
    b: u32,
    replace: bool,
) -> Result<u32, ()> {
    let mut helper: fn(u32, u32) -> u32 = overwritten_function_value_raw_integer;
    if replace {
        helper = overwritten_function_value_safe_integer;
    } else {
        helper = overwritten_function_value_other_safe_integer;
    }
    Ok(helper(a, b))
}

fn overwritten_function_value_raw_integer(a: u32, b: u32) -> u32 {
    a + b
}

fn overwritten_function_value_safe_integer(a: u32, b: u32) -> u32 {
    a.saturating_add(b)
}

fn overwritten_function_value_other_safe_integer(a: u32, b: u32) -> u32 {
    a.saturating_add(b)
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
SEC006 = true
SEC010 = true
SEC011 = true
SEC012 = true
SEC015 = true
TOML

SYN_JSON="$WORK_DIR/syn-hard-rules.json"
RUSTC_JSON="$WORK_DIR/rustc-hard-rules.json"
RUSTC_FILTERED_JSON="$WORK_DIR/rustc-hard-rules-filtered.json"
RUSTC_FILTERED_EMPTY_JSON="$WORK_DIR/rustc-hard-rules-filtered-empty.json"
RUSTC_RULE_FILTERED_JSON="$WORK_DIR/rustc-hard-rules-rule-filtered.json"

cargo +1.93.0 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" --bin polkadot-linter -- \
  -c "$CONFIG_FILE" \
  "$WORK_DIR" \
  --rules SEC006,SEC010,SEC011,SEC012,SEC015 \
  -f json > "$SYN_JSON"

cargo +nightly-2025-09-01 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver \
  --bin polkadot-linter-driver -- \
  "$FIXTURE" \
  --crate-type lib \
  --edition 2021 \
  --emit metadata \
  --out-dir "$RUSTC_TARGET_DIR" > "$RUSTC_JSON"

POLKADOT_LINTER_DRIVER_FILE_FILTERS_JSON='["hard-rules-fixture/src/lib.rs"]' \
  cargo +nightly-2025-09-01 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
    --features rustc-driver \
    --bin polkadot-linter-driver -- \
    "$FIXTURE" \
    --crate-type lib \
    --edition 2021 \
    --emit metadata \
    --out-dir "$RUSTC_TARGET_DIR" > "$RUSTC_FILTERED_JSON"

POLKADOT_LINTER_DRIVER_FILE_FILTERS_JSON='["does-not-match.rs"]' \
  cargo +nightly-2025-09-01 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
    --features rustc-driver \
    --bin polkadot-linter-driver -- \
    "$FIXTURE" \
    --crate-type lib \
    --edition 2021 \
    --emit metadata \
    --out-dir "$RUSTC_TARGET_DIR" > "$RUSTC_FILTERED_EMPTY_JSON"

POLKADOT_LINTER_DRIVER_RULES="SEC011,SEC012" \
  cargo +nightly-2025-09-01 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
    --features rustc-driver \
    --bin polkadot-linter-driver -- \
    "$FIXTURE" \
    --crate-type lib \
    --edition 2021 \
    --emit metadata \
    --out-dir "$RUSTC_TARGET_DIR" > "$RUSTC_RULE_FILTERED_JSON"

syn_sec006_count="$(jq '[.[] | select(.rule_id == "SEC006")] | length' "$SYN_JSON")"
syn_sec010_count="$(jq '[.[] | select(.rule_id == "SEC010")] | length' "$SYN_JSON")"
syn_sec011_count="$(jq '[.[] | select(.rule_id == "SEC011")] | length' "$SYN_JSON")"
syn_sec012_count="$(jq '[.[] | select(.rule_id == "SEC012")] | length' "$SYN_JSON")"
syn_sec015_count="$(jq '[.[] | select(.rule_id == "SEC015")] | length' "$SYN_JSON")"
rustc_sec006_count="$(jq '[.[] | select(.rule_id == "SEC006")] | length' "$RUSTC_JSON")"
rustc_sec010_count="$(jq '[.[] | select(.rule_id == "SEC010")] | length' "$RUSTC_JSON")"
rustc_sec011_count="$(jq '[.[] | select(.rule_id == "SEC011")] | length' "$RUSTC_JSON")"
rustc_sec012_count="$(jq '[.[] | select(.rule_id == "SEC012")] | length' "$RUSTC_JSON")"
rustc_sec015_count="$(jq '[.[] | select(.rule_id == "SEC015")] | length' "$RUSTC_JSON")"
discarded_repatriate_line="$(grep -n 'pub fn discarded_repatriate_reserved' "$FIXTURE" | cut -d: -f1)"
standalone_discarded_repatriate_line="$(grep -n 'pub fn standalone_discarded_repatriate_reserved' "$FIXTURE" | cut -d: -f1)"
checked_repatriate_line="$(grep -n 'pub fn checked_repatriate_reserved' "$FIXTURE" | cut -d: -f1)"
unrelated_repatriate_line="$(grep -n 'pub fn unrelated_repatriate_reserved' "$FIXTURE" | cut -d: -f1)"
rustc_sec006_discarded_count="$(jq --argjson line "$discarded_repatriate_line" '[.[] | select(.rule_id == "SEC006" and .line >= $line and .line <= ($line + 2))] | length' "$RUSTC_JSON")"
rustc_sec006_standalone_discarded_count="$(jq --argjson line "$standalone_discarded_repatriate_line" '[.[] | select(.rule_id == "SEC006" and .line >= $line and .line <= ($line + 2))] | length' "$RUSTC_JSON")"
rustc_sec006_checked_count="$(jq --argjson line "$checked_repatriate_line" '[.[] | select(.rule_id == "SEC006" and .line >= $line and .line <= ($line + 6))] | length' "$RUSTC_JSON")"
rustc_sec006_unrelated_count="$(jq --argjson line "$unrelated_repatriate_line" '[.[] | select(.rule_id == "SEC006" and .line >= $line and .line <= ($line + 2))] | length' "$RUSTC_JSON")"
unguarded_dispatch_bypass_line="$(grep -n 'pub fn unguarded_dispatch_bypass' "$FIXTURE" | cut -d: -f1)"
root_guarded_dispatch_bypass_line="$(grep -n 'pub fn root_guarded_dispatch_bypass' "$FIXTURE" | cut -d: -f1)"
unrelated_dispatch_bypass_line="$(grep -n 'pub fn unrelated_dispatch_bypass' "$FIXTURE" | cut -d: -f1)"
rustc_sec015_unguarded_count="$(jq --argjson line "$unguarded_dispatch_bypass_line" '[.[] | select(.rule_id == "SEC015" and .line >= $line and .line <= ($line + 2))] | length' "$RUSTC_JSON")"
rustc_sec015_root_guarded_count="$(jq --argjson line "$root_guarded_dispatch_bypass_line" '[.[] | select(.rule_id == "SEC015" and .line >= $line and .line <= ($line + 4))] | length' "$RUSTC_JSON")"
rustc_sec015_unrelated_count="$(jq --argjson line "$unrelated_dispatch_bypass_line" '[.[] | select(.rule_id == "SEC015" and .line >= $line and .line <= ($line + 2))] | length' "$RUSTC_JSON")"
bounded_storage_iteration_line="$(grep -n 'let bounded_storage_iteration =' "$FIXTURE" | cut -d: -f1)"
rustc_sec011_bounded_iteration_count="$(jq --argjson line "$bounded_storage_iteration_line" '[.[] | select(.rule_id == "SEC011" and .line == $line)] | length' "$RUSTC_JSON")"
literal_bound_storage_iteration_line="$(grep -n 'let literal_bound_storage_iteration =' "$FIXTURE" | cut -d: -f1)"
rustc_sec011_literal_bound_iteration_count="$(jq --argjson line "$literal_bound_storage_iteration_line" '[.[] | select(.rule_id == "SEC011" and .line == $line)] | length' "$RUSTC_JSON")"
dynamically_capped_storage_iteration_line="$(grep -n 'frame_support::storage::types::StorageMap::iter().take(limit)' "$FIXTURE" | tail -n1 | cut -d: -f1)"
rustc_sec011_dynamic_iteration_count="$(jq --argjson line "$dynamically_capped_storage_iteration_line" '[.[] | select(.rule_id == "SEC011" and .line == $line)] | length' "$RUSTC_JSON")"
conditionally_unbounded_iteration_line="$(grep -n 'pub fn conditionally_unbounded_storage_iteration' "$FIXTURE" | cut -d: -f1)"
rustc_sec011_conditionally_unbounded_count="$(jq --argjson line "$conditionally_unbounded_iteration_line" '[.[] | select(.rule_id == "SEC011" and .line >= $line and .line <= ($line + 6))] | length' "$RUSTC_JSON")"
conditionally_bounded_iteration_line="$(grep -n 'pub fn conditionally_bounded_storage_iteration' "$FIXTURE" | cut -d: -f1)"
rustc_sec011_conditionally_bounded_count="$(jq --argjson line "$conditionally_bounded_iteration_line" '[.[] | select(.rule_id == "SEC011" and .line >= $line and .line <= ($line + 8))] | length' "$RUSTC_JSON")"
match_conditionally_unbounded_iteration_line="$(grep -n 'pub fn match_conditionally_unbounded_storage_iteration' "$FIXTURE" | cut -d: -f1)"
rustc_sec011_match_conditionally_unbounded_count="$(jq --argjson line "$match_conditionally_unbounded_iteration_line" '[.[] | select(.rule_id == "SEC011" and .line >= $line and .line <= ($line + 8))] | length' "$RUSTC_JSON")"
match_bounded_iteration_line="$(grep -n 'pub fn match_bounded_storage_iteration' "$FIXTURE" | cut -d: -f1)"
rustc_sec011_match_bounded_count="$(jq --argjson line "$match_bounded_iteration_line" '[.[] | select(.rule_id == "SEC011" and .line >= $line and .line <= ($line + 8))] | length' "$RUSTC_JSON")"
local_unbounded_clear_prefix_line="$(grep -n 'pub fn storage_clear_prefix_local_unbounded' "$FIXTURE" | cut -d: -f1)"
rustc_sec012_local_unbounded_count="$(jq --argjson line "$local_unbounded_clear_prefix_line" '[.[] | select(.rule_id == "SEC012" and .line >= $line and .line <= ($line + 3))] | length' "$RUSTC_JSON")"
overwritten_bounded_clear_prefix_line="$(grep -n 'pub fn storage_clear_prefix_overwritten_bounded' "$FIXTURE" | cut -d: -f1)"
rustc_sec012_overwritten_bounded_count="$(jq --argjson line "$overwritten_bounded_clear_prefix_line" '[.[] | select(.rule_id == "SEC012" and .line >= $line and .line <= ($line + 4))] | length' "$RUSTC_JSON")"
conditionally_bounded_clear_prefix_line="$(grep -n 'pub fn storage_clear_prefix_conditionally_overwritten_bounded' "$FIXTURE" | cut -d: -f1)"
rustc_sec012_conditionally_bounded_count="$(jq --argjson line "$conditionally_bounded_clear_prefix_line" '[.[] | select(.rule_id == "SEC012" and .line >= $line and .line <= ($line + 6))] | length' "$RUSTC_JSON")"
match_overwritten_bounded_clear_prefix_line="$(grep -n 'pub fn storage_clear_prefix_match_overwritten_bounded' "$FIXTURE" | cut -d: -f1)"
rustc_sec012_match_overwritten_bounded_count="$(jq --argjson line "$match_overwritten_bounded_clear_prefix_line" '[.[] | select(.rule_id == "SEC012" and .line >= $line and .line <= ($line + 8))] | length' "$RUSTC_JSON")"
match_bounded_clear_prefix_line="$(grep -n 'pub fn storage_clear_prefix_match_bounded' "$FIXTURE" | cut -d: -f1)"
storage_layer_clear_prefix_line="$(grep -n 'pub fn storage_clear_prefix_in_storage_layer' "$FIXTURE" | cut -d: -f1)"
rustc_sec012_storage_layer_count="$(jq --argjson line "$storage_layer_clear_prefix_line" '[.[] | select(.rule_id == "SEC012" and .line == ($line + 2))] | length' "$RUSTC_JSON")"
storage_layer_bounded_line="$(grep -n 'pub fn storage_clear_prefix_bounded_in_storage_layer' "$FIXTURE" | cut -d: -f1)"
rustc_sec012_storage_layer_bounded_count="$(jq --argjson line "$storage_layer_bounded_line" '[.[] | select(.rule_id == "SEC012" and .line > $line and .line <= ($line + 4))] | length' "$RUSTC_JSON")"
rustc_sec012_match_bounded_count="$(jq --argjson line "$match_bounded_clear_prefix_line" '[.[] | select(.rule_id == "SEC012" and .line >= $line and .line <= ($line + 8))] | length' "$RUSTC_JSON")"
unprotected_transactional_hook_line="$(grep -n 'pub struct UnprotectedTransactionalHook' "$FIXTURE" | cut -d: -f1)"
layered_transactional_hook_line="$(grep -n 'pub struct LayeredTransactionalHook' "$FIXTURE" | cut -d: -f1)"
attribute_transactional_hook_line="$(grep -n 'pub struct AttributeTransactionalHook' "$FIXTURE" | cut -d: -f1)"
local_transactional_hook_line="$(grep -n 'pub struct LocalHook' "$FIXTURE" | cut -d: -f1)"
rustc_sec010_unprotected_count="$(jq --argjson line "$unprotected_transactional_hook_line" '[.[] | select(.rule_id == "SEC010" and .line >= $line and .line <= ($line + 12))] | length' "$RUSTC_JSON")"
rustc_sec010_layered_count="$(jq --argjson line "$layered_transactional_hook_line" '[.[] | select(.rule_id == "SEC010" and .line >= $line and .line <= ($line + 12))] | length' "$RUSTC_JSON")"
rustc_sec010_attribute_count="$(jq --argjson line "$attribute_transactional_hook_line" '[.[] | select(.rule_id == "SEC010" and .line >= $line and .line <= ($line + 13))] | length' "$RUSTC_JSON")"
rustc_sec010_local_hook_count="$(jq --argjson line "$local_transactional_hook_line" '[.[] | select(.rule_id == "SEC010" and .line >= $line and .line <= ($line + 12))] | length' "$RUSTC_JSON")"
rustc_filtered_count="$(jq 'length' "$RUSTC_FILTERED_JSON")"
rustc_filtered_empty_count="$(jq 'length' "$RUSTC_FILTERED_EMPTY_JSON")"
rustc_rule_filtered_count="$(jq 'length' "$RUSTC_RULE_FILTERED_JSON")"
rustc_rule_filtered_sec011_count="$(jq '[.[] | select(.rule_id == "SEC011")] | length' "$RUSTC_RULE_FILTERED_JSON")"
rustc_rule_filtered_sec012_count="$(jq '[.[] | select(.rule_id == "SEC012")] | length' "$RUSTC_RULE_FILTERED_JSON")"
rustc_rule_filtered_other_count="$(jq '[.[] | select(.rule_id != "SEC011" and .rule_id != "SEC012")] | length' "$RUSTC_RULE_FILTERED_JSON")"
echo "syntax SEC006 findings: $syn_sec006_count"
echo "rustc SEC006 findings: $rustc_sec006_count"
echo "syntax SEC010 findings: $syn_sec010_count"
echo "rustc SEC010 findings: $rustc_sec010_count"
echo "syntax SEC011 findings: $syn_sec011_count"
echo "rustc SEC011 findings: $rustc_sec011_count"
echo "syntax SEC012 findings: $syn_sec012_count"
echo "rustc SEC012 findings: $rustc_sec012_count"
echo "syntax SEC015 findings: $syn_sec015_count"
echo "rustc SEC015 findings: $rustc_sec015_count"
echo "rustc filtered findings: $rustc_filtered_count"
echo "rustc filtered empty findings: $rustc_filtered_empty_count"
echo "rustc rule-filtered findings: $rustc_rule_filtered_count"

test "$syn_sec006_count" = "0"
test "$rustc_sec006_count" = "2"
test "$rustc_sec006_discarded_count" = "1"
test "$rustc_sec006_standalone_discarded_count" = "1"
test "$rustc_sec006_checked_count" = "0"
test "$rustc_sec006_unrelated_count" = "0"
test "$syn_sec010_count" = "0"
test "$rustc_sec010_count" = "1"
test "$rustc_sec010_unprotected_count" = "1"
test "$rustc_sec010_layered_count" = "0"
test "$rustc_sec010_attribute_count" = "0"
test "$rustc_sec010_local_hook_count" = "0"
test "$syn_sec011_count" = "0"
# Upgrade-only iteration is bookkeeping; runtime iteration positives remain above.
test "$rustc_sec011_count" = "6"
test "$rustc_sec011_bounded_iteration_count" = "0"
test "$rustc_sec011_literal_bound_iteration_count" = "0"
test "$rustc_sec011_dynamic_iteration_count" = "1"
test "$rustc_sec011_conditionally_unbounded_count" = "1"
test "$rustc_sec011_conditionally_bounded_count" = "0"
test "$rustc_sec011_match_conditionally_unbounded_count" = "1"
test "$rustc_sec011_match_bounded_count" = "0"
test "$syn_sec012_count" = "0"
test "$rustc_sec012_count" = "7"
test "$rustc_sec012_storage_layer_count" = "1"
test "$rustc_sec012_storage_layer_bounded_count" = "0"
test "$rustc_sec012_local_unbounded_count" = "1"
test "$rustc_sec012_overwritten_bounded_count" = "0"
test "$rustc_sec012_conditionally_bounded_count" = "1"
test "$rustc_sec012_match_overwritten_bounded_count" = "1"
test "$rustc_sec012_match_bounded_count" = "0"
test "$syn_sec015_count" = "0"
test "$rustc_sec015_count" = "1"
test "$rustc_sec015_unguarded_count" = "1"
test "$rustc_sec015_root_guarded_count" = "0"
test "$rustc_sec015_unrelated_count" = "0"
test "$rustc_filtered_count" = "17"
test "$rustc_filtered_empty_count" = "0"
test "$rustc_rule_filtered_count" = "13"
test "$rustc_rule_filtered_sec011_count" = "6"
test "$rustc_rule_filtered_sec012_count" = "7"
test "$rustc_rule_filtered_other_count" = "0"

# Run additional context regressions independently so existing positive counts stay explicit.
NOISE_FIXTURE="$WORK_DIR/context.rs"
cat > "$NOISE_FIXTURE" <<'RS'
#![feature(register_tool)]
#![register_tool(pallet)]
pub mod frame_support {
    pub mod traits {
        pub trait BuildGenesisConfig { fn build(&self); }
        pub trait OnRuntimeUpgrade { fn on_runtime_upgrade(); }
        pub trait UncheckedOnRuntimeUpgrade { fn on_runtime_upgrade(); }
        pub trait Hooks { fn try_state(); }
        pub trait EnsureOrigin { fn ensure_origin(origin: ()) -> Result<(), ()>; }
    }
    pub mod weights {
        pub struct RuntimeDbWeight;
        impl RuntimeDbWeight { pub fn reads(&self, n: u64) -> u64 { n } }
    }
    pub mod storage {
        pub mod types {
            pub struct StorageMap;
            impl StorageMap { pub fn iter() -> std::vec::IntoIter<u8> { vec![1].into_iter() } }
        }
        pub fn with_storage_layer<T, E>(f: impl FnOnce() -> Result<T, E>) -> Result<T, E> { f() }
    }
}
fn check_len(values: &Vec<u8>) -> Result<(), ()> {
    if values.len() > 32 { return Err(()); }
    Ok(())
}
#[pallet::call_index(0)] #[pallet::weight(1)]
pub fn wrapped_helper(values: Vec<u8>) -> Result<(), ()> {
    frame_support::storage::with_storage_layer(|| {
        let unrelated = 1;
        let _ = unrelated;
        check_len(&values)?;
        for _ in values {}
        Ok(())
    })
}
#[pallet::call_index(4)] #[pallet::weight(1)]
pub fn aliased_length(values: Vec<u8>) -> Result<(), ()> {
    let count = values.len();
    let limit = 32;
    if count > limit { return Err(()); }
    for _ in values {}
    Ok(())
}
fn successful_early_exit(values: &Vec<u8>) -> Result<(), ()> {
    if values.len() > 32 { return Ok(()); }
    Ok(())
}
pub trait Get { fn get() -> usize; }
pub struct Limit;
pub struct OtherLimit;
impl Get for Limit { fn get() -> usize { 32 } }
impl Get for OtherLimit { fn get() -> usize { 64 } }
pub struct BoundedVec<T, S>(Vec<T>, core::marker::PhantomData<S>);
impl<T, S> BoundedVec<T, S> {
    fn len(&self) -> usize { self.0.len() }
    fn try_push(&mut self, value: T) -> Result<(), u8> { self.0.push(value); Ok(()) }
}
pub fn spare_capacity(values: &mut BoundedVec<u8, Limit>) {
    if values.len() < Limit::get() {
        let _ = values.try_push(1);
    }
}

pub struct Genesis;
impl frame_support::traits::BuildGenesisConfig for Genesis {
    fn build(&self) { genesis_only(); shared_with_runtime(); exported_api(None); }
}
fn genesis_only() -> u8 { None::<u8>.unwrap() }
pub(crate) fn shared_with_runtime() -> u8 { None::<u8>.unwrap() }
#[pallet::call_index(20)] #[pallet::weight(1)]
pub fn runtime_path() { shared_with_runtime(); }
pub fn exported_api(value: Option<u8>) -> u8 { value.unwrap() }
pub fn documented_proof(value: Result<u8, u8>) -> u8 {
    value.expect("the input invariant is established; qed")
}

fn ensure_signed(_: ()) -> Result<(), ()> { Ok(()) }
#[pallet::call_index(21)] #[pallet::weight(1)]
pub fn migrate_user(origin: ()) -> Result<(), ()> {
    ensure_signed(origin)?;
    for _ in frame_support::storage::types::StorageMap::iter() {} // SEC011-positive
    Ok(())
}
#[pallet::call_index(22)] #[pallet::weight(1)]
pub fn call_migration(origin: ()) -> Result<(), ()> {
    ensure_signed(origin)?;
    migration::shared();
    Ok(())
}
pub mod migration {
    pub fn shared() {
        for _ in super::frame_support::storage::types::StorageMap::iter() {} // SEC011-positive
        let _ = super::frame_support::weights::RuntimeDbWeight.reads(1);
    }
}
pub struct Upgrade;
impl frame_support::traits::OnRuntimeUpgrade for Upgrade {
    fn on_runtime_upgrade() { upgrade_only(); migration::shared(); }
}
fn upgrade_only() {
    let _ = [1u64].map(|n| frame_support::weights::RuntimeDbWeight.reads(n));
    for _ in frame_support::storage::types::StorageMap::iter() {}
    let _ = frame_support::weights::RuntimeDbWeight.reads(1);
}
pub struct UncheckedUpgrade;
impl frame_support::traits::UncheckedOnRuntimeUpgrade for UncheckedUpgrade {
    fn on_runtime_upgrade() { upgrade_only(); }
}
pub struct TryState;
impl frame_support::traits::Hooks for TryState {
    fn try_state() { upgrade_only(); }
}
#[cfg(feature = "try-runtime")]
pub mod inspection {
    pub fn inspect() {
        for _ in super::frame_support::storage::types::StorageMap::iter() {}
        let _ = super::frame_support::weights::RuntimeDbWeight.reads(1);
    }
}
use frame_support::traits::EnsureOrigin;
pub trait Config { type CreateOrigin: EnsureOrigin; type UserOrigin: EnsureOrigin; }
#[pallet::call_index(27)] #[pallet::weight(1)]
pub fn wide_length(values: Vec<u8>) -> Result<(), ()> {
    let n = values.len() as u64;
    if n > 32 { return Err(()); }
    for _ in values {}
    Ok(())
}
fn bypass_check(values: &Vec<u8>, bypass: bool) -> Result<(), ()> {
    if bypass { return Ok(()); }
    if values.len() > 32 { return Err(()); }
    Ok(())
}
macro_rules! defensive_user { ($value:expr) => { debug_assert!($value) }; }

RS
CONTEXT_JSON="$WORK_DIR/context.json"
cargo +nightly-2025-09-01 run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" \
  --features rustc-driver --bin polkadot-linter-driver -- \
  '--cfg=feature="try-runtime"' "$NOISE_FIXTURE" --crate-type lib --edition 2021 --emit metadata --out-dir "$RUSTC_TARGET_DIR" > "$CONTEXT_JSON"
while IFS=: read -r line _; do
  test "$(jq --argjson line "$line" '[.[] | select(.rule_id == "SEC011" and .line == $line)] | length' "$CONTEXT_JSON")" -eq 1
done < <(grep -n "SEC011-positive" "$NOISE_FIXTURE")
test "$(jq '[.[] | select(.rule_id == "SEC011")] | length' "$CONTEXT_JSON")" -eq 2
