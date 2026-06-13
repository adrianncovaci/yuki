use std::time::Duration;

use anyhow::Context;
use solana_rpc_client::{api::config::CommitmentConfig, nonblocking::rpc_client::RpcClient};
use tokio::fs::read_to_string;

use yuki::{
    config::Config,
    dex::{Dex, DexPool},
    meteora::dlmm::{bin_price_ui, dex::MeteoraDlmmDex},
    orca::dex::OrcaDex,
    raydium::dex::RaydiumDex,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config: Config = toml::from_str(
        &read_to_string("config.toml")
            .await
            .context("Failed to read config.toml")?,
    )
    .context("Failed to parse config")?;

    let client = RpcClient::new_with_timeout_and_commitment(
        config.rpc.url.clone(),
        Duration::from_secs(config.rpc.timeout_secs),
        CommitmentConfig::processed(),
    );

    tracing::info!("scanner starting | {} targets", config.targets.len());

    for target in &config.targets {
        let mut pools: Vec<DexPool> = vec![];

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
                    tracing::error!("{} | failed to fetch Raydium pool: {err}", target.symbol)
                }
            }
        }

        if let Some(pool) = target.orca_pool {
            match OrcaDex::fetch_pool_bytes(&client, pool).await {
                Ok(pool) => {
                    let (dec_0, dec_1) = if pool.token_mint_a == spl_token::native_mint::id() {
                        (9u8, target.decimals as u8)
                    } else {
                        (target.decimals as u8, 9u8)
                    };
                    pools.push(DexPool::Orca {
                        price: OrcaDex::sqrt_price_to_price(pool.sqrt_price, dec_0, dec_1),
                        pool: Box::new(pool),
                    })
                }
                Err(err) => tracing::error!("{} | failed to fetch Orca pool: {err}", target.symbol),
            }
        }

        if let Some(pool) = target.meteora_dlmm_pool {
            match MeteoraDlmmDex::fetch_pool_bytes(&client, pool).await {
                Ok(pool) => {
                    let (dec_x, dec_y) = if pool.token_x_mint.to_bytes()
                        == spl_token::native_mint::id().to_bytes()
                    {
                        (9u8, target.decimals as u8)
                    } else {
                        (target.decimals as u8, 9u8)
                    };
                    pools.push(DexPool::Meteora {
                        price: bin_price_ui(pool.active_id, pool.bin_step, dec_x, dec_y),
                        pool: Box::new(pool),
                    })
                }
                Err(err) => {
                    tracing::error!("{} | failed to fetch Meteora pool: {err}", target.symbol)
                }
            }
        }

        // Cheapest SOL-per-token first; the spread between the cheapest and
        // dearest venue is the (mid-price, indicative) arbitrage headline.
        pools.sort_by(|a, b| a.sol_price().total_cmp(&b.sol_price()));

        match pools.len() {
            0 => tracing::warn!("{} | no pools available", target.symbol),
            1 => tracing::info!(
                "{} | only {} ({:.6}) | no spread",
                target.symbol,
                pools[0].name(),
                pools[0].sol_price()
            ),
            _ => {
                let best = pools[0].sol_price();
                let worst = pools[pools.len() - 1].sol_price();
                let spread_bps = (worst - best) / best * 10_000.0;
                let venues = pools
                    .iter()
                    .map(|p| format!("{} {:.6}", p.name(), p.sol_price()))
                    .collect::<Vec<_>>()
                    .join("  ");
                tracing::info!("{} | {venues} | spread {spread_bps:.1}bps", target.symbol);
            }
        }
    }

    Ok(())
}
