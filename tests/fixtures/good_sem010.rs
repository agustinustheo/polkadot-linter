// GOOD: Uses .pow() for exponentiation.

const UNIT: u128 = 10_u128.pow(18);
const CENTS: u128 = UNIT / 100;

fn calculate_fee() -> u128 {
    let base = 10_u128.pow(16);
    base * 5
}
