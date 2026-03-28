pub mod config;
pub mod orca_amm;

use solana_sdk::{signature::read_keypair_file, signer::Signer};
use std::time::Duration;
use tokio::fs::read_to_string;

use anyhow::Context;

use solana_rpc_client::{api::config::CommitmentConfig, nonblocking::rpc_client::RpcClient};

use crate::{
    config::Config,
    orca_amm::{fetch_human_readable_price, fetch_pool_bytes},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();

    let config_str = read_to_string("config.toml")
        .await
        .context("Failed to read config file")?;

    let config = toml::from_str::<Config>(config_str.as_str())?;

    let commitment_config = CommitmentConfig::processed();

    let client = RpcClient::new_with_timeout_and_commitment(
        config.rpc.url,
        Duration::from_secs(config.rpc.timeout_secs),
        commitment_config,
    );

    let dca_wallet = read_keypair_file(&config.wallet.keypair_path)
        .map_err(|e| anyhow::anyhow!("Failed to read keypair: {e}"))?;

    let balance = client.get_balance(&dca_wallet.pubkey()).await;

    tracing::info!("balance = {balance:?}, pubkey = {:?}", dca_wallet.pubkey());

    let jto_orca_pool = config
        .targets
        .iter()
        .find(|x| x.symbol.as_str() == "JTO")
        .context("Failed to find jto target")?;

    let jto_whirlpool = fetch_pool_bytes(&client, jto_orca_pool.orca_pool).await?;

    let jto_price = fetch_human_readable_price(jto_whirlpool.sqrt_price, jto_orca_pool.decimals, 9);

    tracing::info!("jito price = {jto_price:#?}");

    let usdc_orca_pool = config
        .targets
        .iter()
        .find(|x| x.symbol.as_str() == "USDC")
        .context("Failed to find usdc target")?;

    let usdc_whirlpool = fetch_pool_bytes(&client, usdc_orca_pool.orca_pool).await?;

    let usdc_price =
        fetch_human_readable_price(usdc_whirlpool.sqrt_price, usdc_orca_pool.decimals, 9);

    tracing::info!("usdc price = {usdc_price:#?}");

    Ok(())
}
