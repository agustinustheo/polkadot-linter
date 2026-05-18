// GOOD: Uses benchmarked weight functions that capture both ref-time
// and proof size dimensions.

fn on_runtime_upgrade() -> Weight {
    T::WeightInfo::migrate_storage()
}

fn migrate_storage() -> Weight {
    T::WeightInfo::on_runtime_upgrade(count)
}
