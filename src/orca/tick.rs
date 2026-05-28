use anyhow::Context;
use orca_whirlpools_client::get_tick_array_address;
use orca_whirlpools_core::{TICK_ARRAY_SIZE, get_tick_array_start_tick_index};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
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

    pub async fn fetch_and_deserialize(
        &self,
        rpc_client: &RpcClient,
    ) -> anyhow::Result<[orca_whirlpools_client::TickArray; 3]> {
        let result = rpc_client
            .get_multiple_accounts(&self.inner)
            .await
            .context("Failed to get tick ararys")?
            .into_iter()
            .enumerate()
            .map(|(idx, el)| {
                let acct = el.ok_or(anyhow::anyhow!("Failed to get element: {idx:?}"))?;
                TickArray::from_bytes(&acct.data)
            })
            .collect::<anyhow::Result<Vec<orca_whirlpools_client::TickArray>>>()?;

        result
            .try_into()
            .map_err(|v: Vec<_>| anyhow::anyhow!("Expected 3 tick arrays, got {}", v.len()))
    }

    fn from_bytes(data: &[u8]) -> anyhow::Result<orca_whirlpools_client::TickArray> {
        orca_whirlpools_client::TickArray::from_bytes(data)
            .context("Failed to deserialize tick arr: {data:?}")
    }
}
