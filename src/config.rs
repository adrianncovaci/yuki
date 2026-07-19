use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, de::Deserializer};

use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub rpc: Rpc,
    pub wallet: Wallet,
    pub swap: Swap,
    #[serde(default)]
    pub scanner: Scanner,
    pub targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
pub struct Scanner {
    #[serde(default = "default_scanner_interval")]
    pub interval: u64,
}

impl Default for Scanner {
    fn default() -> Self {
        Self {
            interval: default_scanner_interval(),
        }
    }
}

fn default_scanner_interval() -> u64 {
    300
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
    #[serde(default, deserialize_with = "deserialize_optional_pubkey")]
    pub orca_pool: Option<Pubkey>,
    #[serde(default, deserialize_with = "deserialize_optional_pubkey")]
    pub raydium_pool: Option<Pubkey>,
    #[serde(default, deserialize_with = "deserialize_optional_pubkey")]
    pub meteora_dlmm_pool: Option<Pubkey>,
    pub decimals: u32,
}

fn deserialize_pubkey<'de, D>(de: D) -> Result<Pubkey, D::Error>
where
    D: Deserializer<'de>,
{
    let pubkey = String::deserialize(de)?;
    Pubkey::from_str(&pubkey).map_err(D::Error::custom)
}

fn deserialize_optional_pubkey<'de, D>(de: D) -> Result<Option<Pubkey>, D::Error>
where
    D: Deserializer<'de>,
{
    let pubkey: Option<String> = Option::deserialize(de)?;
    match pubkey {
        Some(s) => Pubkey::from_str(&s).map(Some).map_err(D::Error::custom),
        None => Ok(None),
    }
}
