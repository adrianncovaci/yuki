use anyhow::Context;
use raydium_clmm::{
    accounts::{PoolState, TickArrayState},
    types::TickState,
};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

use crate::raydium::RAYDIUM_CLMM_PROGRAM;

const TICK_ARRAY_SIZE: u16 = 60;

pub struct TickArray {
    pub inner: [Pubkey; 3],
}

impl TickArray {
    pub fn new(pool: Pubkey, start_idx: i32, tick_spacing: u16, a_to_b: bool) -> Self {
        let ticks_in_array = (tick_spacing * TICK_ARRAY_SIZE) as i32;
        let inner = if a_to_b {
            let tick1 = Self::get_pda(
                pool,
                Self::get_tick_array_start_tick_index(start_idx, tick_spacing),
            );
            let tick2 = Self::get_pda(
                pool,
                Self::get_tick_array_start_tick_index(start_idx - ticks_in_array, tick_spacing),
            );
            let tick3 = Self::get_pda(
                pool,
                Self::get_tick_array_start_tick_index(start_idx - 2 * ticks_in_array, tick_spacing),
            );
            [tick1, tick2, tick3]
        } else {
            let tick1 = Self::get_pda(
                pool,
                Self::get_tick_array_start_tick_index(start_idx, tick_spacing),
            );
            let tick2 = Self::get_pda(
                pool,
                Self::get_tick_array_start_tick_index(start_idx + ticks_in_array, tick_spacing),
            );
            let tick3 = Self::get_pda(
                pool,
                Self::get_tick_array_start_tick_index(start_idx + 2 * ticks_in_array, tick_spacing),
            );
            [tick1, tick2, tick3]
        };
        Self { inner }
    }

    fn get_pda(pool: Pubkey, start_idx: i32) -> Pubkey {
        Pubkey::find_program_address(
            &[b"tick_array", pool.as_ref(), &start_idx.to_be_bytes()],
            &RAYDIUM_CLMM_PROGRAM,
        )
        .0
    }

    fn get_tick_array_start_tick_index(tick_index: i32, tick_spacing: u16) -> i32 {
        let tick_spacing = tick_spacing as i32;
        let real_index = tick_index
            .div_euclid(tick_spacing)
            .div_euclid(TICK_ARRAY_SIZE as i32);
        real_index * tick_spacing * TICK_ARRAY_SIZE as i32
    }

    pub async fn fetch_and_deserialize(
        &self,
        rpc_client: &RpcClient,
        pool: &PoolState,
        a_to_b: bool,
    ) -> anyhow::Result<Vec<TickState>> {
        let mut res = rpc_client
            .get_multiple_accounts(&self.inner)
            .await
            .context("Failed to get tick arrays")?
            .into_iter()
            .enumerate()
            .try_fold(Vec::new(), |mut acc, (idx, el)| {
                let acct = el.ok_or(anyhow::anyhow!("Failed to get element: {idx:?}"))?;

                let ticks = TickArrayState::from_bytes(&acct.data)
                    .with_context(|| format!("Failed to deserialize tick array state: {idx}"))?
                    .ticks
                    .into_iter()
                    .filter(|el| el.liquidity_gross != 0)
                    .filter(|t| {
                        if a_to_b {
                            t.tick <= pool.tick_current
                        } else {
                            t.tick > pool.tick_current
                        }
                    })
                    .collect::<Vec<_>>();
                acc.extend(ticks);
                Ok(acc)
            });

        let _ = res.as_mut().map(|ticks| {
            ticks.sort_by(|a, b| {
                if a_to_b {
                    b.tick.cmp(&a.tick)
                } else {
                    a.tick.cmp(&b.tick)
                }
            })
        });

        res
    }
}
