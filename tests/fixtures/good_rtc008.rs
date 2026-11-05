// RTC008 negative fixture: complete deny-lists, allow-lists, and supersets
// backed by the filters.

construct_runtime!(
	pub enum Runtime {
		System: frame_system = 0,
		Balances: pallet_balances = 10,
		Indices: pallet_indices = 11,
		Registrar: paras_registrar = 12,
		PolkadotXcm: pallet_xcm = 31,
		Utility: pallet_utility = 40,
		Assets: pallet_assets::<Instance1> = 50,
		ForeignAssets: pallet_assets::<Instance2> = 53,
	}
);

impl InstanceFilter<RuntimeCall> for ProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			ProxyType::Any => true,
			ProxyType::NonTransfer => !matches!(
				c,
				RuntimeCall::Balances { .. } |
					RuntimeCall::Assets(..) |
					RuntimeCall::ForeignAssets(..) |
					RuntimeCall::PolkadotXcm(..) |
					RuntimeCall::Indices(pallet_indices::Call::transfer { .. })
			),
			ProxyType::Governance => matches!(c, RuntimeCall::Utility(..) | RuntimeCall::System(..)),
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		match (self, o) {
			(x, y) if x == y => true,
			(ProxyType::Any, _) => true,
			(_, ProxyType::Any) => false,
			(ProxyType::NonTransfer, _) => true,
			_ => false,
		}
	}
}

impl InstanceFilter<RuntimeCall> for RelayProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			RelayProxyType::Any => true,
			RelayProxyType::NonTransfer => matches!(
				c,
				RuntimeCall::System(..) |
					RuntimeCall::Utility(..) |
					RuntimeCall::Proxy(..) |
					RuntimeCall::Registrar(paras_registrar::Call::register { .. })
			),
			RelayProxyType::Auction => {
				matches!(c, RuntimeCall::Registrar(..) | RuntimeCall::Utility(..))
			},
			RelayProxyType::Spokesperson => matches!(
				c,
				RuntimeCall::System(frame_system::Call::remark { .. }) |
					RuntimeCall::Utility(pallet_utility::Call::batch { .. })
			),
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		match (self, o) {
			(x, y) if x == y => true,
			(RelayProxyType::Any, _) => true,
			(_, RelayProxyType::Any) => false,
			// `Auction` admits `Registrar::swap`; `NonTransfer` omits it on purpose.
			(RelayProxyType::NonTransfer, RelayProxyType::Auction) => false,
			(RelayProxyType::NonTransfer, _) => true,
			_ => false,
		}
	}
}

// Wildcard patterns are not modelled: `!matches!(c, _)` denies everything and
// `matches!(c, _)` allows everything.
impl InstanceFilter<RuntimeCall> for WildProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			WildProxyType::All => matches!(c, _),
			WildProxyType::Nothing => !matches!(c, _),
			WildProxyType::Transfer => matches!(c, RuntimeCall::Balances(..)),
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		match (self, o) {
			(WildProxyType::All, _) => true,
			_ => false,
		}
	}
}

// A `(_, Y) => false` carve-out binds before `(X, _) => true`; a superset whose
// filter cannot add proxies cannot escalate.
impl InstanceFilter<RuntimeCall> for CarveProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			CarveProxyType::NonTransfer => matches!(c, RuntimeCall::System(..) | RuntimeCall::Proxy(..)),
			CarveProxyType::Transfer => matches!(c, RuntimeCall::Balances(..)),
			CarveProxyType::Remark => matches!(c, RuntimeCall::System(..)),
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		match (self, o) {
			(_, CarveProxyType::Transfer) => false,
			(CarveProxyType::NonTransfer, _) => true,
			(CarveProxyType::Remark, _) => true,
			_ => false,
		}
	}
}
