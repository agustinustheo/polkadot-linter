use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type WeightInfo: WeightInfo;
    }

    pub trait WeightInfo {
        fn force_release() -> Weight;
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::force_release().saturating_mul(recipients.len() as u64))]
        pub fn force_release(origin: OriginFor<T>, recipients: Vec<T::AccountId>) -> DispatchResult {
            ensure_root(origin)?;

            for recipient in recipients {
                log::info!("released hold for {:?}", recipient);
            }

            Ok(())
        }
    }
}
