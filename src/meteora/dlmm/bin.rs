use anyhow::Context;
use meteora_sol::accounts::BinArray;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

use crate::meteora::dlmm::METEORA_DLMM_PROGRAM;

pub const MAX_BIN_PER_ARRAY: i32 = 70;

pub struct BinArrays {
    pub inner: [Pubkey; 3],
}

impl BinArrays {
    pub fn new(pool: Pubkey, active_id: i32, swap_for_y: bool) -> Self {
        let idx0 = active_id.div_euclid(MAX_BIN_PER_ARRAY);
        let indices = if swap_for_y {
            [idx0, idx0 - 1, idx0 - 2]
        } else {
            [idx0, idx0 + 1, idx0 + 2]
        };
        let inner = indices.map(|idx| Self::get_pda(pool, idx));
        Self { inner }
    }

    fn get_pda(pool: Pubkey, index: i32) -> Pubkey {
        Pubkey::find_program_address(
            &[b"bin_array", pool.as_ref(), &(index as i64).to_le_bytes()],
            &METEORA_DLMM_PROGRAM,
        )
        .0
    }

    pub async fn fetch_and_deserialize(
        &self,
        rpc_client: &RpcClient,
    ) -> anyhow::Result<Vec<(Pubkey, BinArray)>> {
        let accounts = rpc_client
            .get_multiple_accounts(&self.inner)
            .await
            .context("Failed to get bin arrays")?;

        let mut res = Vec::new();
        for (idx, maybe) in accounts.into_iter().enumerate() {
            let Some(account) = maybe else {
                continue;
            };
            let bin_array = BinArray::from_bytes(&account.data)
                .with_context(|| format!("Failed to deserialize bin array: {idx}"))?;
            res.push((self.inner[idx], bin_array));
        }
        Ok(res)
    }
}
