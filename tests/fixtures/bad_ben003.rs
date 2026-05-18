// BAD: Extrinsic defined with call_index but no matching benchmark.

#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn do_something(origin: OriginFor<T>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        Data::<T>::put(42);
        Self::deposit_event(Event::SomethingDone { who });
        Ok(())
    }

    #[pallet::call_index(1)]
    pub fn do_another_thing(origin: OriginFor<T>, value: u32) -> DispatchResult {
        ensure_root(origin)?;
        Config::<T>::put(value);
        Ok(())
    }
}
