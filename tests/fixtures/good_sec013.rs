// GOOD: Unbounded storage is explicitly annotated.

#[pallet::unbounded]
#[pallet::storage]
pub type PendingItems<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;
