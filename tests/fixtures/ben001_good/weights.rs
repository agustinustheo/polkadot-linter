use frame_support::weights::Weight;

pub trait WeightInfo {
    fn submit_proposal() -> Weight;
    fn close_proposal() -> Weight;
    fn sweep_expired(proposals: u32) -> Weight;
}
