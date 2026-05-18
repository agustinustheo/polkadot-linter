// GOOD: OCW submission logging uses a stable log target.

fn submit_cleanup_xt<T: Config>(xt: OpaqueExtrinsic) {
    if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_transaction(xt) {
        log::warn!(
            target: LOG_TARGET,
            "Failed to submit cleanup transaction: {e:?}",
        );
    }
}
