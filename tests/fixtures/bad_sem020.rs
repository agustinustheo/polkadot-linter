// SEM020: the pallet-assets ERC20 precompile before polkadot-sdk#11676.
impl<Runtime> Precompile for ERC20<Runtime> {
	fn call(address: &[u8; 20], input: &Self::Interface, env: &mut impl Ext<T = Self::T>) -> Result<Vec<u8>, Error> {
		match input {
			IERC20Calls::transfer(call) => Self::transfer(address, call, env),
		}
	}
}

impl<Runtime> ERC20<Runtime> {
	fn caller(env: &mut impl Ext<T = Runtime>) -> Result<H160, Error> {
		env.caller().account_id().map(AddressMapper::to_address)
	}
}
