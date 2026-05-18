use frame_support::{
    dispatch::{DispatchResultWithPostInfo, Pays},
    ensure, pallet_prelude::*,
};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::storage]
    pub type Enabled<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub type Notes<T: Config> = StorageMap<_, Blake2_128Concat, u32, Vec<u8>, OptionQuery>;

    #[pallet::error]
    pub enum Error<T> {
        Disabled,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn submit_free_note(
            origin: OriginFor<T>,
            note_id: u32,
            note: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            ensure_signed(origin)?;
            ensure!(Enabled::<T>::get(), Error::<T>::Disabled);

            Notes::<T>::insert(note_id, note);
            Ok(Pays::No.into())
        }
    }
}
