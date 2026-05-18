// BAD: Return value of repatriate_reserved is discarded.
// It returns Ok(remaining) where remaining > 0 means not all funds transferred.

pub fn transfer_deposit(from: &T::AccountId, to: &T::AccountId, amount: Balance) {
    let _ = T::Currency::repatriate_reserved(from, to, amount, BalanceStatus::Free);
}
