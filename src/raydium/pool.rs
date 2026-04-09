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
