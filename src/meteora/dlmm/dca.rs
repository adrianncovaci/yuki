use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use meteora_sol::accounts::LbPair;
use solana_rpc_client::{
    api::response::transaction::Transaction, nonblocking::rpc_client::RpcClient,
};
use solana_sdk::{
    instruction::InstructionError, native_token::LAMPORTS_PER_SOL, pubkey::Pubkey,
    signature::Keypair, signer::Signer, transaction::TransactionError,
};
use spl_associated_token_account_interface::{
    address::get_associated_token_address, instruction::create_associated_token_account_idempotent,
};

use crate::{
    config::{Config, Target},
    dex::Dex,
    dto::SwapResult,
    meteora::dlmm::{
        bin::BinArrays,
        dex::{MeteoraDlmmDex, QuoteParams, SwapParams},
    },
    transfer::transfer_to_cold_wallet,
    wsol::WsolSwap,
};

#[allow(clippy::too_many_arguments)]
pub async fn meteora_dca_target(
    config: &Config,
    client: &RpcClient,
    wallet: &Keypair,
    target: &Target,
    total_weight: u32,
    rent_exempt: u64,
    pool: Box<LbPair>,
    cold_wallet: Option<Pubkey>,
) -> anyhow::Result<SwapResult> {
    let pool_address = target
        .meteora_dlmm_pool
        .context(format!("{}: no meteora pool configured", target.symbol))?;

    if pool.status != 0 {
        anyhow::bail!(
            "{}: meteora pool not enabled (status {})",
            target.symbol,
            pool.status
        );
    }
    if pool.token_mint_x_program_flag != 0 || pool.token_mint_y_program_flag != 0 {
        anyhow::bail!("{}: meteora token-2022 pool not supported", target.symbol);
    }

    let swap_for_y = pool.token_x_mint.to_bytes() == spl_token::native_mint::id().to_bytes();

    let amount = (config.swap.amount_sol
        * LAMPORTS_PER_SOL as f64
        * (target.weight as f64 / total_weight as f64)) as u64;

    let bin_arrays = BinArrays::new(pool_address, pool.active_id, swap_for_y)
        .fetch_and_deserialize(client)
        .await?;
    let bin_array_keys: Vec<Pubkey> = bin_arrays.iter().map(|(pubkey, _)| *pubkey).collect();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before epoch")?
        .as_secs() as i64;

    // DLMM fees are bin-dependent, so `amount` is the GROSS, fee-inclusive input (no AMM-config fee strip like Raydium).
    let estimated_output_raw = MeteoraDlmmDex::quote(QuoteParams {
        amount_in: amount,
        pair: *pool.clone(),
        bin_arrays,
        timestamp: now,
    })?;
    let estimated_output = estimated_output_raw / 10f64.powi(target.decimals as i32);

    let recent_blockhash = client
        .get_latest_blockhash()
        .await
        .context("Failed to get latest blockhash")?;

    let ata = get_associated_token_address(&wallet.pubkey(), &target.mint_address);
    let wsol_swap = WsolSwap::new(&wallet.pubkey(), amount, rent_exempt)?;

    let diag = format!(
        "active_id={}, bin_step={}, swap_for_y={}, bin_arrays={:?}",
        pool.active_id, pool.bin_step, swap_for_y, bin_array_keys,
    );

    let swap_params = SwapParams {
        signer: wallet.pubkey(),
        pool_address,
        pair: *pool,
        user_token_in: wsol_swap.keypair.pubkey(),
        user_token_out: ata,
        amount,
        slippage_bps: config.swap.slippage_bps,
        estimate_output: estimated_output_raw,
        bin_arrays: bin_array_keys,
    };

    let swap_ix = MeteoraDlmmDex::swap(swap_params);

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

    let signature = match client.send_and_confirm_transaction(&tx).await {
        Ok(signature) => signature,
        Err(err) => {
            let decoded = err.get_transaction_error().and_then(|te| match te {
                TransactionError::InstructionError(_, InstructionError::Custom(code)) => {
                    Some((code, crate::meteora::dlmm::errors::decode(code)))
                }
                _ => None,
            });
            let prefix = match decoded {
                Some((code, Some(msg))) => format!("meteora program error {code}: {msg} — "),
                Some((code, None)) => format!("meteora program error {code} (unknown) — "),
                None => String::new(),
            };
            return Err(
                anyhow::Error::new(err).context(format!("{prefix}meteora swap failed [{diag}]"))
            );
        }
    };

    tracing::info!(
        "{} | Meteora | {:.4} SOL | tx: {signature}",
        target.symbol,
        amount as f64 / 1e9
    );

    transfer_to_cold_wallet(client, wallet, target, ata, cold_wallet).await?;

    Ok(SwapResult {
        symbol: target.symbol.clone(),
        amount,
        dex: MeteoraDlmmDex::name().to_string(),
        estimated_output,
        signature: signature.to_string(),
    })
}
