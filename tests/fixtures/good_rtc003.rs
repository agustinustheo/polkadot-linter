// RTC003 negative fixture: fees routed through a fee manager and exponential
// delivery prices; a sender-less executor cannot incur delivery fees.

pub type PriceForSiblingParachainDelivery =
	ExponentialPrice<FeeAssetId, BaseDeliveryFee, TransactionByteFee, XcmpQueue>;
pub type PriceForParentDelivery =
	ExponentialPrice<FeeAssetId, BaseDeliveryFee, TransactionByteFee, ParachainSystem>;

pub type XcmRouter = (
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, PriceForParentDelivery>,
	XcmpQueue,
);

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type XcmSender = XcmRouter;
	type FeeManager = XcmFeeManagerFromComponents<
		WaivedLocations,
		SendXcmFeeToAccount<Self::AssetTransactor, TreasuryAccount>,
	>;
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type PriceForSiblingDelivery = PriceForSiblingParachainDelivery;
}

pub mod glutton {
	pub struct XcmConfig;
	impl xcm_executor::Config for XcmConfig {
		type XcmSender = (); // sending XCM not supported
		type MessageExporter = ();
		type FeeManager = ();
	}
}

construct_runtime!(
	pub enum Runtime {
		XcmpQueue: cumulus_pallet_xcmp_queue = 30,
	}
);
