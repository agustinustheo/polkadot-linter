// BAD: Runtime upgrade writes storage without a StorageVersion gate.

pub fn on_runtime_upgrade() -> Weight {
    Migrated::<T>::put(true);
    Processed::<T>::insert(0, true);
    T::DbWeight::get().writes(2)
}
