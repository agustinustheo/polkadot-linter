// RTC010 fixture, shaped like the fellows system parachains before
// polkadot-fellows/runtimes#1237: the unpaid barrier gets the storage-reading
// `AuthorizedAliasers` through the executor's alias.
construct_runtime!(
	pub enum Runtime {
		PolkadotXcm: pallet_xcm = 31,
	}
);

pub type TrustedAliasers = (AliasChildLocation, AuthorizedAliasers<Runtime>);
pub type Nested = (CheapTrustedAliasers, TrustedAliasers);
pub type CheapTrustedAliasers = (AliasChildLocation,);

pub type Barrier = TrailingSetTopicAsId<
	DenyThenTry<
		DenyReserveTransferToRelayChain,
		(
			TakeWeightCredit,
			WithComputedOrigin<
				(
					AllowTopLevelPaidExecutionFrom<Everything>,
					AllowExplicitUnpaidExecutionFrom<
						(ParentOrParentsPlurality, FellowshipEntities),
						TrustedAliasers,
					>,
					AllowExplicitUnpaidExecutionFrom<Equals<Parent>, Nested>,
					AllowExplicitUnpaidExecutionFrom<Equals<Parent>, pallet_xcm::AuthorizedAliasers<Runtime>>,
					// Fixed shapes (runtimes#1237, polkadot-sdk#12831): no finding.
					AllowExplicitUnpaidExecutionFrom<Equals<Parent>, CheapTrustedAliasers>,
					AllowExplicitUnpaidExecutionFrom<Equals<Parent>>,
					AllowExplicitUnpaidExecutionFrom<Equals<Parent>, Nothing>,
				),
				UniversalLocation,
				ConstU32<8>,
			>,
		),
	>,
>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	// The executor may read storage: it runs after fees are paid.
	type Aliasers = TrustedAliasers;
	type Barrier = Barrier;
}

#[cfg(test)]
mod tests {
	type TestBarrier = AllowExplicitUnpaidExecutionFrom<Everything, TrustedAliasers>;
}
