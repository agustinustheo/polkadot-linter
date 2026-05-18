// GOOD: authorize attribute has a dedicated authorize-weight hook.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::authorize(|source, identifier| {
        Self::ensure_can_do_thing(source, identifier)
    })]
    #[pallet::call_index(7)]
    #[pallet::weight(T::WeightInfo::do_thing())]
    #[pallet::weight_of_authorize(T::WeightInfo::authorize_do_thing())]
    pub fn do_thing(origin: OriginFor<T>, identifier: u32) -> DispatchResult {
        ensure_authorized(origin)?;
        Ok(())
    }
}
