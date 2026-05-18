use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn cleanup(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;

            // This optimisation avoids iterating over empty shards.
            log::info!("cleanup requested");

            Ok(())
        }
    }
}
