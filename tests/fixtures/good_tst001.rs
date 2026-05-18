use frame_support::{assert_noop, assert_ok, traits::tokens::Preservation};

#[test]
fn transfer_rejects_zero_value_with_assert_noop() {
    new_test_ext().execute_with(|| {
        assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), ALICE, 100));

        assert_noop!(
            Balances::transfer_allow_death(
                RuntimeOrigin::signed(ALICE),
                BOB,
                0,
                Preservation::Expendable,
            ),
            pallet_balances::Error::<Test>::ExistentialDeposit,
        );
    });
}
