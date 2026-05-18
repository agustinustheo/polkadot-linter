// BAD: Production code bypasses dispatch filters.

pub fn execute(call: <T as Config>::RuntimeCall, origin: T::RuntimeOrigin) -> DispatchResult {
    call.dispatch_bypass_filter(origin)?;
    Ok(())
}
