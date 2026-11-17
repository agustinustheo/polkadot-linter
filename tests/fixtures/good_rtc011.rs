// RTC011: removed (or commented-out) pallets and unresolved names are fine.
construct_runtime!(
	pub enum Runtime {
		System: frame_system = 0,
		// DELETED: Psm: pallet_psm = 65,
	}
);
pub const PsmName: &'static str = "Psm";
pub type Migrations = (RemovePallet<PsmName, RocksDbWeight>, RemovePallet<Unknown, RocksDbWeight>);
