// GOOD: Errors are propagated with `?` or explicitly handled.

pub fn process_member(who: &T::AccountId) -> DispatchResult {
    T::Currency::reserve(who, deposit)?;
    T::Currency::transfer(who, &pot, amount, Preservation::Expendable)?;
    Members::<T>::try_mutate(who, |m| {
        m.status = Status::Active;
        Ok(())
    })?;
    Ok(())
}
