// RTC011: Asset Hub Westend before polkadot-sdk#13137.
construct_runtime!(
	pub enum Runtime {
		System: frame_system = 0,
		Psm: pallet_psm = 65,
	}
);
parameter_types! {
	pub PsmName: &'static str = "Psm";
}
pub type Migrations = (frame_support::migrations::RemovePallet<PsmName, RocksDbWeight>,);
