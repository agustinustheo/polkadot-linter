// GOOD: Zero guard before division, or uses checked_div.

pub fn distribute_rewards(total: BalanceOf<T>) -> DispatchResult {
    let interval = T::RewardInterval::get();
    ensure!(interval > 0, Error::<T>::ZeroInterval);
    let per_interval = total / interval;

    let members = Members::<T>::get();
    let share = total.checked_div(members.len() as u128)
        .ok_or(Error::<T>::NoMembers)?;

    Ok(())
}
