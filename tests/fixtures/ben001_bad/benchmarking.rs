use crate::{Call, Event, Pallet};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn submit_proposal() {
        let caller = whitelisted_caller();
        let proposal = vec![1u8; 16];

        #[extrinsic_call]
        submit_proposal(RawOrigin::Signed(caller.clone()), proposal.clone());

        verify {
            assert_last_event::<Test>(Event::ProposalSubmitted { who: caller, proposal }.into());
        }
    }

    #[benchmark]
    fn close_proposal() {
        let caller = whitelisted_caller();
        let proposal_id = 3;

        Pallet::<Test>::seed_open_proposal(proposal_id, &caller);

        #[extrinsic_call]
        close_proposal(RawOrigin::Signed(caller.clone()), proposal_id);

        verify {
            assert_last_event::<Test>(Event::ProposalClosed { proposal_id, who: caller }.into());
        }
    }
}
