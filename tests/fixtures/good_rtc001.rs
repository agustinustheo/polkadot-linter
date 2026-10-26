// RTC001 negative fixture: benchmarked runtime weights, allowlisted unit
// weights, non-runtime self types and test-only code.

impl pallet_xcm::Config for Runtime {
	type WeightInfo = weights::pallet_xcm::WeightInfo<Runtime>;
}

impl pallet_message_queue::Config for Runtime {
	type WeightInfo = crate::weights::pallet_message_queue::WeightInfo<Runtime>;
}

// Hand-written production equivocation weights: allowlisted.
impl pallet_babe::Config for Runtime {
	type WeightInfo = ();
}

// Upstream reference weights are only reported when flag_substrate_weight = true.
impl pallet_balances::Config for Runtime {
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
}

// Not a `Config for Runtime` implementation.
impl frame_election_provider_support::onchain::Config for OnChainConfig {
	type WeightInfo = ();
}

#[cfg(test)]
mod tests {
	impl pallet_sudo::Config for Runtime {
		type WeightInfo = ();
	}
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system = 0,
		Babe: pallet_babe = 1,
	}
);
