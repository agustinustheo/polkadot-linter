// RTC002 negative fixture: every runtime-local weights binding is benchmarked,
// directly, through a joined module path, or through the pallet crate name.

impl pallet_balances::Config for Runtime {
	type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
}

impl parachains_slashing::Config for Runtime {
	type WeightInfo = weights::polkadot_runtime_parachains_disputes_slashing::WeightInfo<Runtime>;
}

impl pallet_assets::Config<PoolAssetsInstance> for Runtime {
	type WeightInfo = weights::pallet_assets_pool::WeightInfo<Runtime>;
}

// Upstream weights are RTC001's concern, not RTC002's.
impl pallet_sudo::Config for Runtime {
	type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

construct_runtime!(
	pub enum Runtime {
		Balances: pallet_balances = 10,
	}
);

mod benches {
	polkadot_sdk::frame_benchmarking::define_benchmarks!(
		[pallet_balances, Balances]
		[polkadot_runtime_parachains::disputes::slashing, ParasSlashing]
		[pallet_assets, Pool]
	);
}
