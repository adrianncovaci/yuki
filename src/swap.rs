use orca_whirlpools_client::{SwapV2Builder, Whirlpool};
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
) -> Instruction {
    let expected_price = amount as f64 * b_token_price * b_token_decimals as f64;
    let other_amount_threshold = expected_price * ((10_000 - slippage_bps) / 10_000) as f64;
    let other_amount_threshold = other_amount_threshold as u64;

    SwapV2Builder::new()
        .amount(amount)
        .token_program_a(spl_token::id())
        .token_program_b(spl_token::id())
        .token_mint_a(whirlpool.token_mint_a)
        .token_mint_b(whirlpool.token_mint_b)
        .a_to_b(true)
        .amount_specified_is_input(true)
        .other_amount_threshold(other_amount_threshold)
        .token_authority(authority)
        .whirlpool(whirlpool_pool)
        .token_owner_account_a(authority)
        .token_owner_account_b(authority)
        .token_vault_a(a_token_vault)
        .token_vault_b(b_token_vault)
        .tick_array0(tick_array0)
        .tick_array1(tick_array1)
        .tick_array2(tick_array2)
        .instruction()
}
