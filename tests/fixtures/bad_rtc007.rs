// RTC007 positive fixture: instruction-inspecting denies not wrapped in
// `DenyRecursively` (polkadot-sdk #7148; polkadot-fellows/runtimes system
// parachains on main).

pub type Barrier = TrailingSetTopicAsId<
	DenyThenTry<
		DenyReserveTransferToRelayChain,
		(TakeWeightCredit, AllowTopLevelPaidExecutionFrom<Everything>),
	>,
>;

pub type BridgeBarrier = DenyThenTry<
	(DenyReserveTransferToRelayChain, DenyExportMessageFrom<Equals<Here>, Equals<Kusama>>),
	AllowExplicitUnpaidExecutionFrom<ParentOrParentsExecutivePlurality>,
>;

type Denies = (DenyRecursively<DenyReserveTransferToRelayChain>, DenyExportMessageFrom<Here, Here>);

pub type LocalBarrier = DenyThenTry<Denies, TakeWeightCredit>;

construct_runtime!(
	pub enum Runtime {
		PolkadotXcm: pallet_xcm = 31,
	}
);
