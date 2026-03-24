use serde::Deserialize;
use std::time::Duration;
use tokio::{fs::File, io::AsyncReadExt};

use anyhow::Context;

use solana_rpc_client::{api::config::CommitmentConfig, nonblocking::rpc_client::RpcClient};

#[derive(Debug, Deserialize)]
struct Config {
    timeout_secs: u64,
    url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();

    let mut config_file = File::open("config.toml")
        .await
        .context("Failed to open config file")?;

    let mut file_str = String::new();

    config_file
        .read_to_string(&mut file_str)
        .await
        .context("Failed to read config file")?;

    let config = toml::from_str::<Config>(file_str.as_str())?;

    let commitment_config = CommitmentConfig::processed();
    let client = RpcClient::new_with_timeout_and_commitment(
        config.url,
        Duration::from_secs(config.timeout_secs),
        commitment_config,
    );

    tracing::info!("{:?}", client.get_slot().await);

    Ok(())
}
