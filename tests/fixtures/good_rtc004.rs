// RTC004 negative fixture: non-zero deposits, origin-gated zero deposits, and
// benchmark/test-only overrides.

pub const EXISTENTIAL_DEPOSIT: Balance = MILLICENTS;

parameter_types! {
	pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
	pub const ApprovalDeposit: Balance = EXISTENTIAL_DEPOSIT;
	pub const MaxDeposits: u32 = 0;
	pub DepositBase: Balance = 10 * UNIT;
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

pub type FellowshipAdmin = EitherOf<EnsureRoot<AccountId>, EnsureMember<Runtime, Fellowship, 3>>;

impl pallet_referenda::Config<AmbassadorReferendaInstance> for Runtime {
	type SubmitOrigin = MapSuccess<FellowshipAdmin, Replace<ConstU16<0>>>;
	type SubmissionDeposit = ConstU128<0>;
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
