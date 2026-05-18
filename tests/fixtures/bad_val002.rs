// BAD: Divides by a config value and a .len() result without zero guards.
// If either is zero, the runtime panics.

pub fn distribute_rewards(total: BalanceOf<T>) -> DispatchResult {
    let interval = T::RewardInterval::get();
    let per_interval = total / interval;

    let members = Members::<T>::get();
    let share = total / members.len();

    Ok(())
}
