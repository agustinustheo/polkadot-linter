// BAD: Extrinsic accepts unbounded Vec<T> parameter.
// Attacker can pass vec![0u8; 10_000_000] to exhaust memory/weight.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn submit_data(origin: OriginFor<T>, data: Vec<u8>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        Data::<T>::put(data);
        Ok(())
    }
}
