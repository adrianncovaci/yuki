use anyhow::{Context, bail};
use solana_rpc_client::{
    api::response::transaction::Transaction, nonblocking::rpc_client::RpcClient,
};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use spl_associated_token_account_interface::{
    address::get_associated_token_address, instruction::create_associated_token_account_idempotent,
};

use crate::config::Target;

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
