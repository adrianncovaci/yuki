pub const TICK_ARRAY_SIZE: usize = 88;

pub fn fetch_human_readable_price(
    sqrt_price: u128,
    base_decimals: u32,
    quote_decimals: u32,
) -> f64 {
    let two_64 = (1u128 << 64) as f64;
    let sqrt_price: f64 = sqrt_price as f64 / two_64;
    let price = sqrt_price * sqrt_price;
    price / (10f64.powi(base_decimals as i32 - quote_decimals as i32))
}

pub fn get_tick_array_start_tick_index(tick_index: i32, tick_spacing: u16) -> i32 {
    let tick_spacing_i32 = tick_spacing as i32;
    let tick_array_size_i32 = TICK_ARRAY_SIZE as i32;
    let real_index = tick_index
        .div_euclid(tick_spacing_i32)
        .div_euclid(tick_array_size_i32);
    real_index * tick_spacing_i32 * tick_array_size_i32
}
