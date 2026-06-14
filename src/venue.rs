use anyhow::Context;
use meteora_sol::accounts::{BinArray, LbPair};
use orca_whirlpools_client::Whirlpool;
use orca_whirlpools_core::TickArrays;
use raydium_clmm::{
    accounts::{AmmConfig, PoolState},
    types::TickState,
};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

use crate::{
    dex::Dex,
    meteora::dlmm::{
        bin::BinArrays,
        bin_price_ui,
        dex::{MeteoraDlmmDex, QuoteParams as MeteoraQuote},
    },
    orca::{
        dex::{OrcaDex, QuoteParams as OrcaQuote},
        tick::TickArray as OrcaTicks,
    },
    raydium::{
        dex::RaydiumDex, quote::QuoteParams as RaydiumQuote, tick::TickArray as RaydiumTicks,
    },
};

#[derive(Clone)]
pub struct VenueMeta {
    pub address: Pubkey,
    pub sol_is_first: bool,
    pub decimals: u8,
    pub kind: VenueKind,
}

#[derive(Clone, Copy, strum::Display)]
pub enum VenueKind {
    Orca {
        tick_space: u16,
    },
    Raydium {
        tick_space: u16,
        trade_fee_rate: u32,
    },
    Meteora {
        bin_step: u16,
    },
}

impl VenueMeta {
    pub async fn orca(client: &RpcClient, address: Pubkey, decimals: u8) -> anyhow::Result<Self> {
        let pool = OrcaDex::fetch_pool_bytes(client, address).await?;
        Ok(Self {
            address,
            sol_is_first: pool.token_mint_a == spl_token::native_mint::ID,
            decimals,
            kind: VenueKind::Orca {
                tick_space: pool.tick_spacing,
            },
        })
    }

    pub async fn raydium(
        client: &RpcClient,
        address: Pubkey,
        decimals: u8,
    ) -> anyhow::Result<Self> {
        let pool = RaydiumDex::fetch_pool_bytes(client, address).await?;
        let amm = AmmConfig::from_bytes(
            &client
                .get_account_data(&Pubkey::new_from_array(pool.amm_config.to_bytes()))
                .await
                .context("fetch amm config")?,
        )
        .context("decode amm config")?;
        Ok(Self {
            address,
            sol_is_first: pool.token_mint0.to_bytes() == spl_token::native_mint::id().to_bytes(),
            decimals,
            kind: VenueKind::Raydium {
                tick_space: pool.tick_spacing,
                trade_fee_rate: amm.trade_fee_rate,
            },
        })
    }

    pub async fn meteora(
        client: &RpcClient,
        address: Pubkey,
        decimals: u8,
    ) -> anyhow::Result<Self> {
        let pool = MeteoraDlmmDex::fetch_pool_bytes(client, address).await?;
        Ok(Self {
            address,
            sol_is_first: pool.token_x_mint.to_bytes() == spl_token::native_mint::id().to_bytes(),
            decimals,
            kind: VenueKind::Meteora {
                bin_step: pool.bin_step,
            },
        })
    }
}

pub enum Snapshot {
    Orca {
        pool: Box<Whirlpool>,
        ticks_sol_to_token: Box<TickArrays>,
        ticks_token_to_sol: Box<TickArrays>,
    },
    Raydium {
        pool: Box<PoolState>,
        ticks_sol_to_token: Vec<TickState>,
        ticks_token_to_sol: Vec<TickState>,
    },
    Meteora {
        pool: Box<LbPair>,
        bins_sol_to_token: Vec<(Pubkey, BinArray)>,
        bins_token_to_sol: Vec<(Pubkey, BinArray)>,
    },
}

impl Snapshot {
    pub async fn fetch(client: &RpcClient, meta: &VenueMeta) -> anyhow::Result<Self> {
        let dir_fwd = meta.sol_is_first;
        let dir_rev = !meta.sol_is_first;

        Ok(match meta.kind {
            VenueKind::Orca { tick_space } => {
                let pool = OrcaDex::fetch_pool_bytes(client, meta.address).await?;
                let ticks_sol_to_token =
                    OrcaTicks::new(meta.address, pool.tick_current_index, tick_space, dir_fwd)?
                        .fetch_and_deserialize(client)
                        .await?;
                let ticks_token_to_sol =
                    OrcaTicks::new(meta.address, pool.tick_current_index, tick_space, dir_rev)?
                        .fetch_and_deserialize(client)
                        .await?;
                Snapshot::Orca {
                    pool: Box::new(pool),
                    ticks_sol_to_token: Box::new(ticks_sol_to_token),
                    ticks_token_to_sol: Box::new(ticks_token_to_sol),
                }
            }
            VenueKind::Raydium { tick_space, .. } => {
                let pool = RaydiumDex::fetch_pool_bytes(client, meta.address).await?;
                let ticks_sol_to_token =
                    RaydiumTicks::new(meta.address, pool.tick_current, tick_space, dir_fwd)
                        .fetch_and_deserialize(client, &pool, dir_fwd)
                        .await?;
                let ticks_token_to_sol =
                    RaydiumTicks::new(meta.address, pool.tick_current, tick_space, dir_rev)
                        .fetch_and_deserialize(client, &pool, dir_rev)
                        .await?;
                Snapshot::Raydium {
                    pool: Box::new(pool),
                    ticks_sol_to_token,
                    ticks_token_to_sol,
                }
            }
            VenueKind::Meteora { .. } => {
                let pool = MeteoraDlmmDex::fetch_pool_bytes(client, meta.address).await?;
                let bins_sol_to_token = BinArrays::new(meta.address, pool.active_id, dir_fwd)
                    .fetch_and_deserialize(client)
                    .await?;
                let bins_token_to_sol = BinArrays::new(meta.address, pool.active_id, dir_rev)
                    .fetch_and_deserialize(client)
                    .await?;
                Snapshot::Meteora {
                    pool: Box::new(pool),
                    bins_sol_to_token,
                    bins_token_to_sol,
                }
            }
        })
    }

    /// Quote a swap over this snapshot — PURE, no I/O. Uses the pre-fetched
    /// arrays for the leg's direction and calls the same `Dex::quote` the DCA
    /// bot uses. `input_is_sol = true` quotes SOL→token, `false` token→SOL.
    /// `now` is the unix timestamp (Meteora/Orca fee math depends on it).
    pub fn quote(
        &self,
        meta: &VenueMeta,
        amount_in: u64,
        input_is_sol: bool,
        now: u64,
    ) -> anyhow::Result<u64> {
        // Selling the FIRST token (mint0 / token_x / token_a)?
        let dir = input_is_sol == meta.sol_is_first;

        let out = match self {
            Snapshot::Raydium {
                pool,
                ticks_sol_to_token,
                ticks_token_to_sol,
            } => {
                let VenueKind::Raydium { trade_fee_rate, .. } = meta.kind else {
                    anyhow::bail!("snapshot/meta mismatch: Raydium snapshot, non-Raydium meta");
                };
                let net_in = amount_in as f64 * (1.0 - trade_fee_rate as f64 / 1_000_000.0);
                let ticks = if input_is_sol {
                    ticks_sol_to_token
                } else {
                    ticks_token_to_sol
                };
                RaydiumDex::quote(RaydiumQuote {
                    net_in,
                    ticks: ticks.clone(),
                    pool: (**pool).clone(),
                    swap_for_y: dir,
                })?
            }
            Snapshot::Meteora {
                pool,
                bins_sol_to_token,
                bins_token_to_sol,
            } => {
                let bins = if input_is_sol {
                    bins_sol_to_token
                } else {
                    bins_token_to_sol
                };
                MeteoraDlmmDex::quote(MeteoraQuote {
                    amount_in,
                    pair: (**pool).clone(),
                    bin_arrays: bins.clone(),
                    timestamp: now as i64,
                    swap_for_y: dir,
                })?
            }
            Snapshot::Orca {
                pool,
                ticks_sol_to_token,
                ticks_token_to_sol,
            } => {
                let tick_arrays = if input_is_sol {
                    ticks_sol_to_token
                } else {
                    ticks_token_to_sol
                };
                OrcaDex::quote(OrcaQuote {
                    token_in: amount_in,
                    specified_token_a: dir,
                    slippage_tolerance_bps: 0, // 0 = expected out, not min-out
                    whirlpool: (**pool).clone().into(),
                    oracle: None,
                    tick_arrays: (**tick_arrays).clone(),
                    timestamp: now,
                    transfer_fee_a: None,
                    transfer_fee_b: None,
                })?
            }
        };

        Ok(out as u64)
    }

    /// Mid price as SOL-per-token (lower = cheaper), from the live pool. Mirrors
    /// `DexPool::sol_price`: compute the first→second token price, invert if SOL
    /// is the first token.
    pub fn sol_price(&self, meta: &VenueMeta) -> f64 {
        let (dec0, dec1) = if meta.sol_is_first {
            (9u8, meta.decimals)
        } else {
            (meta.decimals, 9u8)
        };

        let price_0_in_1 = match self {
            Snapshot::Raydium { pool, .. } => {
                RaydiumDex::sqrt_price_to_price(pool.sqrt_price_x64, dec0, dec1)
            }
            Snapshot::Orca { pool, .. } => {
                OrcaDex::sqrt_price_to_price(pool.sqrt_price, dec0, dec1)
            }
            Snapshot::Meteora { pool, .. } => {
                let VenueKind::Meteora { bin_step } = meta.kind else {
                    return f64::NAN;
                };
                bin_price_ui(pool.active_id, bin_step, dec0, dec1)
            }
        };

        if meta.sol_is_first {
            1.0 / price_0_in_1
        } else {
            price_0_in_1
        }
    }
}
