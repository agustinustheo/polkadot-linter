// BAD: Identity hasher is used for low-entropy numeric keys.

#[pallet::storage]
pub type UserScores<T: Config> = StorageMap<_, Identity, u32, BalanceOf<T>, ValueQuery>;
