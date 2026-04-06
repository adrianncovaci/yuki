pub mod config;
pub mod orca;
pub mod wsol;

use clap::Parser;
use orca_whirlpools_client::get_oracle_address;
use orca_whirlpools_core::sqrt_price_to_price;
use solana_sdk::{
    native_token::LAMPORTS_PER_SOL,
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

use crate::{
    config::{Config, Target},
    orca::{
        pool::fetch_pool_bytes,
        swap::{SwapParams, swap},
        tick::TickArray,
    },
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

        tracing::info!("DCA round starting | balance: {:.4} SOL", balance as f64 / 1e9);

        if (balance as f64) < needed {
            tracing::warn!(
                "Insufficient balance: {:.4} SOL < {:.4} SOL needed",
                balance as f64 / 1e9,
                needed / 1e9
            );
        } else {
            for target in &config.targets {
                if let Err(err) = dca_target(
                    &config,
                    &client,
                    &dca_wallet,
                    target,
                    total_weight,
                    rent_exempt,
                )
                .await
                {
                    tracing::error!("Failed to dca {target:?}: {err:?}");
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

pub async fn dca_target(
    config: &Config,
    client: &RpcClient,
    wallet: &Keypair,
    target: &Target,
    total_weight: u32,
    rent_exempt: u64,
) -> anyhow::Result<()> {
    let whirlpool = fetch_pool_bytes(client, target.orca_pool).await?;

    let (oracle, _) = get_oracle_address(&target.orca_pool)?;

    let price = sqrt_price_to_price(whirlpool.sqrt_price, target.decimals as u8, 9);

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

    let tick_array = TickArray::new(
        target.orca_pool,
        whirlpool.tick_current_index,
        whirlpool.tick_spacing,
        whirlpool.token_mint_a == spl_token::native_mint::ID,
    )?;

    let swap_params = SwapParams {
        whirlpool,
        pool_address: target.orca_pool,
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

    let swap_ix = swap(swap_params);

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

    tracing::info!("{} | swapped {:.4} SOL | tx: {signature}", target.symbol, amount as f64 / 1e9);

    Ok(())
}
