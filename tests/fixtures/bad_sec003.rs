// BAD: Decode::decode without depth limit on user-supplied data.
// Attacker crafts batch(batch(batch(...))) to exhaust the stack.

pub fn execute_call(mut data: &[u8]) -> DispatchResult {
    let call = <T as Config>::RuntimeCall::decode(&mut data)
        .map_err(|_| Error::<T>::InvalidCall)?;
    call.dispatch(origin)
}
