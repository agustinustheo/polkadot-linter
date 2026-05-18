// GOOD: Extrinsic emits an event after mutating storage.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn update_value(origin: OriginFor<T>, new_value: u32) -> DispatchResult {
        let who = ensure_signed(origin)?;
        CurrentValue::<T>::put(new_value);
        Self::deposit_event(Event::ValueUpdated { who, new_value });
        Ok(())
    }
}
