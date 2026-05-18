#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposits_event_for_manual_note() {
        new_test_ext().execute_with(|| {
            use frame_support::assert_ok;
            use pallet_remark::Event as RemarkEvent;

            assert_ok!(Remark::store(RuntimeOrigin::signed(ALICE), b"ready".to_vec()));
            System::assert_last_event(RemarkEvent::Stored { who: ALICE }.into());
        });
    }
}
