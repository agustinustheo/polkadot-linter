// BAD: Storage iteration in a dispatchable scales with chain state.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn sweep(origin: OriginFor<T>) -> DispatchResult {
        let _who = ensure_signed(origin)?;
        for (_account, balance) in Accounts::<T>::iter() {
            Total::<T>::put(balance);
        }
        Ok(())
    }
}
