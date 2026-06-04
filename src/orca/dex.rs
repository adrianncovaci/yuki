use anyhow::Context;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;

use orca_whirlpools_client::{SwapV2Builder, Whirlpool};
use orca_whirlpools_core::{
    MAX_SQRT_PRICE, MIN_SQRT_PRICE, OracleFacade, TickArrays, TransferFee, WhirlpoolFacade,
};
use solana_sdk::{message::Instruction, pubkey::Pubkey};

use crate::dex::Dex;

pub struct SwapParams {
    pub whirlpool: Whirlpool,
    pub pool_address: Pubkey,
    pub amount: u64,
    pub other_amount_threshold: u64,
    pub authority: Pubkey,
    pub wsol_ata: Pubkey,
    pub dca_token_ata: Pubkey,
    pub tick_arrays: [Pubkey; 3],
    pub oracle: Pubkey,
    pub amount_specified_is_input: bool,
}

pub struct QuoteParams {
    pub token_in: u64,
    pub specified_token_a: bool,
    pub slippage_tolerance_bps: u16,
    pub whirlpool: WhirlpoolFacade,
    pub oracle: Option<OracleFacade>,
    pub tick_arrays: TickArrays,
    pub timestamp: u64,
    pub transfer_fee_a: Option<TransferFee>,
    pub transfer_fee_b: Option<TransferFee>,
}

pub struct OrcaDex {}

impl Dex for OrcaDex {
    type Pool = Whirlpool;

    type SwapParams = SwapParams;

    type QuoteParams = QuoteParams;

    async fn fetch_pool_bytes(
        client: &RpcClient,
        pool_mint: Pubkey,
    ) -> Result<Whirlpool, anyhow::Error> {
        let address = client
            .get_account_data(&pool_mint)
            .await
            .context("Failed to get account data")?;

        Whirlpool::from_bytes(&address).context("Failed to extract whirlpool account")
    }

    fn swap(params: SwapParams) -> Instruction {
        let SwapParams {
            whirlpool,
            pool_address,
            amount,
            other_amount_threshold,
            authority,
            wsol_ata,
            dca_token_ata,
            tick_arrays,
            oracle,
            amount_specified_is_input,
        } = params;

        let a_to_b = whirlpool.token_mint_a == spl_token::native_mint::ID;

        let (a_token_ata, b_token_ata) = if a_to_b {
            (wsol_ata, dca_token_ata)
        } else {
            (dca_token_ata, wsol_ata)
        };

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

    fn name() -> &'static str {
        "Orca"
    }

    fn quote(params: QuoteParams) -> anyhow::Result<f64> {
        orca_whirlpools_core::swap_quote_by_input_token(
            params.token_in,
            params.specified_token_a,
            params.slippage_tolerance_bps,
            params.whirlpool,
            params.oracle,
            params.tick_arrays,
            params.timestamp,
            params.transfer_fee_a,
            params.transfer_fee_b,
        )
        .map(|res| res.token_min_out as f64)
        .map_err(|err| anyhow::anyhow!("Failed to fetch swap quote: {err:?}"))
    }
}
