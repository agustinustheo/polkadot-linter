// RTC001 positive fixture: placeholder weights bound in a production runtime.
// Mirrors SRL OpenZeppelin runtime templates 2024-04 §3.2.3, InvArch 2024 §5.4
// and Hyperbridge 2024 §6.4.

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_xcm::TestWeightInfo;
}

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

impl pallet_collective::Config<CouncilCollective> for Runtime {
	type WeightInfo = ();
}

impl frame_system::Config for Runtime {
	type SystemWeightInfo = ();
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system = 0,
		MessageQueue: pallet_message_queue = 1,
		PolkadotXcm: pallet_xcm = 2,
		Council: pallet_collective::<Instance1> = 3,
	}
);
