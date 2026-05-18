// GOOD: clear_prefix is chunked with a bounded limit.

pub fn wipe(owner: T::AccountId) {
    Accounts::<T>::clear_prefix(owner, Some(100), None);
}
