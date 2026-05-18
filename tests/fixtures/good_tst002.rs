#[test]
fn apply_extrinsic_checks_both_result_layers() {
    new_test_ext().execute_with(|| {
        let call = RuntimeCall::Treasury(pallet_treasury::Call::spend_local {
            amount: 1_000,
            beneficiary: BOB,
        });
        let xt = UncheckedExtrinsic::new_unsigned(call);

        assert_eq!(Executive::apply_extrinsic(xt), Ok(Ok(())));
    });
}
