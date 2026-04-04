use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, de::Deserializer};

use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub rpc: Rpc,
    pub wallet: Wallet,
    pub swap: Swap,
    pub targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
pub struct Rpc {
    pub timeout_secs: u64,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct Swap {
    pub slippage_bps: u32,
    pub amount_sol: f64,
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
pub struct Wallet {
    pub keypair_path: String,
    pub reserve_sol: f64,
}

#[derive(Debug, Deserialize)]
pub struct Target {
    pub symbol: String,
    #[serde(deserialize_with = "deserialize_pubkey")]
    pub mint_address: Pubkey,
    pub weight: u32,
    #[serde(deserialize_with = "deserialize_pubkey")]
    pub orca_pool: Pubkey,
    pub decimals: u32,
}

fn deserialize_pubkey<'de, D>(de: D) -> Result<Pubkey, D::Error>
where
    D: Deserializer<'de>,
{
    let pubkey = String::deserialize(de)?;
    Pubkey::from_str(&pubkey).map_err(D::Error::custom)
}
