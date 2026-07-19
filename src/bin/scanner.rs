use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use clap::Parser;
use solana_rpc_client::{api::config::CommitmentConfig, nonblocking::rpc_client::RpcClient};
use tokio::{fs::read_to_string, time::interval};

use yuki::{
    config::Config,
    venue::{Snapshot, VenueMeta},
};

struct Target {
    symbol: String,
    venues: Vec<VenueMeta>,
}

#[derive(Parser, Debug)]
struct CliArgs {
    #[arg(long, action)]
    daemon: bool,

    #[arg(long, default_value = "config.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    dotenvy::dotenv().ok();

    let cli_args = CliArgs::parse();

    let config: Config = toml::from_str(
        &read_to_string(&cli_args.config)
            .await
            .context("Failed to read config file")?,
    )
    .context("Failed to parse config")?;

    let client = RpcClient::new_with_timeout_and_commitment(
        std::env::var("RPC_NODE_URL").context("Failed to read rpc node url")?,
        Duration::from_secs(config.rpc.timeout_secs),
        CommitmentConfig::processed(),
    );

    let mut targets: Vec<Target> = vec![];
    for target in &config.targets {
        let decimals = target.decimals as u8;
        let mut venues = vec![];

        if let Some(address) = target.raydium_pool {
            match VenueMeta::raydium(&client, address, decimals).await {
                Ok(meta) => venues.push(meta),
                Err(err) => tracing::error!("{} | raydium meta: {err:#}", target.symbol),
            }
        }
        if let Some(address) = target.orca_pool {
            match VenueMeta::orca(&client, address, decimals).await {
                Ok(meta) => venues.push(meta),
                Err(err) => tracing::error!("{} | orca meta: {err:#}", target.symbol),
            }
        }
        if let Some(address) = target.meteora_dlmm_pool {
            match VenueMeta::meteora(&client, address, decimals).await {
                Ok(meta) => venues.push(meta),
                Err(err) => tracing::error!("{} | meteora meta: {err:#}", target.symbol),
            }
        }

        targets.push(Target {
            symbol: target.symbol.clone(),
            venues,
        });
    }

    tracing::info!("scanner starting | {} targets", targets.len());

    let probe: u64 = 100_000_000; // 0.1 SOL

    let scan_round = async || {
        for target in &targets {
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

            let mut snaps: Vec<(&VenueMeta, Snapshot)> = vec![];
            for meta in &target.venues {
                match Snapshot::fetch(&client, meta).await {
                    Ok(snap) => snaps.push((meta, snap)),
                    Err(err) => {
                        tracing::error!("{} | {} snapshot: {err:#}", target.symbol, meta.kind)
                    }
                }
            }

            let mut prices: Vec<(String, f64)> = snaps
                .iter()
                .map(|(meta, snap)| (meta.kind.to_string(), snap.sol_price(meta)))
                .collect();

            prices.sort_by(|a, b| a.1.total_cmp(&b.1));

            if prices.len() >= 2 {
                let (best, worst) = (prices[0].1, prices[prices.len() - 1].1);
                let spread_bps = (worst - best) / best * 10_000.0;
                let venues = prices
                    .iter()
                    .map(|(name, price)| format!("{name} {price:.6}"))
                    .collect::<Vec<_>>()
                    .join("  ");
                tracing::info!("{} | {venues} | spread {spread_bps:.1}bps", target.symbol);
            }

            for (i, (buy_meta, buy)) in snaps.iter().enumerate() {
                for (j, (sell_meta, sell)) in snaps.iter().enumerate() {
                    if i == j {
                        continue;
                    }

                    let token_out = match buy.quote(buy_meta, probe, true, now) {
                        Ok(v) => v,
                        Err(err) => {
                            tracing::warn!(
                                "{} | buy {} failed: {err:#}",
                                target.symbol,
                                buy_meta.kind
                            );
                            continue;
                        }
                    };
                    let sol_back = match sell.quote(sell_meta, token_out, false, now) {
                        Ok(v) => v,
                        Err(err) => {
                            tracing::warn!(
                                "{} | sell {} failed: {err:#}",
                                target.symbol,
                                sell_meta.kind
                            );
                            continue;
                        }
                    };

                    let net_bps = (sol_back as f64 - probe as f64) / probe as f64 * 10_000.0;
                    tracing::info!(
                        "{} | buy {} / sell {} | 0.1 SOL -> {token_out} -> {:.6} SOL | net {net_bps:+.1}bps",
                        target.symbol,
                        buy_meta.kind,
                        sell_meta.kind,
                        sol_back as f64 / 1e9,
                    );
                }
            }
        }

        anyhow::Ok(())
    };

    if cli_args.daemon {
        let mut tick = interval(Duration::from_secs(config.scanner.interval));
        loop {
            tick.tick().await;
            if let Err(err) = scan_round().await {
                tracing::error!("scan round failed: {err:?}");
            }
        }
    } else {
        scan_round().await?;
    }

    Ok(())
}
