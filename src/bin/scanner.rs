use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use solana_rpc_client::{api::config::CommitmentConfig, nonblocking::rpc_client::RpcClient};
use solana_sdk::pubkey::Pubkey;
use tokio::fs::read_to_string;

use yuki::{
    config::Config,
    dex::{Dex, DexPool},
    meteora::dlmm::{bin_price_ui, dex::MeteoraDlmmDex},
    orca::dex::OrcaDex,
    raydium::dex::RaydiumDex,
};

/// Quote a single swap on one venue, EITHER direction, returning raw output
/// lamports. `input_is_sol = true` quotes SOL→token; `false` quotes token→SOL.
///
/// This is the scanner's one canonical "fetch + quote" path: it picks the swap
/// direction, fetches the direction-correct tick/bin arrays, and then calls the
/// same pure `Dex::quote` the DCA bot uses (no quote-math duplication).
async fn quote_leg(
    client: &RpcClient,
    pool: &DexPool,
    amount_in: u64,
    input_is_sol: bool,
) -> anyhow::Result<u64> {
    // Are we selling the FIRST token (mint0 / token_x / token_a)?
    let dir = input_is_sol == pool.is_sol_mint0();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let out = match pool {
        DexPool::Raydium { pool, address, .. } => {
            let amm = raydium_clmm::accounts::AmmConfig::from_bytes(
                &client
                    .get_account_data(&Pubkey::new_from_array(pool.amm_config.to_bytes()))
                    .await
                    .context("fetch amm config")?,
            )
            .context("decode amm config")?;
            let net_in = amount_in as f64 * (1.0 - amm.trade_fee_rate as f64 / 1_000_000.0);

            let ticks = yuki::raydium::tick::TickArray::new(
                *address,
                pool.tick_current,
                pool.tick_spacing,
                dir,
            )
            .fetch_and_deserialize(client, pool, dir)
            .await?;

            RaydiumDex::quote(yuki::raydium::quote::QuoteParams {
                net_in,
                ticks,
                pool: (**pool).clone(),
                swap_for_y: dir,
            })?
        }
        DexPool::Meteora { pool, address, .. } => {
            let bin_arrays =
                yuki::meteora::dlmm::bin::BinArrays::new(*address, pool.active_id, dir)
                    .fetch_and_deserialize(client)
                    .await?;

            MeteoraDlmmDex::quote(yuki::meteora::dlmm::dex::QuoteParams {
                amount_in,
                pair: (**pool).clone(),
                bin_arrays,
                timestamp: now as i64,
                swap_for_y: dir,
            })?
        }
        DexPool::Orca { pool, address, .. } => {
            let tick_arrays = yuki::orca::tick::TickArray::new(
                *address,
                pool.tick_current_index,
                pool.tick_spacing,
                dir,
            )?
            .fetch_and_deserialize(client)
            .await?;

            OrcaDex::quote(yuki::orca::dex::QuoteParams {
                token_in: amount_in,
                specified_token_a: dir,
                slippage_tolerance_bps: 0, // 0 = expected out, not min-out
                whirlpool: (**pool).clone().into(),
                oracle: None,
                tick_arrays,
                timestamp: now,
                transfer_fee_a: None,
                transfer_fee_b: None,
            })?
        }
    };

    Ok(out as u64)
}

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

        if let Some(address) = target.raydium_pool {
            match RaydiumDex::fetch_pool_bytes(&client, address).await {
                Ok(pool) => pools.push(DexPool::Raydium {
                    price: RaydiumDex::sqrt_price_to_price(
                        pool.sqrt_price_x64,
                        pool.mint_decimals0,
                        pool.mint_decimals1,
                    ),
                    pool: Box::new(pool),
                    address,
                }),
                Err(err) => {
                    tracing::error!("{} | failed to fetch Raydium pool: {err}", target.symbol)
                }
            }
        }

        if let Some(address) = target.orca_pool {
            match OrcaDex::fetch_pool_bytes(&client, address).await {
                Ok(pool) => {
                    let (dec_0, dec_1) = if pool.token_mint_a == spl_token::native_mint::id() {
                        (9u8, target.decimals as u8)
                    } else {
                        (target.decimals as u8, 9u8)
                    };
                    pools.push(DexPool::Orca {
                        price: OrcaDex::sqrt_price_to_price(pool.sqrt_price, dec_0, dec_1),
                        pool: Box::new(pool),
                        address,
                    })
                }
                Err(err) => tracing::error!("{} | failed to fetch Orca pool: {err}", target.symbol),
            }
        }

        if let Some(address) = target.meteora_dlmm_pool {
            match MeteoraDlmmDex::fetch_pool_bytes(&client, address).await {
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
                        address,
                    })
                }
                Err(err) => {
                    tracing::error!("{} | failed to fetch Meteora pool: {err}", target.symbol)
                }
            }
        }

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

        let probe = 100_000_000;
        for (i, buy) in pools.iter().enumerate() {
            for (j, sell) in pools.iter().enumerate() {
                if i == j {
                    continue;
                }

                let res = async {
                    let token_out = quote_leg(&client, buy, probe, true).await?;
                    let sol_out = quote_leg(&client, sell, token_out, false).await?;
                    anyhow::Ok((token_out, sol_out))
                }
                .await;

                match res {
                    Ok((token_out, sol_out)) => {
                        let net_bps = (sol_out as f64 - probe as f64) / probe as f64 * 10_000.0;
                        tracing::info!(
                            "{} | buy {} / sell {} | 0.1 SOL -> {token_out} -> {:.6} SOL | net {net_bps:+.1}bps",
                            target.symbol,
                            buy.name(),
                            sell.name(),
                            sol_out as f64 / 1e9,
                        );
                    }
                    Err(err) => tracing::warn!(
                        "{} | {} -> {} failed: {err:#}",
                        target.symbol,
                        buy.name(),
                        sell.name()
                    ),
                }
            }
        }
    }

    Ok(())
}
