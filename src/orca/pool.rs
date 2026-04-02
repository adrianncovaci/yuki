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
