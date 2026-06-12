use meteora_sol::accounts::{BinArray, LbPair};
use meteora_sol::types::StaticParameters;
use solana_sdk::pubkey::Pubkey;

use super::bin::MAX_BIN_PER_ARRAY;

const FEE_PRECISION: u128 = 1_000_000_000;
const MAX_FEE_RATE: u128 = 100_000_000;
const BASIS_POINT_MAX: u128 = 10_000;

pub fn total_fee_rate(
    params: &StaticParameters,
    bin_step: u16,
    volatility_accumulator: u32,
) -> u128 {
    let base = (params.base_factor as u128)
        * (bin_step as u128)
        * 10
        * 10u128.pow(params.base_fee_power_factor as u32);

    let variable = if params.variable_fee_control > 0 {
        let square = ((volatility_accumulator as u128) * (bin_step as u128)).pow(2);
        let v_fee = (params.variable_fee_control as u128) * square;

        v_fee.div_ceil(100_000_000_000)
    } else {
        0
    };

    (base + variable).min(MAX_FEE_RATE)
}

fn compute_fee(amount: u64, rate: u128) -> u64 {
    ((amount as u128) * rate).div_ceil(FEE_PRECISION - rate) as u64
}

fn compute_fee_from_amount(amount: u64, rate: u128) -> u64 {
    ((amount as u128) * rate).div_ceil(FEE_PRECISION) as u64
}

pub struct Volatility {
    pub accumulator: u32,
    reference: u32,
    index_reference: i32,
    max: u32,
}

impl Volatility {
    pub fn from_pair(pair: &LbPair, now: i64) -> Self {
        let p = &pair.parameters;
        let v = &pair.v_parameters;
        let elapsed = now - v.last_update_timestamp;

        let (reference, index_reference) = if elapsed >= p.filter_period as i64 {
            // High-frequency window has passed: decay the reference (or reset).
            let reference = if elapsed < p.decay_period as i64 {
                ((v.volatility_accumulator as u128 * p.reduction_factor as u128) / BASIS_POINT_MAX)
                    as u32
            } else {
                0
            };
            (reference, pair.active_id)
        } else {
            // Still within the filter period: keep the stored references.
            (v.volatility_reference, v.index_reference)
        };

        let mut vol = Self {
            accumulator: v.volatility_accumulator,
            reference,
            index_reference,
            max: p.max_volatility_accumulator,
        };
        vol.accumulate(pair.active_id);
        vol
    }

    pub fn accumulate(&mut self, bin_id: i32) {
        let delta = (self.index_reference - bin_id).unsigned_abs() as u64;
        let acc = self.reference as u64 + delta * BASIS_POINT_MAX as u64;
        self.accumulator = acc.min(self.max as u64) as u32;
    }
}

struct BinSwap {
    amount_in: u64,
    amount_out: u64,
}

fn swap_at_bin(
    amount_left: u64,
    price: f64,
    rate: u128,
    reserve_out: u64,
    swap_for_y: bool,
) -> BinSwap {
    if reserve_out == 0 {
        return BinSwap {
            amount_in: 0,
            amount_out: 0,
        };
    }

    // Net input required to drain this bin's output reserve.
    let max_in_net = if swap_for_y {
        (reserve_out as f64 / price).ceil() as u64
    } else {
        (reserve_out as f64 * price).ceil() as u64
    };
    let max_in_gross = max_in_net.saturating_add(compute_fee(max_in_net, rate));

    if max_in_gross > 0 && amount_left >= max_in_gross {
        // Whole bin consumed.
        BinSwap {
            amount_in: max_in_gross,
            amount_out: reserve_out,
        }
    } else {
        // Partial fill: the remaining input stops inside this bin.
        let fee = compute_fee_from_amount(amount_left, rate);
        let net = amount_left.saturating_sub(fee) as f64;
        let out = if swap_for_y { net * price } else { net / price };
        let out = (out.floor() as u64).min(reserve_out);
        BinSwap {
            amount_in: amount_left,
            amount_out: out,
        }
    }
}

pub fn estimate_output(
    amount_in: u64,
    pair: &LbPair,
    bin_arrays: &[(Pubkey, BinArray)],
    now: i64,
) -> anyhow::Result<f64> {
    let swap_for_y = pair.token_x_mint.to_bytes() == spl_token::native_mint::id().to_bytes();
    let bin_step = pair.bin_step;
    let params = &pair.parameters;

    let mut vol = Volatility::from_pair(pair, now);
    let mut amount_left = amount_in;
    let mut total_out: u64 = 0;
    let mut bin_id = pair.active_id;

    while amount_left > 0 {
        let array_index = bin_id.div_euclid(MAX_BIN_PER_ARRAY) as i64;
        let Some((_, array)) = bin_arrays.iter().find(|(_, a)| a.index == array_index) else {
            break;
        };
        let offset = bin_id.rem_euclid(MAX_BIN_PER_ARRAY) as usize;
        let bin = &array.bins[offset];

        vol.accumulate(bin_id);
        let rate = total_fee_rate(params, bin_step, vol.accumulator);
        let price = (1.0 + bin_step as f64 / 10_000.0).powi(bin_id);
        let reserve_out = if swap_for_y {
            bin.amount_y
        } else {
            bin.amount_x
        };

        let res = swap_at_bin(amount_left, price, rate, reserve_out, swap_for_y);
        total_out += res.amount_out;

        if res.amount_in >= amount_left {
            break;
        }
        amount_left -= res.amount_in;
        bin_id = if swap_for_y { bin_id - 1 } else { bin_id + 1 };
    }

    Ok(total_out as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_roundtrip_and_amounts() {
        let rate = 1_000_000; // 0.1%

        assert_eq!(compute_fee_from_amount(1_000_000_000, rate), 1_000_000);

        let net = 999_000_000u64;
        let gross = net + compute_fee(net, rate);
        assert!((1_000_000_000..=1_000_001_000).contains(&gross));
    }

    #[test]
    fn base_fee_formula() {
        let params = StaticParameters {
            base_factor: 10_000,
            base_fee_power_factor: 0,
            variable_fee_control: 0,
            filter_period: 0,
            decay_period: 0,
            reduction_factor: 0,
            max_volatility_accumulator: 0,
            min_bin_id: 0,
            max_bin_id: 0,
            protocol_share: 0,
            padding: [0; 5],
        };

        assert_eq!(total_fee_rate(&params, 20, 0), 2_000_000);
    }

    #[test]
    fn variable_fee_caps_at_max() {
        let params = StaticParameters {
            base_factor: 10_000,
            base_fee_power_factor: 0,
            variable_fee_control: 40_000,
            filter_period: 0,
            decay_period: 0,
            reduction_factor: 0,
            max_volatility_accumulator: 1_000_000,
            min_bin_id: 0,
            max_bin_id: 0,
            protocol_share: 0,
            padding: [0; 5],
        };
        assert_eq!(total_fee_rate(&params, 20, 1_000_000), MAX_FEE_RATE);
    }

    #[test]
    fn volatility_accumulates_by_bins_crossed() {
        let mut vol = Volatility {
            accumulator: 0,
            reference: 0,
            index_reference: 100,
            max: 1_000_000,
        };
        vol.accumulate(98);
        assert_eq!(vol.accumulator, 2 * 10_000);
        vol.accumulate(100);
        assert_eq!(vol.accumulator, 0);
    }

    #[test]
    fn single_bin_partial_applies_fee() {
        let res = swap_at_bin(1_000_000_000, 1.0, 1_000_000, u64::MAX, true);
        assert_eq!(res.amount_in, 1_000_000_000);
        assert_eq!(res.amount_out, 999_000_000);
    }

    #[test]
    fn full_bin_crossing_no_fee() {
        let res = swap_at_bin(1_000_000_000, 1.0, 0, 1000, true);
        assert_eq!(res.amount_in, 1000);
        assert_eq!(res.amount_out, 1000);
    }
}
