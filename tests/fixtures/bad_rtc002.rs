// RTC002 positive fixture: runtime-local weights for pallets that are absent from
// define_benchmarks!. Mirrors SRL Peaq 2402 §5.2.12, Hyperbridge 2024 §6.5 and
// t3rn 2024 §6.9.

impl pallet_balances::Config for Runtime {
	type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
}

impl pallet_sudo::Config for Runtime {
	type WeightInfo = weights::pallet_sudo::WeightInfo<Runtime>;
}

impl pallet_assets::Config<ForeignAssetsInstance> for Runtime {
	type WeightInfo = crate::weights::pallet_assets_foreign::WeightInfo<Runtime>;
}

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	pub struct Runtime;

	#[runtime::pallet_index(10)]
	pub type Balances = pallet_balances;
	#[runtime::pallet_index(15)]
	pub type Sudo = pallet_sudo;
	#[runtime::pallet_index(50)]
	pub type ForeignAssets = pallet_assets<Instance2>;
}

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	frame_benchmarking::define_benchmarks!(
		[frame_system, SystemBench::<Runtime>]
		[pallet_balances, Balances]
	);
}
