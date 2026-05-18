use crate::{mock::*, Call, Event, Pallet};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn submit_proposal() {
        let caller = whitelisted_caller();
        let payload = vec![7u8; 32];

        #[extrinsic_call]
        submit_proposal(RawOrigin::Signed(caller), payload.clone());

        verify {
            assert_last_event::<Test>(Event::ProposalSubmitted { who: caller, payload }.into());
        }
    }
}
