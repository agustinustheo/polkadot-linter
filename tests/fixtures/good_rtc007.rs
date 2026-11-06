// RTC007 negative fixture: recursive denies and origin-only denies.

pub type Barrier = TrailingSetTopicAsId<
	DenyThenTry<
		DenyRecursively<DenyReserveTransferToRelayChain>,
		(TakeWeightCredit, AllowTopLevelPaidExecutionFrom<Everything>),
	>,
>;

pub type BridgeBarrier = DenyThenTry<
	DenyRecursively<(DenyReserveTransferToRelayChain, DenyExportMessageFrom<Equals<Here>, Equals<Kusama>>)>,
	TakeWeightCredit,
>;

// Unknown local denies are not assumed to inspect instructions.
pub type RelayBarrier = DenyThenTry<DenyOnceMigrationStarts, TakeWeightCredit>;

construct_runtime!(
	pub enum Runtime {
		PolkadotXcm: pallet_xcm = 31,
	}
);
