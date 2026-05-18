// GOOD: Weight uses only pre-benchmarked functions, no DB access.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::weight(T::WeightInfo::process_pending(T::MaxPendingItems::get()))]
    #[pallet::call_index(0)]
    pub fn process_pending(origin: OriginFor<T>) -> DispatchResult {
        Ok(())
    }
}
