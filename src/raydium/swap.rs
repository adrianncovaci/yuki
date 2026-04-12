use borsh::BorshSerialize;
use raydium_clmm::accounts::PoolState;
use solana_sdk::{
    message::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::raydium::RAYDIUM_CLMM_PROGRAM;

const SWAP_DISCRIMINATOR: [u8; 8] = [43, 4, 237, 11, 26, 201, 30, 98];
const TOKEN_PROGRAM_2022: Pubkey =
    solana_sdk::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
const MEMO_PROGRAM: Pubkey = solana_sdk::pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

const MIN_SQRT_PRICE_X64: u128 = 4295048016 + 1;
const MAX_SQRT_PRICE_X64: u128 = 79226673521066979257578248091 - 1;

#[derive(BorshSerialize)]
pub struct SwapArgs {
    pub amount: u64,
    pub other_amount_threshold: u64,
    pub sqrt_price_limit_x64: u128,
    pub is_base_input: bool,
}

pub struct SwapParams {
    pub signer: Pubkey,
    pub pool_address: Pubkey,
    pub pool_state: PoolState,
    pub input_token_account: Pubkey,
    pub output_token_account: Pubkey,
    pub input_vault: Pubkey,
    pub output_vault: Pubkey,
    pub observation_state: Pubkey,
    pub input_vault_mint: Pubkey,
    pub output_vault_mint: Pubkey,
    pub amount: u64,
    pub slippage_bps: u32,
    pub token_decimals: u32,
    pub token_price: f64,
    pub tick_arrays: [Pubkey; 3],
}

/// Instruction builder for `SwapV2`.
///
/// ### Accounts:
///
///   0. `[signer]` payer
///   1. `[]` amm_config
///   2. `[writable]` pool_state
///   3. `[writable]` input_token_account
///   4. `[writable]` output_token_account
///   5. `[writable]` input_vault
///   6. `[writable]` output_vault
///   7. `[writable]` observation_state
///   8. `[optional]` token_program (default to `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`)
///   9. `[optional]` token_program2022 (default to `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb`)
///   10. `[optional]` memo_program (default to `MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr`)
///   11. `[]` input_vault_mint
///   12. `[]` output_vault_mint
pub fn swap(params: SwapParams) -> Instruction {
    let SwapParams {
        signer,
        pool_address,
        pool_state,
        input_token_account,
        output_token_account,
        input_vault,
        output_vault,
        observation_state,
        input_vault_mint,
        output_vault_mint,
        amount,
        slippage_bps,
        token_decimals,
        token_price,
        tick_arrays,
    } = params;

    let amm_config = Pubkey::new_from_array(pool_state.amm_config.to_bytes());
    let a_to_b = pool_state.token_mint0.to_bytes() == spl_token::native_mint::id().to_bytes();

    let amount_sol = amount as f64 / 1e9;

    let expected_output = if a_to_b {
        (amount_sol * token_price * 10f64.powi(token_decimals as i32)) as u64
    } else {
        (amount_sol / token_price * 10f64.powi(token_decimals as i32)) as u64
    };

    let other_amount_threshold =
        (expected_output as f64 * (10_000 - slippage_bps) as f64 / 10_000.0) as u64;

    let sqrt_price_limit_x64 = if a_to_b {
        MIN_SQRT_PRICE_X64
    } else {
        MAX_SQRT_PRICE_X64
    };

    let args = SwapArgs {
        amount,
        other_amount_threshold,
        sqrt_price_limit_x64,
        is_base_input: true,
    };

    let mut data = SWAP_DISCRIMINATOR.to_vec();
    data.extend(borsh::to_vec(&args).unwrap());

    let mut accounts = vec![
        AccountMeta::new_readonly(signer, true),
        AccountMeta::new_readonly(amm_config, false),
        AccountMeta::new(pool_address, false),
        AccountMeta::new(input_token_account, false),
        AccountMeta::new(output_token_account, false),
        AccountMeta::new(input_vault, false),
        AccountMeta::new(output_vault, false),
        AccountMeta::new(observation_state, false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(TOKEN_PROGRAM_2022, false),
        AccountMeta::new_readonly(MEMO_PROGRAM, false),
        AccountMeta::new_readonly(input_vault_mint, false),
        AccountMeta::new_readonly(output_vault_mint, false),
    ];

    for tick_array in &tick_arrays {
        accounts.push(AccountMeta::new(*tick_array, false));
    }

    Instruction {
        program_id: RAYDIUM_CLMM_PROGRAM,
        accounts,
        data,
    }
}
