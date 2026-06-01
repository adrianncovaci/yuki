pub struct PoolSpec {
    pub sqrt_price_x64: u128,
    pub liquidity: u128,
    pub tick_current: i32,
    pub tick_spacing: u16,
    pub fee_rate_pm: u32,
    pub sol_is_mint0: bool,
    pub ticks: Vec<(i32, i128, u128)>,
}
