// BAD: Event payload contains Vec<T>.

#[pallet::event]
pub enum Event<T: Config> {
    PayloadStored { bytes: Vec<u8> },
}
