use orca_whirlpools_client::{SwapV2Builder, Whirlpool};
use solana_sdk::{message::Instruction, pubkey::Pubkey};

pub fn swap(
    whirlpool: Whirlpool,
    amount: u64,
    slippage_bps: u32,
    b_token_decimals: u32,
    b_token_price: f64,
    authority: Pubkey,
    whirlpool_config: Pubkey,
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
        .whirlpool(whirlpool_config)
        .instruction()
}
