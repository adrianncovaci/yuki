#[cfg(test)]
pub(crate) mod tests {
    use orca_whirlpools_core::{TickArrayFacade, TickFacade, WhirlpoolFacade};

    use crate::test::PoolSpec;

    pub(crate) fn mk_pool(spec: &PoolSpec) -> WhirlpoolFacade {
        WhirlpoolFacade {
            sqrt_price: spec.sqrt_price_x64,
            liquidity: spec.liquidity,
            tick_current_index: spec.tick_current,
            tick_spacing: spec.tick_spacing,
            fee_rate: spec.fee_rate_pm as u16,
            fee_tier_index_seed: spec.tick_spacing.to_le_bytes(),
            ..Default::default()
        }
    }

    pub(crate) fn mk_ticks(spec: &PoolSpec) -> [TickArrayFacade; 3] {
        let spacing = spec.tick_spacing as i32;
        let ticks_per_arr = spacing * 88;
        let containing = spec.tick_current.div_euclid(spacing).div_euclid(88) * ticks_per_arr;

        let starts: [i32; 3] = if spec.sol_is_mint0 {
            [
                containing,
                containing - ticks_per_arr,
                containing - 2 * ticks_per_arr,
            ]
        } else {
            [
                containing,
                containing + ticks_per_arr,
                containing + 2 * ticks_per_arr,
            ]
        };

        let mut arrays = starts.map(|start| TickArrayFacade {
            start_tick_index: start,
            ticks: [TickFacade::default(); 88],
        });

        for &(tick, lnet, lgross) in &spec.ticks {
            let slot_global = tick.div_euclid(spacing);
            let arr_start = slot_global.div_euclid(88) * ticks_per_arr;
            let arr_idx = starts
                .iter()
                .position(|&s| s == arr_start)
                .expect("spec tick outside 3-array window");
            let slot = slot_global.rem_euclid(88) as usize;
            arrays[arr_idx].ticks[slot] = TickFacade {
                initialized: true,
                liquidity_net: lnet,
                liquidity_gross: lgross,
                ..Default::default()
            };
        }

        arrays
    }
}
