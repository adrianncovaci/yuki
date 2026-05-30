use anyhow::Context;
use raydium_clmm::{
    accounts::{AmmConfig, PoolState},
    types::TickState,
};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

pub async fn estimate_output(
    amount: u64,
    ticks: &[TickState],
    pool: &PoolState,
    client: &RpcClient,
) -> anyhow::Result<f64> {
    let amm_config = client
        .get_account_data(&Pubkey::new_from_array(pool.amm_config.to_bytes()))
        .await
        .context("Failed to fetch amm config ")?;

    let amm_config = AmmConfig::from_bytes(&amm_config).context("Failed to create amm config")?;

    let sqrt_price = pool.sqrt_price_x64 as f64 / (1u128 << 64) as f64;
    let net_in = amount as f64 * (1.0 - amm_config.trade_fee_rate as f64 / 1_000_000.0);

    Ok(0.0)
}
