// BAD: Extrinsic mutates storage but does not emit an event.
// External consumers (UIs, indexers) can't observe the state change.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn update_value(origin: OriginFor<T>, new_value: u32) -> DispatchResult {
        let who = ensure_signed(origin)?;
        CurrentValue::<T>::put(new_value);
        Ok(())
    }
}
