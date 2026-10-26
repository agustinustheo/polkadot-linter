// RTC003 positive fixture: XCM fees waived or delivery priced at zero.
// Mirrors SRL OpenZeppelin 2024-04 §3.2.2, OpenZeppelin 2024-07 §3.2.1,
// InvArch 2024 §5.1-5.2, Hyperbridge 2024 §6.1-6.2 and Peaq 2402 §5.2.6.

pub type XcmRouter = (
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, ()>,
	XcmpQueue,
);

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type XcmSender = XcmRouter;
	type FeeManager = WaiveDeliveryFees;
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
}

pub type PriceForParentDelivery = ();

pub type WaivedLocations = (Equals<RootLocation>, Everything);

pub mod aliased {
	pub struct XcmConfig;
	impl xcm_executor::Config for XcmConfig {
		type XcmSender = XcmRouter;
		type FeeManager = XcmFeeManagerFromComponents<
			WaivedLocations,
			SendXcmFeeToAccount<Self::AssetTransactor, TreasuryAccount>,
		>;
	}
}

pub mod inline {
	pub struct XcmConfig;
	impl xcm_executor::Config for XcmConfig {
		type XcmSender = XcmRouter;
		type FeeManager = XcmFeeManagerFromComponents<Everything, ()>;
	}
}

construct_runtime!(
	pub enum Runtime {
		XcmpQueue: cumulus_pallet_xcmp_queue = 30,
	}
);
