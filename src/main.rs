pub mod config;
pub mod orca;
pub mod wsol;

use orca_whirlpools_client::get_oracle_address;
use orca_whirlpools_core::sqrt_price_to_price;
use solana_sdk::{native_token::LAMPORTS_PER_SOL, signature::read_keypair_file, signer::Signer};
use spl_associated_token_account_interface::{
    address::get_associated_token_address, instruction::create_associated_token_account_idempotent,
};
use std::time::Duration;
use tokio::fs::read_to_string;

use anyhow::Context;

use solana_rpc_client::{
    api::{config::CommitmentConfig, response::transaction::Transaction},
    nonblocking::rpc_client::RpcClient,
};

use crate::{
    config::Config,
    orca::{
        pool::fetch_pool_bytes,
        swap::{SwapParams, swap},
        tick::TickArray,
    },
    wsol::WsolSwap,
};

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

    let usdc_orca_pool = config
        .targets
        .iter()
        .find(|x| x.symbol.as_str() == "USDC")
        .context("Failed to find usdc target")?;

    let usdc_whirlpool = fetch_pool_bytes(&client, usdc_orca_pool.orca_pool).await?;

    let (oracle, _) = get_oracle_address(&usdc_orca_pool.orca_pool)?;

    let usdc_price =
        sqrt_price_to_price(usdc_whirlpool.sqrt_price, usdc_orca_pool.decimals as u8, 9);

    tracing::info!("usdc price = {usdc_price:#?}");

    let recent_blockhash = client
        .get_latest_blockhash()
        .await
        .context("Failed to get latest blockhash")?;

    let rent_exempt = client
        .get_minimum_balance_for_rent_exemption(165)
        .await
        .context("Failed to fetch minimum balance for rent exemption")?;

    let amount = LAMPORTS_PER_SOL / 100;

    let usdc_ata = get_associated_token_address(&dca_wallet.pubkey(), &usdc_orca_pool.mint_address);
    let wsol_swap = WsolSwap::new(&dca_wallet.pubkey(), amount, rent_exempt)?;

    let tick_array = TickArray::new(
        usdc_orca_pool.orca_pool,
        usdc_whirlpool.tick_current_index,
        usdc_whirlpool.tick_spacing,
    )?;

    let swap_params = SwapParams {
        whirlpool: usdc_whirlpool,
        pool_address: usdc_orca_pool.orca_pool,
        amount,
        slippage_bps: 100,
        b_token_decimals: usdc_orca_pool.decimals,
        b_token_price: usdc_price,
        authority: dca_wallet.pubkey(),
        a_token_ata: wsol_swap.keypair.pubkey(),
        b_token_ata: usdc_ata,
        tick_arrays: tick_array.inner,
        oracle,
        a_to_b: true,
        amount_specified_is_input: true,
    };

    let swap_ix = swap(swap_params);

    let tx = Transaction::new_signed_with_payer(
        &[
            wsol_swap.create_ix,
            wsol_swap.init_ix,
            wsol_swap.sync_ix,
            create_associated_token_account_idempotent(
                &dca_wallet.pubkey(),
                &dca_wallet.pubkey(),
                &usdc_orca_pool.mint_address,
                &spl_token::id(),
            ),
            swap_ix,
            wsol_swap.close_ix,
        ],
        Some(&dca_wallet.pubkey()),
        &[&dca_wallet, &wsol_swap.keypair],
        recent_blockhash,
    );

    let signature = client.send_and_confirm_transaction(&tx).await?;

    tracing::info!("{signature:#?}");

    Ok(())
}
