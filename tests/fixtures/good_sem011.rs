// GOOD: Uses a benchmarked weight function instead of Weight::zero().

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::weight_of_authorize(T::WeightInfo::authorize_submit_data())]
    #[pallet::call_index(0)]
    pub fn submit_data(origin: OriginFor<T>, data: Vec<u8>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        Data::<T>::put(data);
        Ok(())
    }
}
