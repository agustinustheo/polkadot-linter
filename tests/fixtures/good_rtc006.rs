// RTC006 negative fixture: every delivery price is free and nothing is
// exported, so `FeeManager = ()` never receives a fee to burn (the free
// prices are RTC003's finding). Mirrors the Acala, Astar and KILT runtimes.

pub type XcmRouter = WithUniqueTopic<(
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, ()>,
	XcmpQueue,
)>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type XcmSender = XcmRouter;
	type FeeManager = ();
	type MessageExporter = ();
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
}

construct_runtime!(
	pub enum Runtime {
		PolkadotXcm: pallet_xcm = 31,
	}
);
