// BAD: Identity hasher is used for AccountId keys.

#[pallet::storage]
pub type Accounts<T: Config> =
    StorageMap<_, Identity, T::AccountId, BalanceOf<T>, ValueQuery>;
