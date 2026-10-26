// RTC005 positive fixture: collective-flip randomness wired into the runtime.
// Mirrors SRL Peaq 2402 §5.2.13 and t3rn 2024 §6.6.

use pallet_insecure_randomness_collective_flip as collective_flip;

impl pallet_insecure_randomness_collective_flip::Config for Runtime {}

impl pallet_contracts::Config for Runtime {
	type Randomness = RandomnessCollectiveFlip;
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system = 0,
		RandomnessCollectiveFlip: pallet_insecure_randomness_collective_flip = 2,
		Contracts: pallet_contracts = 40,
	}
);
