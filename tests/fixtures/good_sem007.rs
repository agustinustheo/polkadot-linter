// GOOD: Uses Debug/DebugNoBound which preserves debug info in wasm.

#[derive(Clone, Encode, Decode, Debug, TypeInfo)]
pub struct MyType {
    pub value: u32,
}

#[derive(Clone, Encode, Decode, DebugNoBound, TypeInfo)]
pub struct MyGenericType<T: Config> {
    pub balance: T::Balance,
}
