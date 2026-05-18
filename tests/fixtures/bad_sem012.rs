// BAD: Suppresses dead_code warning instead of removing the unused code.

#[allow(dead_code)]
fn old_migration_helper() -> Weight {
    Weight::zero()
}

#[allow(dead_code)]
struct DeprecatedConfig {
    interval: u32,
}
