// RTC005 negative fixture: VRF-backed randomness, and references to the removed
// pallet that are only storage-prefix bytes or test code.

impl pallet_contracts::Config for Runtime {
	type Randomness = pallet_babe::RandomnessFromOneEpochAgo<Runtime>;
}

pub fn remove_old_pallet() {
	migration::remove_storage_prefix(b"RandomnessCollectiveFlip", b"RandomMaterial", b"");
}

#[cfg(test)]
mod tests {
	impl pallet_insecure_randomness_collective_flip::Config for Test {}
	pub type Randomness = RandomnessCollectiveFlip;
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system = 0,
		Babe: pallet_babe = 1,
		#[cfg(test)]
		RandomnessCollectiveFlip: pallet_insecure_randomness_collective_flip = 2,
	}
);
