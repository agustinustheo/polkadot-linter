// BAD: Uses ^ (XOR) when exponentiation was intended.
// In Rust, ^ is bitwise XOR: 10 ^ 16 = 26, NOT 10000000000000000.
// This was an actual bug found by reviewer in PR #442.

const UNIT: u128 = 10 ^ 18;
const CENTS: u128 = UNIT / 100;

fn calculate_fee() -> u128 {
    let base = 10u128 ^ 16;
    base * 5
}
