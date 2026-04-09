use anyhow::Context;
use raydium_clmm::accounts::PoolState;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

pub async fn fetch_pool_bytes(
    client: &RpcClient,
    pool_mint: Pubkey,
) -> Result<PoolState, anyhow::Error> {
    let data = client
        .get_account_data(&pool_mint)
        .await
        .context("Failed to get account data")?;
    PoolState::from_bytes(&data).context("Couldnt deserialize raydium pool data")
}

pub fn sqrt_price_to_price(sqrt_price: u128, decimals_a: u8, decimals_b: u8, a_to_b: bool) -> f64 {
    let sqrt_price = sqrt_price as f64 / 2u128.pow(64) as f64;
    let price = sqrt_price * sqrt_price;
    if a_to_b {
        price * 10f64.powi(decimals_a as i32 - decimals_b as i32)
    } else {
        1f64 / price * 10f64.powi(decimals_a as i32 - decimals_b as i32)
    }
}
