use frame_support::assert_ok;

#[test]
fn asserts_on_observable_behaviour() {
    new_test_ext().execute_with(|| {
        assert_ok!(QueuePallet::enqueue(RuntimeOrigin::signed(ALICE), 42));

        assert_eq!(QueueItems::<Test>::get(0), Some(42));
        System::assert_last_event(Event::ItemQueued { who: ALICE, item: 42 }.into());
    });
}
