// RTC005 positive fixture: collective-flip randomness wired into the runtime.
// Mirrors SRL Peaq 2402 §5.2.13 and t3rn 2024 §6.6; `pallet_randomness_collective_flip`
// is the crate's name before it was renamed to `pallet_insecure_randomness_collective_flip`.

use pallet_insecure_randomness_collective_flip as collective_flip;

impl pallet_insecure_randomness_collective_flip::Config for Runtime {}

impl pallet_contracts::Config for Runtime {
	type Randomness = RandomnessCollectiveFlip;
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system = 0,
		RandomnessCollectiveFlip: pallet_insecure_randomness_collective_flip = 2,
		LegacyFlip: pallet_randomness_collective_flip = 3,
		Contracts: pallet_contracts = 40,
	}
);
