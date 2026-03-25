pub mod config;

use solana_sdk::{signature::read_keypair_file, signer::Signer};
use std::time::Duration;
use tokio::fs::read_to_string;

use anyhow::Context;

use solana_rpc_client::{api::config::CommitmentConfig, nonblocking::rpc_client::RpcClient};

use crate::config::Config;

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

    Ok(())
}
