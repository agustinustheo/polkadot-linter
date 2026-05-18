// BAD: authorize attribute exists, but there is no weight_of_authorize hook.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::authorize(|source, identifier| {
        Self::ensure_can_do_thing(source, identifier)
    })]
    #[pallet::call_index(7)]
    #[pallet::weight(T::WeightInfo::do_thing())]
    pub fn do_thing(origin: OriginFor<T>, identifier: u32) -> DispatchResult {
        ensure_authorized(origin)?;
        Ok(())
    }
}
