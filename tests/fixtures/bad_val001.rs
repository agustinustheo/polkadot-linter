use frame_support::{dispatch::DispatchResult, ensure, pallet_prelude::*};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    #[pallet::storage]
    pub type Members<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, (), OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CandidateAccepted(T::AccountId),
    }

    #[pallet::error]
    pub enum Error<T> {
        SelfNomination,
        AlreadyMember,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn accept_candidate(origin: OriginFor<T>, candidate: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let existing = Members::<T>::get(&candidate);

            ensure!(candidate != who, Error::<T>::SelfNomination);
            ensure!(existing.is_none(), Error::<T>::AlreadyMember);

            Members::<T>::insert(&candidate, ());
            Self::deposit_event(Event::CandidateAccepted(candidate));
            Ok(())
        }
    }
}
