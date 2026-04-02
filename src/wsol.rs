use anyhow::Context;
use solana_sdk::{
    message::Instruction, program_pack::Pack, pubkey::Pubkey, signature::Keypair, signer::Signer,
};
use solana_system_interface::instruction::create_account;
use spl_token::instruction::initialize_account;

pub struct WsolSwap {
    pub wsol_keypair: Keypair,
    pub create_ix: Instruction,
    pub init_ix: Instruction,
    pub sync_ix: Instruction,
    pub close_ix: Instruction,
}

impl WsolSwap {
    pub fn new(
        from_address: &Pubkey,
        amount_lamports: u64,
        rent_exempt: u64,
    ) -> anyhow::Result<Self> {
        let wsol_keypair = Keypair::new();

        let create_ix = create_account(
            from_address,
            &wsol_keypair.pubkey(),
            amount_lamports + rent_exempt,
            spl_token::state::Account::LEN as u64,
            &spl_token::id(),
        );

        let init_ix = initialize_account(
            &spl_token::id(),
            &wsol_keypair.pubkey(),
            &spl_token::native_mint::id(),
            from_address,
        )
        .context("Failed to initialize wsol ix")?;

        let sync_ix = spl_token::instruction::sync_native(&spl_token::id(), &wsol_keypair.pubkey())
            .context("Failed to sync wsol ix")?;

        let close_ix = spl_token::instruction::close_account(
            &spl_token::id(),
            &wsol_keypair.pubkey(),
            from_address,
            from_address,
            &[],
        )
        .context("Failed to close wsol ix")?;

        Ok(Self {
            wsol_keypair,
            create_ix,
            init_ix,
            sync_ix,
            close_ix,
        })
    }
}
