// RTC006 positive fixture: `FeeManager = ()` charges XCM fees and burns them.
// Mirrors the setting reported in SRL OpenZeppelin 2024-04 §3.2.1 and Peaq 2402 §5.2.3.

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type XcmSender = XcmRouter;
	type MessageExporter = ();
	type FeeManager = ();
}

construct_runtime!(
	pub enum Runtime {
		PolkadotXcm: pallet_xcm = 31,
	}
);
