// BAD: Raw arithmetic in a function returning Result.
// In release builds, 255u8 + 1 wraps to 0 silently.

pub fn calculate_share(total: u128, count: u32) -> Result<u128, Error> {
    let per_member = total * count as u128;
    let bonus = per_member + extra_reward;
    Ok(bonus)
}
