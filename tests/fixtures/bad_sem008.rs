// BAD: Uses sp_std which was deprecated in polkadot-sdk.

use sp_std::vec::Vec;
use sp_std::prelude::*;

fn process_items(items: sp_std::vec::Vec<u32>) -> u32 {
    items.iter().sum()
}
