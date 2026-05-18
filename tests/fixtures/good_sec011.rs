// GOOD: Dispatchable only touches bounded storage accesses.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn sweep(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        let balance = Accounts::<T>::get(&who);
        Total::<T>::put(balance);
        Ok(())
    }
}
