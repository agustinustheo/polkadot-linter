// BAD: Non-saturating arithmetic inside #[pallet::weight(...)].
// Overflow produces tiny weight -> overweight block -> chain stalls.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::weight(
        T::WeightInfo::base().add(T::WeightInfo::per_item().mul(items.len() as u64))
    )]
    #[pallet::call_index(0)]
    pub fn process(origin: OriginFor<T>, items: BoundedVec<u8, MaxItems>) -> DispatchResult {
        Ok(())
    }
}
