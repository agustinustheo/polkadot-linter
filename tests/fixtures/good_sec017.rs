// GOOD: Event payload is bounded.

#[pallet::event]
pub enum Event<T: Config> {
    PayloadStored { bytes: BoundedVec<u8, MaxBytes> },
}
