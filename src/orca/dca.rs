use anyhow::Context;
use orca_whirlpools_client::{Whirlpool, get_oracle_address};
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
    orca::{
        dex::{OrcaDex, SwapParams},
        tick::TickArray,
    },
    transfer::transfer_to_cold_wallet,
    wsol::WsolSwap,
};

#[allow(clippy::too_many_arguments)]
pub async fn orca_dca_target(
    config: &Config,
    client: &RpcClient,
    wallet: &Keypair,
    target: &Target,
    total_weight: u32,
    rent_exempt: u64,
    whirlpool: Box<Whirlpool>,
    cold_wallet: Option<Pubkey>,
) -> anyhow::Result<SwapResult> {
    let pool_address = target
        .orca_pool
        .context(format!("{}: no orca pool configured", target.symbol))?;

    let (oracle, _) = get_oracle_address(&pool_address)?;

    let amount = (config.swap.amount_sol
        * LAMPORTS_PER_SOL as f64
        * (target.weight as f64 / total_weight as f64)) as u64;

    let sol_is_mint0 = whirlpool.token_mint_a == spl_token::native_mint::ID;
    let sqrt_price = whirlpool.sqrt_price as f64 / (1u128 << 64) as f64;
    let l = whirlpool.liquidity as f64;

    let estimated_output = if sol_is_mint0 {
        let p2 = (l * sqrt_price) / (l + amount as f64 * sqrt_price);
        l * (sqrt_price - p2)
    } else {
        let p2 = sqrt_price + (amount as f64 / l);
        l * (1f64 / sqrt_price - 1f64 / p2)
    };

    let price = OrcaDex::sqrt_price_to_price(whirlpool.sqrt_price, target.decimals as u8, 9);

    let recent_blockhash = client
        .get_latest_blockhash()
        .await
        .context("Failed to get latest blockhash")?;

    let ata = get_associated_token_address(&wallet.pubkey(), &target.mint_address);
    let wsol_swap = WsolSwap::new(&wallet.pubkey(), amount, rent_exempt)?;

    let tick_array = TickArray::new(
        pool_address,
        whirlpool.tick_current_index,
        whirlpool.tick_spacing,
        whirlpool.token_mint_a == spl_token::native_mint::ID,
    )?;

    let swap_params = SwapParams {
        whirlpool: *whirlpool,
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
        "{} | Orca | {:.4} SOL | tx: {signature}",
        target.symbol,
        amount as f64 / 1e9
    );

    transfer_to_cold_wallet(client, wallet, target, ata, cold_wallet).await?;

    Ok(SwapResult {
        symbol: target.symbol.clone(),
        amount,
        dex: OrcaDex::name().to_string(),
        estimated_output,
    })
}
