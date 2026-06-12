use solana_pubkey::Pubkey as MeteoraPubkey;
use solana_sdk::pubkey;
use solana_sdk::pubkey::Pubkey;

pub mod bin;
pub mod dca;
pub mod dex;
pub mod errors;
pub mod quote;

pub const METEORA_DLMM_PROGRAM: Pubkey = pubkey!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");

pub const MEMO_PROGRAM: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

pub fn to_meteora_pubkey(p: &Pubkey) -> MeteoraPubkey {
    MeteoraPubkey::new_from_array(p.to_bytes())
}

pub fn bin_price_ui(active_id: i32, bin_step: u16, decimals_x: u8, decimals_y: u8) -> f64 {
    (1.0 + bin_step as f64 / 10_000.0).powi(active_id)
        * 10f64.powi(decimals_x as i32 - decimals_y as i32)
}

#[cfg(test)]
mod tests {
    use crate::meteora::dlmm::METEORA_DLMM_PROGRAM;

    #[test]
    fn dlmm_program_pubkey_match() {
        assert_eq!(
            meteora_sol::programs::LB_CLMM_ID.to_bytes(),
            METEORA_DLMM_PROGRAM.to_bytes()
        );
    }
}
