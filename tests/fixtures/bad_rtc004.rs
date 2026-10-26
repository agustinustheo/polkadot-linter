// RTC004 positive fixture: zero existential and storage deposits.
// Mirrors SRL OpenZeppelin 2024-07 §3.2.3 and NeuroWeb 2025 §4.4.

pub const EXISTENTIAL_DEPOSIT: Balance = 0;

parameter_types! {
	pub const ExistentialDeposit: Balance = 0;
	pub const ApprovalDeposit: Balance = 0;
	pub const MetadataDepositPerByte: Balance = 0_u128;
}

impl pallet_assets::Config<PoolAssetsInstance> for Runtime {
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type AssetDeposit = AssetDeposit;
	type ApprovalDeposit = ConstU128<0>;
	type AssetAccountDeposit = ConstU128<0>;
}

impl pallet_balances::Config for Runtime {
	type ExistentialDeposit = ConstU128<0>;
}

parameter_types! {
	pub const AssetDeposit: Balance = 0;
}

parameter_types! {
	pub DepositBase: Balance = 0;
	pub storage DepositPerItem: Balance = 0 * UNIT;
}

pub const DEPOSIT_FACTOR: Balance = 0 * CENTS;

pub type PublicSubmissionOrigin = EnsureSigned<AccountId>;

impl pallet_referenda::Config for Runtime {
	type SubmitOrigin = EitherOfDiverse<EnsureRoot<AccountId>, PublicSubmissionOrigin>;
	type SubmissionDeposit = ConstU128<0>;
}

construct_runtime!(
	pub enum Runtime {
		Balances: pallet_balances = 10,
	}
);
