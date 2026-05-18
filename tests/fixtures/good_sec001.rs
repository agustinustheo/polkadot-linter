// GOOD: Uses BoundedVec with an explicit max length.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn submit_data(origin: OriginFor<T>, data: BoundedVec<u8, T::MaxDataLen>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        Data::<T>::put(data);
        Ok(())
    }
}
