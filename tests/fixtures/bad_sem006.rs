// BAD: Uses DbWeight::get().reads() which only accounts for ref-time,
// not proof size (PoV). Parachains need both weight dimensions.
// This pattern was flagged ~15 times by reviewer across 8+ PRs.

fn on_runtime_upgrade() -> Weight {
    // This only estimates ref-time, completely ignoring proof size.
    // A parachain block has two limits: proof size AND time to verify.
    T::DbWeight::get().reads_writes(5, 3)
}

fn migrate_storage() -> Weight {
    let count = OldStorage::<T>::iter().count() as u64;
    T::DbWeight::get().reads(count)
}
