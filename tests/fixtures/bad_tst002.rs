use frame_support::assert_ok;

#[test]
fn apply_extrinsic_hides_inner_dispatch_error() {
    new_test_ext().execute_with(|| {
        let call = RuntimeCall::Treasury(pallet_treasury::Call::spend_local {
            amount: 1_000,
            beneficiary: BOB,
        });
        let xt = UncheckedExtrinsic::new_unsigned(call);

        assert_ok!(Executive::apply_extrinsic(xt));
    });
}
