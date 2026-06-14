use anyhow::Context;
use meteora_sol::accounts::{BinArray, LbPair};
use meteora_sol::instructions::Swap2Builder;
use meteora_sol::types::RemainingAccountsInfo;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;

use crate::dex::Dex;
use crate::meteora::dlmm::quote::estimate_output;
use crate::meteora::dlmm::{MEMO_PROGRAM, METEORA_DLMM_PROGRAM, to_meteora_pubkey};

pub struct MeteoraDlmmDex;

pub struct SwapParams {
    pub signer: Pubkey,
    pub pool_address: Pubkey,
    pub pair: LbPair,
    pub user_token_in: Pubkey,
    pub user_token_out: Pubkey,
    pub amount: u64,
    pub slippage_bps: u32,
    pub estimate_output: f64,
    pub bin_arrays: Vec<Pubkey>,
}

pub struct QuoteParams {
    pub amount_in: u64,
    pub pair: LbPair,
    pub bin_arrays: Vec<(Pubkey, BinArray)>,
    pub timestamp: i64,
    pub swap_for_y: bool,
}

impl Dex for MeteoraDlmmDex {
    type Pool = LbPair;

    type SwapParams = SwapParams;

    type QuoteParams = QuoteParams;

    async fn fetch_pool_bytes(
        client: &RpcClient,
        pool_mint: Pubkey,
    ) -> Result<Self::Pool, anyhow::Error> {
        let data = client
            .get_account_data(&pool_mint)
            .await
            .context("Failed to get lb pair account data")?;
        LbPair::from_bytes(&data).context("Failed to deserialize lb pair")
    }

    fn swap(params: Self::SwapParams) -> Instruction {
        let SwapParams {
            signer,
            pool_address,
            pair,
            user_token_in,
            user_token_out,
            amount,
            slippage_bps,
            estimate_output,
            bin_arrays,
        } = params;

        let min_amount_out = (estimate_output * (10_000 - slippage_bps) as f64 / 10_000.0) as u64;

        let (event_authority, _) =
            Pubkey::find_program_address(&[b"__event_authority"], &METEORA_DLMM_PROGRAM);

        let ix = Swap2Builder::new()
            .lb_pair(to_meteora_pubkey(&pool_address))
            .bin_array_bitmap_extension(None)
            .reserve_x(pair.reserve_x)
            .reserve_y(pair.reserve_y)
            .user_token_in(to_meteora_pubkey(&user_token_in))
            .user_token_out(to_meteora_pubkey(&user_token_out))
            .token_x_mint(pair.token_x_mint)
            .token_y_mint(pair.token_y_mint)
            .oracle(pair.oracle)
            .host_fee_in(None)
            .user(to_meteora_pubkey(&signer))
            .token_x_program(to_meteora_pubkey(&spl_token::id()))
            .token_y_program(to_meteora_pubkey(&spl_token::id()))
            .memo_program(to_meteora_pubkey(&MEMO_PROGRAM))
            .event_authority(to_meteora_pubkey(&event_authority))
            .program(to_meteora_pubkey(&METEORA_DLMM_PROGRAM))
            .amount_in(amount)
            .min_amount_out(min_amount_out)
            .remaining_accounts_info(RemainingAccountsInfo { slices: vec![] })
            .instruction();

        let mut accounts: Vec<AccountMeta> = ix
            .accounts
            .iter()
            .map(|m| AccountMeta {
                pubkey: Pubkey::new_from_array(m.pubkey.to_bytes()),
                is_signer: m.is_signer,
                is_writable: m.is_writable,
            })
            .collect();
        accounts.extend(bin_arrays.iter().map(|p| AccountMeta::new(*p, false)));

        Instruction {
            program_id: Pubkey::new_from_array(ix.program_id.to_bytes()),
            accounts,
            data: ix.data,
        }
    }

    fn quote(params: Self::QuoteParams) -> anyhow::Result<f64> {
        estimate_output(
            params.amount_in,
            &params.pair,
            &params.bin_arrays,
            params.timestamp,
            params.swap_for_y,
        )
    }

    fn name() -> &'static str {
        "Meteora"
    }
}
