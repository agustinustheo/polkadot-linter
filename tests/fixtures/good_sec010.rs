// GOOD: Hook wrapped in with_storage_layer for atomicity.

impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_poll(_n: BlockNumberFor<T>, _weight: &mut WeightMeter) {
        let _ = with_storage_layer(|| {
            let items = PendingItems::<T>::get();
            ProcessedCount::<T>::put(items.len() as u32);
            for item in items {
                ItemStatus::<T>::insert(item.id, Status::Done);
            }
            PendingItems::<T>::kill();
            Ok::<(), ()>(())
        });
    }
}
