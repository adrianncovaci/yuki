use anyhow::Context;
use orca_whirlpools_client::Whirlpool;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

pub async fn fetch_pool_bytes(
    client: &RpcClient,
    pool_mint: Pubkey,
) -> Result<Whirlpool, anyhow::Error> {
    let address = client
        .get_account_data(&pool_mint)
        .await
        .context("Failed to get account data")?;
    Whirlpool::from_bytes(&address).context("Failed to extract whirlpool account")
}

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
