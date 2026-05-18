use frame_support::{assert_noop, assert_ok};

#[test]
fn settlement_focuses_on_outcome() {
    new_test_ext().execute_with(|| {
        assert_ok!(SettlementPallet::set_price(RuntimeOrigin::root(), 125));
        assert_ok!(SettlementPallet::deposit_collateral(RuntimeOrigin::signed(ALICE), 500));
        assert_ok!(SettlementPallet::settle(RuntimeOrigin::signed(ALICE), 9));

        assert_eq!(SettledTrades::<Test>::get(9), Some(ALICE));
        System::assert_last_event(Event::TradeSettled { trade_id: 9, who: ALICE }.into());
        assert_noop!(
            SettlementPallet::settle(RuntimeOrigin::signed(ALICE), 9),
            Error::<Test>::AlreadySettled,
        );
    });
}
