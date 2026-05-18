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
