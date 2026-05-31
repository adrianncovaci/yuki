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
