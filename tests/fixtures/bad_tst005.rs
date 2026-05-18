use frame_support::assert_ok;

#[test]
fn reaches_into_internal_fields() {
    new_test_ext().execute_with(|| {
        assert_ok!(QueuePallet::enqueue(RuntimeOrigin::signed(ALICE), 42));

        let tracker = QueuePallet::debug_tracker();
        assert_eq!(tracker.inner, vec![42]);
        assert_eq!(tracker.counter, 1);
    });
}
