// GOOD: No dead code suppression — code is either used or removed.

fn active_migration_helper() -> Weight {
    T::WeightInfo::migrate()
}

struct ActiveConfig {
    interval: u32,
}
