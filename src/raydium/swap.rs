use borsh::BorshSerialize;
use raydium_clmm::accounts::PoolState;
use solana_sdk::pubkey::Pubkey;

pub(crate) const SWAP_DISCRIMINATOR: [u8; 8] = [43, 4, 237, 11, 26, 201, 30, 98];
pub(crate) const TOKEN_PROGRAM_2022: Pubkey =
    solana_sdk::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub(crate) const MEMO_PROGRAM: Pubkey =
    solana_sdk::pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

pub(crate) const MIN_SQRT_PRICE_X64: u128 = 4295048016 + 1;
pub(crate) const MAX_SQRT_PRICE_X64: u128 = 79226673521066979257578248091 - 1;

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
