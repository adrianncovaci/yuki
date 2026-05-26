use anyhow::Context;
use orca_whirlpools_client::get_tick_array_address;
use orca_whirlpools_core::{TICK_ARRAY_SIZE, get_tick_array_start_tick_index};
use solana_sdk::pubkey::Pubkey;

pub struct TickArray {
    pub inner: [Pubkey; 3],
}

impl TickArray {
    pub fn new(
        whirlpool: Pubkey,
        tick_current_index: i32,
        whirlpool_tick_spacing: u16,
        a_to_b: bool,
    ) -> anyhow::Result<Self> {
        let ticks_in_array = whirlpool_tick_spacing as i32 * TICK_ARRAY_SIZE as i32;

        let tick_arr0_idx =
            get_tick_array_start_tick_index(tick_current_index, whirlpool_tick_spacing);

        let inner = if a_to_b {
            let tick_arr1_idx = tick_arr0_idx - ticks_in_array;

            let tick_arr2_idx = tick_arr1_idx - ticks_in_array;

            let tick_arr0 = get_tick_array_address(&whirlpool, tick_arr0_idx, None)
                .context("Failed to get tick array address")?;
            let tick_arr1 = get_tick_array_address(&whirlpool, tick_arr1_idx, None)
                .context("Failed to get tick array address")?;
            let tick_arr2 = get_tick_array_address(&whirlpool, tick_arr2_idx, None)
                .context("Failed to get tick array address")?;

            [tick_arr0.0, tick_arr1.0, tick_arr2.0]
        } else {
            let tick_arr1_idx = tick_arr0_idx + ticks_in_array;

            let tick_arr2_idx = tick_arr1_idx + ticks_in_array;

            let tick_arr0 = get_tick_array_address(&whirlpool, tick_arr0_idx, None)
                .context("Failed to get tick array address")?;
            let tick_arr1 = get_tick_array_address(&whirlpool, tick_arr1_idx, None)
                .context("Failed to get tick array address")?;
            let tick_arr2 = get_tick_array_address(&whirlpool, tick_arr2_idx, None)
                .context("Failed to get tick array address")?;

            [tick_arr0.0, tick_arr1.0, tick_arr2.0]
        };

        Ok(Self { inner })
    }
}
