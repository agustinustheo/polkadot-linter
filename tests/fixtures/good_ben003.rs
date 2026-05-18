// GOOD: All extrinsics are just data — this file does not have #[pallet::call]
// so BEN003 should not fire.

pub fn helper_function() -> u32 {
    42
}

pub fn another_helper(x: u32) -> u32 {
    x * 2
}
