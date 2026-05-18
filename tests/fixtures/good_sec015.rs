// GOOD: Production code uses normal dispatch.

pub fn execute(call: <T as Config>::RuntimeCall, origin: T::RuntimeOrigin) -> DispatchResult {
    call.dispatch(origin)?;
    Ok(())
}
