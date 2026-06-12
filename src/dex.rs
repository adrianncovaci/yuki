use meteora_sol::accounts::LbPair;
use orca_whirlpools_client::Whirlpool;
use raydium_clmm::accounts::PoolState;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{message::Instruction, pubkey::Pubkey};

pub trait Dex {
    type Pool;
    type SwapParams;
    type QuoteParams;

    fn sqrt_price_to_price(sqrt_price: u128, decimals_0: u8, decimals_1: u8) -> f64 {
        let sqrt_price = sqrt_price as f64 / 2u128.pow(64) as f64;
        let price = sqrt_price * sqrt_price;
        price * 10f64.powi(decimals_0 as i32 - decimals_1 as i32)
    }

    fn fetch_pool_bytes(
        client: &RpcClient,
        pool_mint: Pubkey,
    ) -> impl Future<Output = Result<Self::Pool, anyhow::Error>> + Send;

    fn swap(params: Self::SwapParams) -> Instruction;

    fn quote(params: Self::QuoteParams) -> anyhow::Result<f64>;

    fn name() -> &'static str;
}

#[derive(strum::Display)]
pub enum DexPool {
    Orca { pool: Box<Whirlpool>, price: f64 },
    Raydium { pool: Box<PoolState>, price: f64 },
    Meteora { pool: Box<LbPair>, price: f64 },
}

impl DexPool {
    pub fn name(&self) -> String {
        self.to_string()
    }

    pub fn get_price(&self) -> f64 {
        match self {
            DexPool::Orca { price, .. } => *price,
            DexPool::Raydium { price, .. } => *price,
            DexPool::Meteora { price, .. } => *price,
        }
    }

    pub fn sol_price(&self) -> f64 {
        match self {
            DexPool::Orca { price, .. } => {
                if self.is_sol_mint0() {
                    1.0 / price
                } else {
                    *price
                }
            }
            DexPool::Raydium { price, .. } => {
                if self.is_sol_mint0() {
                    1.0 / price
                } else {
                    *price
                }
            }
            DexPool::Meteora { price, .. } => {
                if self.is_sol_mint0() {
                    1.0 / price
                } else {
                    *price
                }
            }
        }
    }

    pub fn is_sol_mint0(&self) -> bool {
        match self {
            DexPool::Orca { pool, .. } => pool.token_mint_a == spl_token::native_mint::ID,
            DexPool::Raydium { pool, .. } => {
                pool.token_mint0.to_bytes() == spl_token::native_mint::id().to_bytes()
            }
            DexPool::Meteora { pool, .. } => {
                pool.token_x_mint.to_bytes() == spl_token::native_mint::id().to_bytes()
            }
        }
    }
}
