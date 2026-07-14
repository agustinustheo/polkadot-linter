#[pallet::weight(1u32 + 2)]
pub fn raw_weight() {}

#[pallet::weight(1u32.saturating_add(2))]
pub fn saturating_weight() {}

pub mod frame_support {
    pub mod storage {
        pub mod types {
            pub struct StorageValue;

            impl StorageValue {
                pub fn get() -> u32 {
                    1
                }
            }
        }
    }
}

pub trait GetDispatchInfo {
    fn get_dispatch_info(&self) -> u32;
}

pub struct Proposal;

impl GetDispatchInfo for Proposal {
    fn get_dispatch_info(&self) -> u32 {
        1
    }
}

pub struct MaxItems;

impl MaxItems {
    pub fn get() -> u32 {
        10
    }
}

#[pallet::weight(frame_support::storage::types::StorageValue::get())]
pub fn storage_read_weight() {}

#[pallet::weight({
    let proposal = Proposal;
    proposal.get_dispatch_info()
})]
pub fn dispatch_info_weight() {}

#[pallet::weight(MaxItems::get())]
pub fn config_weight() {}

#[cfg_attr(feature = "enabled-weight", pallet::weight(3u32 + 4))]
pub fn disabled_cfg_weight() {}

#[cfg_attr(not(any()), pallet::weight(5u32 + 6))]
pub fn enabled_cfg_weight() {}
