// RTC008 positive fixture: fail-open proxy filters (polkadot-sdk #12771,
// polkadot-fellows/runtimes #1341).

construct_runtime!(
	pub enum Runtime {
		System: frame_system = 0,
		Balances: pallet_balances = 10,
		CollatorSelection: pallet_collator_selection = 21,
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
				RuntimeCall::Balances { .. } | RuntimeCall::Assets { .. }
			),
			ProxyType::Collator => matches!(
				c,
				RuntimeCall::CollatorSelection { .. } | RuntimeCall::Utility { .. }
			),
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

impl InstanceFilter<RuntimeCall> for HubProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			HubProxyType::Any => true,
			HubProxyType::NonTransfer => matches!(
				c,
				RuntimeCall::System(..) | RuntimeCall::Utility(..) | RuntimeCall::Proxy(..)
			),
			HubProxyType::Collator => matches!(
				c,
				RuntimeCall::CollatorSelection(..) | RuntimeCall::Utility(..)
			),
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		match (self, o) {
			(x, y) if x == y => true,
			(HubProxyType::Any, _) => true,
			(_, HubProxyType::Any) => false,
			(HubProxyType::NonTransfer, _) => true,
			_ => false,
		}
	}
}
