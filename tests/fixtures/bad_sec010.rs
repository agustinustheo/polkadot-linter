// BAD: Hook function with multiple storage writes but no transactional wrapper.
// If the second write fails, the first persists — inconsistent state.

impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_poll(_n: BlockNumberFor<T>, _weight: &mut WeightMeter) {
        let items = PendingItems::<T>::get();
        ProcessedCount::<T>::put(items.len() as u32);
        for item in items {
            ItemStatus::<T>::insert(item.id, Status::Done);
        }
        PendingItems::<T>::kill();
    }
}
