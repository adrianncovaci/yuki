use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use clap::Parser;
use solana_rpc_client::{api::config::CommitmentConfig, nonblocking::rpc_client::RpcClient};
use tokio::{fs::read_to_string, time::interval};

use yuki::{
    config::{Config, Scanner},
    telegram::Telegram,
    venue::{Snapshot, VenueMeta},
};

struct Target {
    symbol: String,
    venues: Vec<VenueMeta>,
}

struct ScanState {
    cooldowns: HashMap<String, Instant>,
    started_at: Instant,
    scan_rounds: u64,
    alerts_fired: u64,
    best_opportunity: Option<(f64, String)>,
}

impl ScanState {
    fn new() -> Self {
        Self {
            cooldowns: HashMap::new(),
            started_at: Instant::now(),
            scan_rounds: 0,
            alerts_fired: 0,
            best_opportunity: None,
        }
    }
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

    let tg = match (
        std::env::var("TELEGRAM_BOT_TOKEN"),
        std::env::var("TELEGRAM_CHAT_ID"),
    ) {
        (Ok(token), Ok(chat_id)) => {
            tracing::info!("Telegram notifications enabled");
            Some(Telegram::new(token, chat_id))
        }
        _ => None,
    };

    tracing::info!("scanner starting | {} targets", targets.len());

    let probe: u64 = 100_000_000; // 0.1 SOL

    let mut state = ScanState::new();

    if cli_args.daemon {
        let mut tick = interval(Duration::from_secs(config.scanner.interval));
        let mut digest_tick = interval(Duration::from_secs(config.scanner.digest_interval_secs));
        digest_tick.tick().await; // interval fires immediately once; skip so we don't digest on every restart

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(err) =
                        scan_round(&client, &targets, probe, &config.scanner, tg.as_ref(), &mut state).await
                    {
                        tracing::error!("scan round failed: {err:?}");
                    }
                }
                _ = digest_tick.tick() => {
                    send_digest(tg.as_ref(), &mut state).await;
                }
            }
        }
    } else {
        scan_round(
            &client,
            &targets,
            probe,
            &config.scanner,
            tg.as_ref(),
            &mut state,
        )
        .await?;
    }

    Ok(())
}

async fn scan_round(
    client: &RpcClient,
    targets: &[Target],
    probe: u64,
    config: &Scanner,
    tg: Option<&Telegram>,
    state: &mut ScanState,
) -> anyhow::Result<()> {
    state.scan_rounds += 1;

    for target in targets {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        let mut snaps: Vec<(&VenueMeta, Snapshot)> = vec![];
        for meta in &target.venues {
            match Snapshot::fetch(client, meta).await {
                Ok(snap) => snaps.push((meta, snap)),
                Err(err) => tracing::error!("{} | {} snapshot: {err:#}", target.symbol, meta.kind),
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
                        tracing::warn!("{} | buy {} failed: {err:#}", target.symbol, buy_meta.kind);
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

                let is_best = state
                    .best_opportunity
                    .as_ref()
                    .is_none_or(|(best, _)| net_bps > *best);
                if is_best {
                    state.best_opportunity = Some((
                        net_bps,
                        format!(
                            "{} buy {}/sell {}",
                            target.symbol, buy_meta.kind, sell_meta.kind
                        ),
                    ));
                }

                if net_bps < config.alert_threshold_bps {
                    continue;
                }

                let key = format!("{}:{}:{}", target.symbol, buy_meta.kind, sell_meta.kind);
                let cooldown = Duration::from_secs(config.alert_cooldown_secs);
                let on_cooldown = state
                    .cooldowns
                    .get(&key)
                    .is_some_and(|last| last.elapsed() < cooldown);
                if on_cooldown {
                    continue;
                }

                state.cooldowns.insert(key, Instant::now());
                state.alerts_fired += 1;

                let Some(tg) = tg else { continue };
                let msg = format!(
                    "[scanner] {} | buy {} / sell {} | net {net_bps:+.1}bps",
                    target.symbol, buy_meta.kind, sell_meta.kind
                );
                if let Err(err) = tg.send(&msg).await {
                    tracing::error!("Telegram alert failed: {err:#}");
                }
            }
        }
    }

    Ok(())
}

async fn send_digest(tg: Option<&Telegram>, state: &mut ScanState) {
    let uptime_hours = state.started_at.elapsed().as_secs_f64() / 3600.0;
    let best = state
        .best_opportunity
        .as_ref()
        .map(|(bps, desc)| format!("{desc} ({bps:+.1}bps)"))
        .unwrap_or_else(|| "none".to_string());

    let msg = format!(
        "[scanner] digest | uptime {uptime_hours:.1}h\nScan rounds: {}\nAlerts fired: {}\nBest spread: {best}",
        state.scan_rounds, state.alerts_fired,
    );

    tracing::info!("{msg}");

    if let Some(tg) = tg
        && let Err(err) = tg.send(&msg).await
    {
        tracing::error!("Telegram digest failed: {err:#}");
    }

    state.scan_rounds = 0;
    state.alerts_fired = 0;
    state.best_opportunity = None;
}
