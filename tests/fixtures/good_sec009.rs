// GOOD: Uses checked/saturating arithmetic in fallible function.

pub fn calculate_share(total: u128, count: u32) -> Result<u128, Error> {
    let per_member = total.saturating_mul(count as u128);
    let bonus = per_member.checked_add(extra_reward).ok_or(Error::Overflow)?;
    Ok(bonus)
}
