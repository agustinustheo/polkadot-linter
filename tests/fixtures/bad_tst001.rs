use frame_support::{assert_ok, traits::tokens::Preservation};

#[test]
fn transfer_rejects_zero_value_manually() {
    new_test_ext().execute_with(|| {
        assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), ALICE, 100));

        let result = Balances::transfer_allow_death(
            RuntimeOrigin::signed(ALICE),
            BOB,
            0,
            Preservation::Expendable,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err, pallet_balances::Error::<Test>::ExistentialDeposit.into());
    });
}
