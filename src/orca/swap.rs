use orca_whirlpools_client::{SwapV2Builder, Whirlpool};
use orca_whirlpools_core::{MAX_SQRT_PRICE, MIN_SQRT_PRICE};
use solana_sdk::{message::Instruction, pubkey::Pubkey};

pub struct SwapParams {
    pub whirlpool: Whirlpool,
    pub pool_address: Pubkey,
    pub amount: u64,
    pub slippage_bps: u32,
    pub b_token_decimals: u32,
    pub b_token_price: f64,
    pub authority: Pubkey,
    pub wsol_ata: Pubkey,
    pub dca_token_ata: Pubkey,
    pub tick_arrays: [Pubkey; 3],
    pub oracle: Pubkey,
    pub amount_specified_is_input: bool,
}

pub fn swap(params: SwapParams) -> Instruction {
    let SwapParams {
        whirlpool,
        pool_address,
        amount,
        slippage_bps,
        b_token_decimals,
        b_token_price,
        authority,
        wsol_ata,
        dca_token_ata,
        tick_arrays,
        oracle,
        amount_specified_is_input,
    } = params;

    let amount_sol = amount as f64 / 1e9;

    let a_to_b = whirlpool.token_mint_a == spl_token::native_mint::ID;

    let (a_token_ata, b_token_ata, expected_price) = if a_to_b {
        let expected_price =
            (amount_sol * b_token_price * 10f64.powi(b_token_decimals as i32)) as u64;
        (wsol_ata, dca_token_ata, expected_price)
    } else {
        let expected_price =
            (amount_sol / b_token_price * 10f64.powi(b_token_decimals as i32)) as u64;
        (dca_token_ata, wsol_ata, expected_price)
    };

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
        .whirlpool(pool_address)
        .token_owner_account_a(a_token_ata)
        .token_owner_account_b(b_token_ata)
        .token_vault_a(whirlpool.token_vault_a)
        .token_vault_b(whirlpool.token_vault_b)
        .tick_array0(tick_arrays[0])
        .tick_array1(tick_arrays[1])
        .tick_array2(tick_arrays[2])
        .oracle(oracle)
        .sqrt_price_limit(sqrt_price)
        .instruction()
}
