pub mod config;
pub mod pool;
pub mod swap;
pub mod wsol;

use orca_whirlpools_client::{get_oracle_address, get_tick_array_address};
use orca_whirlpools_core::{TICK_ARRAY_SIZE, get_tick_array_start_tick_index, sqrt_price_to_price};
use solana_sdk::{
    native_token::LAMPORTS_PER_SOL,
    signature::{Keypair, read_keypair_file},
    signer::Signer,
};
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

use crate::{config::Config, pool::fetch_pool_bytes};

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

    let balance = client.get_balance(&dca_wallet.pubkey()).await;

    tracing::info!("balance = {balance:?}, pubkey = {:?}", dca_wallet.pubkey());

    let jto_orca_pool = config
        .targets
        .iter()
        .find(|x| x.symbol.as_str() == "JTO")
        .context("Failed to find jto target")?;

    let jto_whirlpool = fetch_pool_bytes(&client, jto_orca_pool.orca_pool).await?;

    let jto_price = sqrt_price_to_price(jto_whirlpool.sqrt_price, jto_orca_pool.decimals as u8, 9);

    tracing::info!("jito price = {jto_price:#?}");

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

    let a_vault = usdc_whirlpool.token_vault_a;
    let b_vault = usdc_whirlpool.token_vault_b;

    let ticks_in_array = usdc_whirlpool.tick_spacing as i32 * TICK_ARRAY_SIZE as i32;

    let tick_arr0_idx = get_tick_array_start_tick_index(
        usdc_whirlpool.tick_current_index,
        usdc_whirlpool.tick_spacing,
    );

    let tick_arr1_idx = tick_arr0_idx - ticks_in_array;

    let tick_arr2_idx = tick_arr1_idx - ticks_in_array;

    let tick_arr0 = get_tick_array_address(&usdc_orca_pool.orca_pool, tick_arr0_idx)
        .context("Failed to get tick array address")?;
    let tick_arr1 = get_tick_array_address(&usdc_orca_pool.orca_pool, tick_arr1_idx)
        .context("Failed to get tick array address")?;
    let tick_arr2 = get_tick_array_address(&usdc_orca_pool.orca_pool, tick_arr2_idx)
        .context("Failed to get tick array address")?;

    let wsol_keypair = Keypair::new();
    let amount = LAMPORTS_PER_SOL / 100;

    let create_wsol_ix = solana_system_interface::instruction::create_account(
        &dca_wallet.pubkey(),
        &wsol_keypair.pubkey(),
        amount
            + client
                .get_minimum_balance_for_rent_exemption(165)
                .await
                .context("Failed to fetch minimum balance for rent exemption")?,
        165,
        &spl_token::id(),
    );

    let initialize_wsol_acc_ix = spl_token::instruction::initialize_account(
        &spl_token::id(),
        &wsol_keypair.pubkey(),
        &spl_token::native_mint::id(),
        &dca_wallet.pubkey(),
    )
    .context("Failed to initialize wsol ix")?;

    let sync_native_ix =
        spl_token::instruction::sync_native(&spl_token::id(), &wsol_keypair.pubkey())
            .context("Failed to sync wsol ix")?;

    let usdc_ata = get_associated_token_address(&dca_wallet.pubkey(), &usdc_orca_pool.mint_address);

    let swap_ix = swap::swap(
        usdc_whirlpool,
        amount,
        100,
        usdc_orca_pool.decimals,
        usdc_price,
        dca_wallet.pubkey(),
        usdc_orca_pool.orca_pool,
        a_vault,
        b_vault,
        tick_arr0.0,
        tick_arr1.0,
        tick_arr2.0,
        wsol_keypair.pubkey(),
        usdc_ata,
        oracle,
        true,
        true,
    );

    let close_wsol_ix = spl_token::instruction::close_account(
        &spl_token::id(),
        &wsol_keypair.pubkey(),
        &dca_wallet.pubkey(),
        &dca_wallet.pubkey(),
        &[],
    )
    .context("Failed to close wsol ix")?;

    let tx = Transaction::new_signed_with_payer(
        &[
            create_wsol_ix,
            initialize_wsol_acc_ix,
            sync_native_ix,
            create_associated_token_account_idempotent(
                &dca_wallet.pubkey(),
                &dca_wallet.pubkey(),
                &usdc_orca_pool.mint_address,
                &spl_token::id(),
            ),
            swap_ix,
            close_wsol_ix,
        ],
        Some(&dca_wallet.pubkey()),
        &[&dca_wallet, &wsol_keypair],
        recent_blockhash,
    );

    let signature = client.send_and_confirm_transaction(&tx).await?;

    tracing::info!("{signature:#?}");

    Ok(())
}
