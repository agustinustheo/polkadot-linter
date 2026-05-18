// BAD: Storage write before validation.
// If the ensure! on line 8 fails, the put() on line 6 already happened.

pub fn update_config(origin: OriginFor<T>, new_value: u32) -> DispatchResult {
    ConfigValue::<T>::put(new_value);
    let who = ensure_signed(origin)?;
    ensure!(new_value > 0, Error::<T>::InvalidValue);
    Ok(())
}
