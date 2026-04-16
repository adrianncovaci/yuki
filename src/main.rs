pub mod config;
pub mod dex;
pub mod orca;
pub mod raydium;
pub mod telegram;
pub mod wsol;

use clap::Parser;
use orca_whirlpools_client::{Whirlpool, get_oracle_address};
use raydium_clmm::accounts::PoolState;
use solana_sdk::{
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::{Keypair, read_keypair_file},
    signer::Signer,
};
use spl_associated_token_account_interface::{
    address::get_associated_token_address, instruction::create_associated_token_account_idempotent,
};
use std::str::FromStr;
use std::time::Duration;
use tokio::{fs::read_to_string, time::interval};

use anyhow::{Context, bail};

use solana_rpc_client::{
    api::{config::CommitmentConfig, response::transaction::Transaction},
    nonblocking::rpc_client::RpcClient,
};

use crate::{
    config::{Config, Target},
    dex::{Dex, DexPool},
    orca::dex::OrcaDex,
    raydium::dex::RaydiumDex,
    wsol::WsolSwap,
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
        .with_max_level(tracing::Level::TRACE)
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
                "⚠️ Insufficient balance: {:.4} SOL < {:.4} SOL needed",
                balance as f64 / 1e9,
                needed / 1e9
            );
            tracing::warn!("{msg}");
            if let Some(ref tg) = tg {
                let _ = tg.send(&msg).await;
            }
        } else {
            let mut results: Vec<String> = vec![];

            for target in &config.targets {
                tracing::info!("{:?}", target.symbol);

                let mut pools = vec![];

                if let Some(pool) = target.raydium_pool {
                    match RaydiumDex::fetch_pool_bytes(&client, pool).await {
                        Ok(pool) => {
                            pools.push(DexPool::Raydium {
                                price: RaydiumDex::sqrt_price_to_price(
                                    pool.sqrt_price_x64,
                                    pool.mint_decimals0,
                                    pool.mint_decimals1,
                                ),
                                pool,
                            })
                        }
                        Err(err) => {
                            tracing::error!("Failed to fetch raydium pool: {err:?}");
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
                                pool,
                            })
                        }
                        Err(err) => {
                            tracing::error!("Failed to fetch orca pool: {err:?}");
                        }
                    }
                };

                pools.sort_by(|a, b| a.sol_price().total_cmp(&b.sol_price()));

                let amount_sol =
                    config.swap.amount_sol * (target.weight as f64 / total_weight as f64);

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
                        Ok(()) => {
                            results.push(format!(
                                "✅ {} | {:.4} SOL -> {}",
                                target.symbol, amount_sol, dex_name
                            ));
                            break;
                        }
                        Err(err) => {
                            tracing::warn!(
                                "{} | {} failed: {err:#}, trying next...",
                                target.symbol,
                                dex_name
                            );
                            results.push(format!(
                                "❌ {} | {} failed: {}",
                                target.symbol, dex_name, err
                            ));
                        }
                    }
                }

                if let Some(ref tg) = tg {
                    let remaining = client.get_balance(&dca_wallet.pubkey()).await.unwrap_or(0);
                    let summary = format!(
                        "🔄 DCA Round Complete\nBalance: {:.4} SOL\n\n{}",
                        remaining as f64 / 1e9,
                        results.join("\n")
                    );
                    let _ = tg.send(&summary).await;
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

#[allow(clippy::too_many_arguments)]
pub async fn orca_dca_target(
    config: &Config,
    client: &RpcClient,
    wallet: &Keypair,
    target: &Target,
    total_weight: u32,
    rent_exempt: u64,
    whirlpool: Whirlpool,
    cold_wallet: Option<Pubkey>,
) -> anyhow::Result<()> {
    let pool_address = target
        .orca_pool
        .context(format!("{}: no orca pool configured", target.symbol))?;

    let (oracle, _) = get_oracle_address(&pool_address)?;

    let price = OrcaDex::sqrt_price_to_price(whirlpool.sqrt_price, target.decimals as u8, 9);

    let amount = (config.swap.amount_sol
        * LAMPORTS_PER_SOL as f64
        * (target.weight as f64 / total_weight as f64)) as u64;

    tracing::info!(
        "{} | price: {:.6} | amount: {:.4} SOL",
        target.symbol,
        price,
        amount as f64 / 1e9
    );

    let recent_blockhash = client
        .get_latest_blockhash()
        .await
        .context("Failed to get latest blockhash")?;

    let ata = get_associated_token_address(&wallet.pubkey(), &target.mint_address);
    let wsol_swap = WsolSwap::new(&wallet.pubkey(), amount, rent_exempt)?;

    let tick_array = orca::tick::TickArray::new(
        pool_address,
        whirlpool.tick_current_index,
        whirlpool.tick_spacing,
        whirlpool.token_mint_a == spl_token::native_mint::ID,
    )?;

    let swap_params = orca::dex::SwapParams {
        whirlpool,
        pool_address,
        amount,
        slippage_bps: config.swap.slippage_bps,
        b_token_decimals: target.decimals,
        b_token_price: price,
        authority: wallet.pubkey(),
        wsol_ata: wsol_swap.keypair.pubkey(),
        dca_token_ata: ata,
        tick_arrays: tick_array.inner,
        oracle,
        amount_specified_is_input: true,
    };

    let swap_ix = OrcaDex::swap(swap_params);

    let tx = Transaction::new_signed_with_payer(
        &[
            wsol_swap.create_ix,
            wsol_swap.init_ix,
            wsol_swap.sync_ix,
            create_associated_token_account_idempotent(
                &wallet.pubkey(),
                &wallet.pubkey(),
                &target.mint_address,
                &spl_token::id(),
            ),
            swap_ix,
            wsol_swap.close_ix,
        ],
        Some(&wallet.pubkey()),
        &[wallet, &wsol_swap.keypair],
        recent_blockhash,
    );

    let signature = client.send_and_confirm_transaction(&tx).await?;

    tracing::info!(
        "{} | swapped {:.4} SOL | tx: {signature}",
        target.symbol,
        amount as f64 / 1e9
    );

    transfer_to_cold_wallet(client, wallet, target, ata, cold_wallet).await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn raydium_dca_target(
    config: &Config,
    client: &RpcClient,
    wallet: &Keypair,
    target: &Target,
    total_weight: u32,
    rent_exempt: u64,
    pool: PoolState,
    cold_wallet: Option<Pubkey>,
) -> anyhow::Result<()> {
    let pool_address = target
        .raydium_pool
        .context(format!("{}: no raydium pool configured", target.symbol))?;

    let sol_is_mint0 = pool.token_mint0.to_bytes() == spl_token::native_mint::id().to_bytes();

    let price = RaydiumDex::sqrt_price_to_price(pool.sqrt_price_x64, target.decimals as u8, 9);

    let amount = (config.swap.amount_sol
        * LAMPORTS_PER_SOL as f64
        * (target.weight as f64 / total_weight as f64)) as u64;

    tracing::info!(
        "{} | price: {:.6} | amount: {:.4} SOL",
        target.symbol,
        price,
        amount as f64 / 1e9
    );

    let recent_blockhash = client
        .get_latest_blockhash()
        .await
        .context("Failed to get latest blockhash")?;

    let ata = get_associated_token_address(&wallet.pubkey(), &target.mint_address);
    let wsol_swap = WsolSwap::new(&wallet.pubkey(), amount, rent_exempt)?;

    let tick_array = raydium::tick::TickArray::new(
        pool_address,
        pool.tick_current,
        pool.tick_spacing,
        sol_is_mint0,
    );

    let (input_vault, output_vault) = if sol_is_mint0 {
        (
            Pubkey::new_from_array(pool.token_vault0.to_bytes()),
            Pubkey::new_from_array(pool.token_vault1.to_bytes()),
        )
    } else {
        (
            Pubkey::new_from_array(pool.token_vault1.to_bytes()),
            Pubkey::new_from_array(pool.token_vault0.to_bytes()),
        )
    };

    let swap_params = raydium::swap::SwapParams {
        signer: wallet.pubkey(),
        pool_address,
        observation_state: Pubkey::new_from_array(pool.observation_key.to_bytes()),
        pool_state: pool,
        input_token_account: wsol_swap.keypair.pubkey(),
        output_token_account: ata,
        input_vault,
        output_vault,
        input_vault_mint: spl_token::native_mint::id(),
        output_vault_mint: target.mint_address,
        amount,
        slippage_bps: config.swap.slippage_bps,
        token_decimals: target.decimals,
        token_price: price,
        tick_arrays: tick_array.inner,
    };

    let swap_ix = RaydiumDex::swap(swap_params);

    let tx = Transaction::new_signed_with_payer(
        &[
            wsol_swap.create_ix,
            wsol_swap.init_ix,
            wsol_swap.sync_ix,
            create_associated_token_account_idempotent(
                &wallet.pubkey(),
                &wallet.pubkey(),
                &target.mint_address,
                &spl_token::id(),
            ),
            swap_ix,
            wsol_swap.close_ix,
        ],
        Some(&wallet.pubkey()),
        &[wallet, &wsol_swap.keypair],
        recent_blockhash,
    );

    let signature = client.send_and_confirm_transaction(&tx).await?;

    tracing::info!(
        "{} | swapped {:.4} SOL | tx: {signature}",
        target.symbol,
        amount as f64 / 1e9
    );

    transfer_to_cold_wallet(client, wallet, target, ata, cold_wallet).await?;

    Ok(())
}

pub async fn transfer_to_cold_wallet(
    client: &RpcClient,
    wallet: &Keypair,
    target: &Target,
    ata: Pubkey,
    cold_wallet: Option<Pubkey>,
) -> anyhow::Result<()> {
    let cold_wallet = match cold_wallet {
        Some(cold_wallet) => cold_wallet,
        None => return Ok(()),
    };

    let target_ata = get_associated_token_address(&cold_wallet, &target.mint_address);

    let balance = client
        .get_token_account_balance(&ata)
        .await
        .context("Failed to get ata client")?;

    let balance = match balance.amount.parse::<u64>() {
        Ok(0) => {
            tracing::warn!("ATA balance is equal to 0");
            return Ok(());
        }
        Ok(balance) => balance,
        Err(err) => {
            bail!("Failed to parse the amount: {err:?}");
        }
    };

    let fresh_hash = client
        .get_latest_blockhash()
        .await
        .context("Failed to fetch latest blockhash")?;

    let ix = spl_token::instruction::transfer(
        &spl_token::id(),
        &ata,
        &target_ata,
        &wallet.pubkey(),
        &[],
        balance,
    )
    .context("Failed to create transfer ix")?;

    let tx = Transaction::new_signed_with_payer(
        &[
            create_associated_token_account_idempotent(
                &wallet.pubkey(),
                &cold_wallet,
                &target.mint_address,
                &spl_token::id(),
            ),
            ix,
        ],
        Some(&wallet.pubkey()),
        &[wallet],
        fresh_hash,
    );

    let hash = client
        .send_and_confirm_transaction(&tx)
        .await
        .context("Failed to send tx")?;

    tracing::info!(
        "{} | transferred {} to cold wallet | tx: {hash}",
        target.symbol,
        balance
    );

    Ok(())
}
