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
