// GOOD: All validations before any storage writes.

pub fn update_config(origin: OriginFor<T>, new_value: u32) -> DispatchResult {
    let who = ensure_signed(origin)?;
    ensure!(new_value > 0, Error::<T>::InvalidValue);
    ConfigValue::<T>::put(new_value);
    Ok(())
}
