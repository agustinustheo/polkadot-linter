// BAD: Storage read inside #[pallet::weight(...)].
// Weight is computed before dispatch — expensive ops here create DoS vectors.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::weight({
        let count = PendingItems::<T>::get().len() as u64;
        T::WeightInfo::process(count)
    })]
    #[pallet::call_index(0)]
    pub fn process_pending(origin: OriginFor<T>) -> DispatchResult {
        Ok(())
    }
}
