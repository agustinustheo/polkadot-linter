use frame_support::assert_ok;

#[test]
fn settlement_test_is_mostly_mock_scaffolding() {
    let mut price_mock = MockPriceOracle::new();
    price_mock.expect_spot_price().times(1).returning(|| 125);
    price_mock.expect_twap_price().times(1).returning(|| 123);
    price_mock.expect_confidence().times(1).returning(|| 99);

    let mut balance_mock = MockBalances::new();
    balance_mock.expect_free_balance().times(1).returning(|_| 1_000);
    balance_mock.expect_reserved_balance().times(1).returning(|_| 250);
    balance_mock.expect_can_withdraw().times(1).returning(|_, _| true);

    let mut fee_mock = MockFeeTrader::new();
    fee_mock.expect_base_fee().times(1).returning(|| 2);
    fee_mock.expect_multiplier().times(1).returning(|| 1);
    fee_mock.expect_withdraw_fee().times(1).returning(|_, _| Ok(()));

    let mut hooks_mock = MockHooks::new();
    hooks_mock.expect_before_settlement().times(1).returning(|| ());
    hooks_mock.expect_after_settlement().times(1).returning(|| ());

    new_test_ext().execute_with(|| {
        assert_ok!(SettlementPallet::settle(RuntimeOrigin::signed(ALICE), 9));
    });
}
