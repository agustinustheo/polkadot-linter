// GOOD: Return value of repatriate_reserved is checked.

pub fn transfer_deposit(from: &T::AccountId, to: &T::AccountId, amount: Balance) -> DispatchResult {
    let remaining = T::Currency::repatriate_reserved(from, to, amount, BalanceStatus::Free)?;
    ensure!(remaining.is_zero(), Error::<T>::IncompleteTranfer);
    Ok(())
}
