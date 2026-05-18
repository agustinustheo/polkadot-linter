// BAD: Storage collection is unbounded but missing #[pallet::unbounded].

#[pallet::storage]
pub type PendingItems<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;
