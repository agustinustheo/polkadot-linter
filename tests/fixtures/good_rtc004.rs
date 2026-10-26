// RTC004 negative fixture: non-zero deposits, origin-gated zero deposits, and
// benchmark/test-only overrides.

pub const EXISTENTIAL_DEPOSIT: Balance = MILLICENTS;

parameter_types! {
	pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
	pub const ApprovalDeposit: Balance = EXISTENTIAL_DEPOSIT;
	pub const MaxDeposits: u32 = 0;
	pub const SubmissionDeposit: Balance = 0;
	#[cfg(feature = "runtime-benchmarks")]
	pub const MetadataDepositBase: Balance = 0;
}

impl pallet_referenda::Config<FellowshipReferendaInstance> for Runtime {
	type SubmitOrigin = pallet_ranked_collective::EnsureMember<Runtime, FellowshipCollectiveInstance, 1>;
	type SubmissionDeposit = SubmissionDeposit;
}

impl pallet_assets::Config<PoolAssetsInstance> for Runtime {
	type CreateOrigin =
		AsEnsureOriginWithArg<EnsureSignedBy<AssetConversionOrigin, sp_runtime::AccountId32>>;
	type AssetDeposit = ConstU128<0>;
	type MetadataDepositBase = ConstU128<0>;
	type ApprovalDeposit = ApprovalDeposit;
}

#[cfg(test)]
mod tests {
	parameter_types! {
		pub const ExistentialDeposit: Balance = 0;
	}
}

construct_runtime!(
	pub enum Runtime {
		Balances: pallet_balances = 10,
	}
);
