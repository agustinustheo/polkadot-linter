// BAD: OCW submission logging omits target: LOG_TARGET.

fn submit_cleanup_xt<T: Config>(xt: OpaqueExtrinsic) {
    if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_transaction(xt) {
        log::warn!(
            "Failed to submit cleanup transaction: {e:?}",
        );
    }
}
