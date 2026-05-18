// GOOD: Uses defensive!() or proper error returns instead of debug_assert!.

pub fn convert(amount: u128) -> Option<Fungibility> {
    if amount == 0 {
        defensive!("amount must be non-zero");
        return None;
    }
    Some(Fungibility::Fungible(amount))
}

pub fn process(items: &[Item]) -> Result<(), Error> {
    ensure!(items.len() <= MAX_ITEMS, Error::TooManyItems);
    for item in items {
        handle(item);
    }
    Ok(())
}
