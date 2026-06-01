use raydium_clmm::{accounts::PoolState, types::TickState};

pub fn estimate_output(net_in: f64, ticks: &[TickState], pool: &PoolState) -> f64 {
    let mut full_swap = false;

    let a_to_b = pool.token_mint0.to_bytes() == spl_token::native_mint::ID.to_bytes();

    let mut cur = pool.sqrt_price_x64 as f64 / (1u128 << 64) as f64;
    let mut liquidity = pool.liquidity;

    let mut out = 0.0;
    let mut rem = net_in;

    for tick in ticks {
        let tick_sqrt = 1.0001f64.powf(tick.tick as f64 / 2.0);

        if a_to_b {
            let needed = liquidity as f64 * (1.0 / tick_sqrt - 1.0 / cur);

            if rem >= needed {
                out += liquidity as f64 * (cur - tick_sqrt);
                rem -= needed;
                cur = tick_sqrt;
                liquidity = (liquidity as i128 - tick.liquidity_net) as u128;
            } else {
                let next = 1.0 / (1.0 / cur + rem / liquidity as f64);
                out += liquidity as f64 * (cur - next);
                full_swap = true;
                break;
            }
        } else {
            let needed = liquidity as f64 * (tick_sqrt - cur);

            if rem >= needed {
                out += liquidity as f64 * (1.0 / cur - 1.0 / tick_sqrt);
                rem -= needed;
                cur = tick_sqrt;
                liquidity = (liquidity as i128 + tick.liquidity_net) as u128;
            } else {
                let next = cur + rem / liquidity as f64;
                out += liquidity as f64 * (1.0 / cur - 1.0 / next);
                full_swap = true;
                break;
            }
        };
    }

    if !full_swap {
        tracing::warn!("Failed to get the quote for the full swap, remaining at least: {rem:?}");
    }

    out
}

#[cfg(test)]
mod tests {
    use raydium_clmm::{
        accounts::PoolState,
        types::{RewardInfo, TickState},
    };

    use crate::test::PoolSpec;

    fn mk_pool(spec: &PoolSpec) -> PoolState {
        let wsol = solana_pubkey::Pubkey::new_from_array(spl_token::native_mint::ID.to_bytes());

        let (token_mint0, token_mint1) = if spec.sol_is_mint0 {
            (wsol, solana_pubkey::Pubkey::default())
        } else {
            (solana_pubkey::Pubkey::default(), wsol)
        };

        // only these matter: sqrt_price_x64, liquidity, tick_current, tick_spacing, token_mint0
        // everything else: zeros / Pubkey::default()
        PoolState {
            sqrt_price_x64: spec.sqrt_price_x64,
            liquidity: spec.liquidity,
            tick_current: spec.tick_current,
            tick_spacing: spec.tick_spacing,
            token_mint0,
            token_mint1,
            discriminator: Default::default(),
            bump: Default::default(),
            amm_config: Default::default(),
            owner: Default::default(),
            token_vault0: Default::default(),
            token_vault1: Default::default(),
            observation_key: Default::default(),
            mint_decimals0: Default::default(),
            mint_decimals1: Default::default(),
            padding3: Default::default(),
            padding4: Default::default(),
            fee_growth_global0_x64: Default::default(),
            fee_growth_global1_x64: Default::default(),
            protocol_fees_token0: Default::default(),
            protocol_fees_token1: Default::default(),
            swap_in_amount_token0: Default::default(),
            swap_out_amount_token1: Default::default(),
            swap_in_amount_token1: Default::default(),
            swap_out_amount_token0: Default::default(),
            status: Default::default(),
            padding: Default::default(),
            reward_infos: [
                RewardInfo {
                    reward_state: Default::default(),
                    open_time: Default::default(),
                    end_time: Default::default(),
                    last_update_time: Default::default(),
                    emissions_per_second_x64: Default::default(),
                    reward_total_emissioned: Default::default(),
                    reward_claimed: Default::default(),
                    token_mint: Default::default(),
                    token_vault: Default::default(),
                    authority: Default::default(),
                    reward_growth_global_x64: Default::default(),
                },
                RewardInfo {
                    reward_state: Default::default(),
                    open_time: Default::default(),
                    end_time: Default::default(),
                    last_update_time: Default::default(),
                    emissions_per_second_x64: Default::default(),
                    reward_total_emissioned: Default::default(),
                    reward_claimed: Default::default(),
                    token_mint: Default::default(),
                    token_vault: Default::default(),
                    authority: Default::default(),
                    reward_growth_global_x64: Default::default(),
                },
                RewardInfo {
                    reward_state: Default::default(),
                    open_time: Default::default(),
                    end_time: Default::default(),
                    last_update_time: Default::default(),
                    emissions_per_second_x64: Default::default(),
                    reward_total_emissioned: Default::default(),
                    reward_claimed: Default::default(),
                    token_mint: Default::default(),
                    token_vault: Default::default(),
                    authority: Default::default(),
                    reward_growth_global_x64: Default::default(),
                },
            ],
            tick_array_bitmap: Default::default(),
            total_fees_token0: Default::default(),
            total_fees_claimed_token0: Default::default(),
            total_fees_token1: Default::default(),
            total_fees_claimed_token1: Default::default(),
            fund_fees_token0: Default::default(),
            fund_fees_token1: Default::default(),
            open_time: Default::default(),
            recent_epoch: Default::default(),
            padding1: Default::default(),
            padding2: Default::default(),
        }
    }

    fn mk_ticks_rs(spec: &PoolSpec) -> Vec<TickState> {
        let mut res = vec![];

        for tick in &spec.ticks {
            if spec.sol_is_mint0 && tick.0 <= spec.tick_current
                || !spec.sol_is_mint0 && tick.0 > spec.tick_current
            {
                res.push(TickState {
                    tick: tick.0,
                    liquidity_net: tick.1,
                    liquidity_gross: tick.2,
                    fee_growth_outside0_x64: Default::default(),
                    fee_growth_outside1_x64: Default::default(),
                    reward_growths_outside_x64: Default::default(),
                    padding: Default::default(),
                });
            }
        }
        if spec.sol_is_mint0 {
            res.sort_by_key(|b| std::cmp::Reverse(b.tick));
        } else {
            res.sort_by_key(|b| b.tick);
        }
        res
    }
}
