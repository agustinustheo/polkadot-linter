use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::SaturatedConversion;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn schedule_cleanup(origin: OriginFor<T>, pages: Vec<u16>) -> DispatchResult {
            ensure_signed(origin)?;

            let keys = pages
                .into_iter()
                .map(|page| page.saturated_into::<u32>())
                .collect::<Vec<_>>();
            log::debug!("queued {} cleanup pages", keys.len());

            Ok(())
        }
    }
}
