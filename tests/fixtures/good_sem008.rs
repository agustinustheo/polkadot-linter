// GOOD: Uses alloc instead of deprecated sp_std.

extern crate alloc;
use alloc::vec::Vec;

fn process_items(items: Vec<u32>) -> u32 {
    items.iter().sum()
}
