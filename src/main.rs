pub mod config;
pub mod dex;
pub mod dto;
pub mod orca;
pub mod raydium;
pub mod telegram;
#[cfg(test)]
pub mod test;
pub mod transfer;
pub mod wsol;

use clap::Parser;
use solana_sdk::{pubkey::Pubkey, signature::read_keypair_file, signer::Signer};
use std::str::FromStr;
use std::time::Duration;
use tokio::{fs::read_to_string, time::interval};

use anyhow::Context;

use solana_rpc_client::{api::config::CommitmentConfig, nonblocking::rpc_client::RpcClient};

use crate::{
    config::Config,
    dex::{Dex, DexPool},
    dto::SwapError,
    orca::{dca::orca_dca_target, dex::OrcaDex},
    raydium::{dca::raydium_dca_target, dex::RaydiumDex},
};

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
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let cli_args = CliArgs::parse();

    let config_str = read_to_string(&cli_args.config)
        .await
        .context("Failed to read config file")?;

    let config = toml::from_str::<Config>(config_str.as_str())?;

    let commitment_config = CommitmentConfig::processed();

    let client = RpcClient::new_with_timeout_and_commitment(
        config.rpc.url.clone(),
        Duration::from_secs(config.rpc.timeout_secs),
        commitment_config,
    );

    let dca_wallet = read_keypair_file(&config.wallet.keypair_path)
        .map_err(|e| anyhow::anyhow!("Failed to read keypair: {e}"))?;

    let total_weight: u32 = config.targets.iter().map(|x| x.weight).sum();

    let mut interval = interval(Duration::from_secs(config.swap.interval));

    let rent_exempt = client
        .get_minimum_balance_for_rent_exemption(165)
        .await
        .context("Failed to fetch minimum balance for rent exemption")?;

    let cold_wallet: Option<Pubkey> = std::env::var("COLD_WALLET")
        .ok()
        .map(|s| Pubkey::from_str(&s).expect("Invalid COLD_WALLET address"));

    if let Some(cw) = cold_wallet {
        tracing::info!("Cold wallet configured: {cw}");
    }

    let tg = match (
        std::env::var("TELEGRAM_BOT_TOKEN"),
        std::env::var("TELEGRAM_CHAT_ID"),
    ) {
        (Ok(token), Ok(chat_id)) => {
            tracing::info!("Telegram notifications enabled");
            Some(telegram::Telegram::new(token, chat_id))
        }
        _ => None,
    };

    let dca_run = async || -> anyhow::Result<()> {
        let balance = client.get_balance(&dca_wallet.pubkey()).await?;
        let needed = (config.swap.amount_sol + config.wallet.reserve_sol) * 1e9;

        tracing::info!(
            "DCA round starting | balance: {:.4} SOL",
            balance as f64 / 1e9
        );

        if (balance as f64) < needed {
            let msg = format!(
                "Insufficient balance: {:.4} SOL < {:.4} SOL needed",
                balance as f64 / 1e9,
                needed / 1e9
            );
            tracing::warn!("{msg}");
            if let Some(ref tg) = tg
                && let Err(e) = tg.send(&msg).await
            {
                tracing::error!("Telegram send failed: {e:#}");
            }
        } else {
            let mut results: Vec<String> = vec![];

            for target in &config.targets {
                let mut pools = vec![];

                if let Some(pool) = target.raydium_pool {
                    match RaydiumDex::fetch_pool_bytes(&client, pool).await {
                        Ok(pool) => pools.push(DexPool::Raydium {
                            price: RaydiumDex::sqrt_price_to_price(
                                pool.sqrt_price_x64,
                                pool.mint_decimals0,
                                pool.mint_decimals1,
                            ),
                            pool: Box::new(pool),
                        }),
                        Err(err) => {
                            tracing::error!(
                                "{} | failed to fetch Raydium pool: {err}",
                                target.symbol
                            );
                        }
                    }
                };

                if let Some(pool) = target.orca_pool {
                    match OrcaDex::fetch_pool_bytes(&client, pool).await {
                        Ok(pool) => {
                            let (dec_0, dec_1) =
                                if pool.token_mint_a == spl_token::native_mint::id() {
                                    (9u8, target.decimals as u8)
                                } else {
                                    (target.decimals as u8, 9u8)
                                };

                            pools.push(DexPool::Orca {
                                price: OrcaDex::sqrt_price_to_price(pool.sqrt_price, dec_0, dec_1),
                                pool: Box::new(pool),
                            })
                        }
                        Err(err) => {
                            tracing::error!("{} | failed to fetch Orca pool: {err}", target.symbol);
                        }
                    }
                };

                pools.sort_by(|a, b| a.sol_price().total_cmp(&b.sol_price()));

                if pools.len() > 1 {
                    let best = &pools[0];
                    let second = &pools[1];
                    let diff_bps = ((best.sol_price() - second.sol_price()) / best.sol_price()
                        * 10_000.0)
                        .abs() as u32;
                    tracing::info!(
                        "{} | best={} ({:.6}) vs {}={:.6} | diff={}bps",
                        target.symbol,
                        best.name(),
                        best.sol_price(),
                        second.name(),
                        second.sol_price(),
                        diff_bps
                    );
                } else if pools.len() == 1 {
                    tracing::info!(
                        "{} | only {} ({:.6})",
                        target.symbol,
                        pools[0].name(),
                        pools[0].sol_price()
                    );
                } else {
                    tracing::warn!("{} | no pools available", target.symbol);
                }

                for pool in pools {
                    let dex_name = pool.name();

                    let result = match pool {
                        DexPool::Orca {
                            pool: whirlpool, ..
                        } => {
                            orca_dca_target(
                                &config,
                                &client,
                                &dca_wallet,
                                target,
                                total_weight,
                                rent_exempt,
                                whirlpool,
                                cold_wallet,
                            )
                            .await
                        }
                        DexPool::Raydium {
                            pool: raydium_pool, ..
                        } => {
                            raydium_dca_target(
                                &config,
                                &client,
                                &dca_wallet,
                                target,
                                total_weight,
                                rent_exempt,
                                raydium_pool,
                                cold_wallet,
                            )
                            .await
                        }
                    };

                    match result {
                        Ok(swap) => {
                            results.push(swap.to_string());
                            break;
                        }
                        Err(err) => {
                            let mut swap_err = SwapError {
                                symbol: target.symbol.clone(),
                                dex: dex_name.to_string(),
                                reason: format!("{err:#}"),
                            };
                            tracing::warn!("{swap_err}");
                            if swap_err.reason.len() > 200 {
                                swap_err.reason = format!(
                                    "{}...",
                                    swap_err.reason.chars().take(200).collect::<String>()
                                );
                            }
                            results.push(swap_err.to_string());
                        }
                    }
                }
            }

            if let Some(ref tg) = tg {
                let remaining = client.get_balance(&dca_wallet.pubkey()).await.unwrap_or(0);
                let summary = format!(
                    "DCA Round Complete\n{:?}\nBalance: {:.4} SOL\n",
                    remaining as f64 / 1e9,
                    results.join("\n"),
                );
                if let Err(e) = tg.send(&summary).await {
                    tracing::error!("Telegram send failed: {e:#}");
                }
            }
        }

        Ok(())
    };

    if cli_args.daemon {
        loop {
            interval.tick().await;
            if let Err(err) = dca_run().await {
                tracing::error!("DCA round failed: {err:?}!");
            }
        }
    } else {
        if let Err(err) = dca_run().await {
            tracing::error!("DCA round failed: {err:?}");
        }
    }

    Ok(())
}
