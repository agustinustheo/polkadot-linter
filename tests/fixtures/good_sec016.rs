// GOOD: Runtime upgrade checks StorageVersion before writing.

pub fn on_runtime_upgrade() -> Weight {
    let on_chain = StorageVersion::get::<Pallet<T>>();
    if on_chain == 0 {
        Migrated::<T>::put(true);
        CurrentStorageVersion::new(1).put::<Pallet<T>>();
        return T::DbWeight::get().writes(2);
    }
    Weight::zero()
}
