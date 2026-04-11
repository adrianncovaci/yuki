pub mod config;
pub mod orca;
pub mod raydium;
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
use std::time::Duration;
use tokio::{fs::read_to_string, time::interval};

use anyhow::Context;

use solana_rpc_client::{
    api::{config::CommitmentConfig, response::transaction::Transaction},
    nonblocking::rpc_client::RpcClient,
};

use crate::raydium::pool::sqrt_price_to_price;
use crate::{
    config::{Config, Target},
    wsol::WsolSwap,
};

#[derive(Parser, Debug)]
struct CliArgs {
    #[arg(long, action)]
    daemon: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();

    let cli_args = CliArgs::parse();

    let config_str = read_to_string("config.toml")
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

    let dca_run = async || -> anyhow::Result<()> {
        let balance = client.get_balance(&dca_wallet.pubkey()).await?;
        let needed = (config.swap.amount_sol + config.wallet.reserve_sol) * 1e9;

        tracing::info!(
            "DCA round starting | balance: {:.4} SOL",
            balance as f64 / 1e9
        );

        if (balance as f64) < needed {
            tracing::warn!(
                "Insufficient balance: {:.4} SOL < {:.4} SOL needed",
                balance as f64 / 1e9,
                needed / 1e9
            );
        } else {
            for target in &config.targets {
                tracing::info!("{:?}", target.symbol);

                let (raydium_price, raydium_pool) = if let Some(pool) = target.raydium_pool {
                    let data = raydium::pool::fetch_pool_bytes(&client, pool).await?;

                    let a_to_b =
                        data.token_mint0.to_bytes() == spl_token::native_mint::id().to_bytes();
                    (
                        Some(raydium::pool::sqrt_price_to_price(
                            data.sqrt_price_x64,
                            data.mint_decimals0,
                            data.mint_decimals1,
                            a_to_b,
                        )),
                        Some(data),
                    )
                } else {
                    (None, None)
                };

                let (orca_price, whirlpool) = if let Some(pool) = target.orca_pool {
                    let data = orca::pool::fetch_pool_bytes(&client, pool).await?;

                    let a_to_b = data.token_mint_a == spl_token::native_mint::id();

                    (
                        Some(raydium::pool::sqrt_price_to_price(
                            data.sqrt_price,
                            target.decimals as u8,
                            9,
                            a_to_b,
                        )),
                        Some(data),
                    )
                } else {
                    (None, None)
                };

                tracing::info!("raydium price: {raydium_price:?}, orca price: {orca_price:?}");

                let should_use_orca = match (orca_price, raydium_price) {
                    (Some(o), Some(r)) => o < r,
                    (Some(_), None) => true,
                    (None, Some(_)) => false,
                    (None, None) => {
                        tracing::error!("no pool");
                        continue;
                    }
                };

                if should_use_orca {
                    if let Err(err) = orca_dca_target(
                        &config,
                        &client,
                        &dca_wallet,
                        target,
                        total_weight,
                        rent_exempt,
                        whirlpool.context("Failed to fetch whirlpool")?,
                    )
                    .await
                    {
                        tracing::error!("Failed to dca {target:?}: {err:?}");
                    }
                } else {
                    if let Err(err) = raydium_dca_target(
                        &config,
                        &client,
                        &dca_wallet,
                        target,
                        total_weight,
                        rent_exempt,
                        raydium_pool.context("Failed to fetch raydium pool")?,
                    )
                    .await
                    {
                        tracing::error!("Failed to dca {target:?}: {err:?}");
                    }
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

pub async fn orca_dca_target(
    config: &Config,
    client: &RpcClient,
    wallet: &Keypair,
    target: &Target,
    total_weight: u32,
    rent_exempt: u64,
    whirlpool: Whirlpool,
) -> anyhow::Result<()> {
    let pool_address = target
        .orca_pool
        .context(format!("{}: no orca pool configured", target.symbol))?;

    let a_to_b = whirlpool.token_mint_a == spl_token::native_mint::id();

    let (oracle, _) = get_oracle_address(&pool_address)?;

    let price = sqrt_price_to_price(whirlpool.sqrt_price, target.decimals as u8, 9, a_to_b);

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

    let swap_params = orca::swap::SwapParams {
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

    let swap_ix = orca::swap::swap(swap_params);

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

    Ok(())
}

pub async fn raydium_dca_target(
    config: &Config,
    client: &RpcClient,
    wallet: &Keypair,
    target: &Target,
    total_weight: u32,
    rent_exempt: u64,
    pool: PoolState,
) -> anyhow::Result<()> {
    let pool_address = target
        .raydium_pool
        .context(format!("{}: no raydium pool configured", target.symbol))?;

    let sol_is_mint0 = pool.token_mint0.to_bytes() == spl_token::native_mint::id().to_bytes();

    let price = sqrt_price_to_price(pool.sqrt_price_x64, target.decimals as u8, 9, sol_is_mint0);

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

    let swap_ix = raydium::swap::swap(swap_params);

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

    Ok(())
}
