// BAD: debug_assert! in production pallet code.
// Panics in debug builds, stripped in release — neither is correct.

pub fn convert(amount: u128) -> Fungibility {
    debug_assert_ne!(amount, 0);
    Fungibility::Fungible(amount)
}

pub fn process(items: &[Item]) {
    debug_assert!(items.len() <= MAX_ITEMS);
    for item in items {
        handle(item);
    }
}
