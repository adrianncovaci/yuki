use solana_sdk::pubkey;
use solana_sdk::pubkey::Pubkey;

pub const RAYDIUM_CLMM_PROGRAM: Pubkey = pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");

pub mod dca;
pub mod dex;
pub mod errors;
pub mod quote;
pub mod swap;
pub mod tick;
