// BAD: Weight::zero() placeholder in a weight attribute.
// This means the extrinsic is "free" which is almost certainly wrong.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::weight_of_authorize(Weight::zero())]
    #[pallet::call_index(0)]
    pub fn submit_data(origin: OriginFor<T>, data: Vec<u8>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        Data::<T>::put(data);
        Ok(())
    }
}

// Literal zero weights: a `u64` ref-time `0` (BEEFY bridge `submit_commitment`
// on polkadot-sdk master) and an all-zero `Weight::from_parts`.
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(1)]
    #[pallet::weight(0)]
    pub fn submit_commitment(origin: OriginFor<T>) -> DispatchResult {
        ensure_signed(origin)?;
        Ok(())
    }

    #[pallet::call_index(2)]
    #[pallet::weight(Weight::from_parts(0, 0))]
    pub fn submit_more(origin: OriginFor<T>) -> DispatchResult {
        ensure_signed(origin)?;
        Ok(())
    }
}
