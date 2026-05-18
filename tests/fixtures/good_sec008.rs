// GOOD: Uses defensive!(), error propagation, or unwrap_or_default().

pub fn get_member_name(who: &T::AccountId) -> Result<Vec<u8>, Error<T>> {
    let member = Members::<T>::get(who).ok_or(Error::<T>::MemberNotFound)?;
    let name = member.name.unwrap_or_default();
    Ok(name)
}

pub fn do_critical_thing() -> DispatchResult {
    if something_wrong() {
        defensive!("unexpected state — this should never happen");
        return Err(Error::<T>::InconsistentState.into());
    }
    Ok(())
}
