// GOOD: Uses decode_with_depth_limit to prevent stack exhaustion.

pub fn execute_call(mut data: &[u8]) -> DispatchResult {
    let call = <T as Config>::RuntimeCall::decode_with_depth_limit(
        sp_io::MAX_EXTRINSIC_DEPTH,
        &mut data,
    ).map_err(|_| Error::<T>::InvalidCall)?;
    call.dispatch(origin)
}
