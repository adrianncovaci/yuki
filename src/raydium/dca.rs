use anyhow::Context;
use raydium_clmm::accounts::PoolState;
use solana_rpc_client::{
    api::response::transaction::Transaction, nonblocking::rpc_client::RpcClient,
};
use solana_sdk::{
    native_token::LAMPORTS_PER_SOL, pubkey::Pubkey, signature::Keypair, signer::Signer,
};
use spl_associated_token_account_interface::{
    address::get_associated_token_address, instruction::create_associated_token_account_idempotent,
};

use crate::{
    config::{Config, Target},
    dex::Dex,
    dto::SwapResult,
    raydium::{dex::RaydiumDex, swap::SwapParams, tick::TickArray},
    transfer::transfer_to_cold_wallet,
    wsol::WsolSwap,
};

#[allow(clippy::too_many_arguments)]
pub async fn raydium_dca_target(
    config: &Config,
    client: &RpcClient,
    wallet: &Keypair,
    target: &Target,
    total_weight: u32,
    rent_exempt: u64,
    pool: Box<PoolState>,
    cold_wallet: Option<Pubkey>,
) -> anyhow::Result<SwapResult> {
    let pool_address = target
        .raydium_pool
        .context(format!("{}: no raydium pool configured", target.symbol))?;

    let sol_is_mint0 = pool.token_mint0.to_bytes() == spl_token::native_mint::id().to_bytes();

    let amount = (config.swap.amount_sol
        * LAMPORTS_PER_SOL as f64
        * (target.weight as f64 / total_weight as f64)) as u64;

    let x_v = pool.liquidity as f64 / pool.sqrt_price_x64 as f64;
    let y_v = pool.liquidity as f64 * pool.sqrt_price_x64 as f64;
    let k = x_v * y_v;

    let price = RaydiumDex::sqrt_price_to_price(pool.sqrt_price_x64, target.decimals as u8, 9);

    tracing::info!(
        "{:?} price: {price:?}; liquidity: {:?}; Xv = {:?}; Yv = {:?}; k = {k:?}",
        target.symbol,
        pool.liquidity,
        x_v,
        y_v
    );

    let recent_blockhash = client
        .get_latest_blockhash()
        .await
        .context("Failed to get latest blockhash")?;

    let ata = get_associated_token_address(&wallet.pubkey(), &target.mint_address);
    let wsol_swap = WsolSwap::new(&wallet.pubkey(), amount, rent_exempt)?;

    let tick_array = TickArray::new(
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

    let swap_params = SwapParams {
        signer: wallet.pubkey(),
        pool_address,
        observation_state: Pubkey::new_from_array(pool.observation_key.to_bytes()),
        pool_state: *pool,
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
        "{} | Raydium | {:.4} SOL | tx: {signature}",
        target.symbol,
        amount as f64 / 1e9
    );

    transfer_to_cold_wallet(client, wallet, target, ata, cold_wallet).await?;

    Ok(SwapResult {
        symbol: target.symbol.clone(),
        amount,
        dex: RaydiumDex::name().to_string(),
    })
}
