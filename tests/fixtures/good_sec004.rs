// GOOD: Uses saturating arithmetic in weight attribute.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::weight(
        T::WeightInfo::base().saturating_add(T::WeightInfo::per_item().saturating_mul(items.len() as u64))
    )]
    #[pallet::call_index(0)]
    pub fn process(origin: OriginFor<T>, items: BoundedVec<u8, MaxItems>) -> DispatchResult {
        Ok(())
    }
}
