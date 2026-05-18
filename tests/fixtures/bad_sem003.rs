use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::storage]
    pub type PendingApprovals<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, T::AccountId, OptionQuery>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn flush(origin: OriginFor<T>) -> DispatchResult {
            ensure_signed(origin)?;

            let approvals = PendingApprovals::<T>::iter_keys().collect::<Vec<_>>();
            for approval in approvals.iter() {
                PendingApprovals::<T>::remove(approval);
            }

            Ok(())
        }
    }
}
