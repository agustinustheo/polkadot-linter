// GOOD: Standard concat hasher is used for AccountId keys.

#[pallet::storage]
pub type Accounts<T: Config> =
    StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>, ValueQuery>;
