use orca_whirlpools_client::{SwapV2Builder, Whirlpool};
use orca_whirlpools_core::{MAX_SQRT_PRICE, MIN_SQRT_PRICE};
use solana_sdk::{message::Instruction, pubkey::Pubkey};

#[allow(clippy::too_many_arguments)]
pub fn swap(
    whirlpool: Whirlpool,
    amount: u64,
    slippage_bps: u32,
    b_token_decimals: u32,
    b_token_price: f64,
    authority: Pubkey,
    whirlpool_pool: Pubkey,
    a_token_vault: Pubkey,
    b_token_vault: Pubkey,
    tick_array0: Pubkey,
    tick_array1: Pubkey,
    tick_array2: Pubkey,
    a_token_ata: Pubkey,
    b_token_ata: Pubkey,
    oracle: Pubkey,
    a_to_b: bool,
    amount_specified_is_input: bool,
) -> Instruction {
    let amount_sol = amount as f64 / 1e9;

    let expected_price = (amount_sol * b_token_price * 10f64.powi(b_token_decimals as i32)) as u64;
    let other_amount_threshold =
        (expected_price as f64 * (10_000 - slippage_bps) as f64 / 10_000.0) as u64;

    let sqrt_price = if a_to_b {
        MIN_SQRT_PRICE
    } else {
        MAX_SQRT_PRICE
    };

    SwapV2Builder::new()
        .amount(amount)
        .token_program_a(spl_token::id())
        .token_program_b(spl_token::id())
        .token_mint_a(whirlpool.token_mint_a)
        .token_mint_b(whirlpool.token_mint_b)
        .a_to_b(a_to_b)
        .amount_specified_is_input(amount_specified_is_input)
        .other_amount_threshold(other_amount_threshold)
        .token_authority(authority)
        .whirlpool(whirlpool_pool)
        .token_owner_account_a(a_token_ata)
        .token_owner_account_b(b_token_ata)
        .token_vault_a(a_token_vault)
        .token_vault_b(b_token_vault)
        .tick_array0(tick_array0)
        .tick_array1(tick_array1)
        .tick_array2(tick_array2)
        .oracle(oracle)
        .sqrt_price_limit(sqrt_price)
        .instruction()
}
