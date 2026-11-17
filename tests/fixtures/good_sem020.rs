// SEM020: the same precompile after polkadot-sdk#11676.
impl<Runtime> Precompile for ERC20<Runtime> {
	fn call(address: &[u8; 20], input: &Self::Interface, env: &mut impl Ext<T = Self::T>) -> Result<Vec<u8>, Error> {
		if env.is_delegate_call() {
			return Err(Error::Error(PrecompileDelegateDenied.into()));
		}
		let from = env.caller();
		Self::transfer(from, address, input, env)
	}
}
