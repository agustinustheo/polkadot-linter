// RTC003 positive fixture: XCM delivery fees burned or waived.
// Mirrors SRL OpenZeppelin 2024-04 §3.2.1-3.2.2, OpenZeppelin 2024-07 §3.2.1,
// InvArch 2024 §5.1-5.2, Hyperbridge 2024 §6.1-6.2 and Peaq 2402 §5.2.3/§5.2.6.

pub type XcmRouter = (
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, ()>,
	XcmpQueue,
);

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type XcmSender = XcmRouter;
	type FeeManager = ();
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
}

pub type PriceForParentDelivery = ();

construct_runtime!(
	pub enum Runtime {
		XcmpQueue: cumulus_pallet_xcmp_queue = 30,
	}
);
