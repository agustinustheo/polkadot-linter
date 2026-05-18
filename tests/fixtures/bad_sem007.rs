// BAD: Uses RuntimeDebug which strips debug info in wasm builds.
// polkadot-sdk deprecated this because space savings are negligible
// and it makes debugging much harder.

#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct MyType {
    pub value: u32,
}

#[derive(Clone, Encode, Decode, RuntimeDebugNoBound, TypeInfo)]
pub struct MyGenericType<T: Config> {
    pub balance: T::Balance,
}
