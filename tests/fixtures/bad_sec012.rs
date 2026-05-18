// BAD: clear_prefix uses an unbounded deletion limit.

pub fn wipe(owner: T::AccountId) {
    Accounts::<T>::clear_prefix(owner, None, None);
}
