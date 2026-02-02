
use anchor_lang::prelude::*;

declare_id!("BBgRCsGAHwyie2F3kTf2ahNiBJwxzr6f6oF536ZMNMzG");

pub const NULLIFIER_SIZE: usize = 32;

pub const REGISTRATION_SCOPE: &[u8] = b"sth:registration:v1";

#[program]
pub mod sth_registry {
    use super::*;

    pub fn initialize_registry(ctx: Context<InitializeRegistry>) -> Result<()> {
        let registry = &mut ctx.accounts.registry;
        registry.authority = ctx.accounts.authority.key();
        registry.total_registrations = 0;
        registry.bump = ctx.bumps.registry;

        msg!("STH Registry initialized");
        msg!("Authority: {}", registry.authority);

        Ok(())
    }

    pub fn register_human(
        ctx: Context<RegisterHuman>,
        nullifier: [u8; NULLIFIER_SIZE],
    ) -> Result<()> {
        let registry = &mut ctx.accounts.registry;
        let human_record = &mut ctx.accounts.human_record;
        let nullifier_record = &mut ctx.accounts.nullifier_record;

        human_record.wallet = ctx.accounts.wallet.key();
        human_record.nullifier_hash = hash_nullifier(&nullifier);
        human_record.registered_at = Clock::get()?.unix_timestamp;
        human_record.is_active = true;
        human_record.bump = ctx.bumps.human_record;

        nullifier_record.nullifier = nullifier;
        nullifier_record.wallet = ctx.accounts.wallet.key();
        nullifier_record.registered_at = Clock::get()?.unix_timestamp;
        nullifier_record.bump = ctx.bumps.nullifier_record;

        registry.total_registrations += 1;

        msg!("Human registered successfully!");
        msg!("Wallet: {}", ctx.accounts.wallet.key());
        msg!("Nullifier (first 8 bytes): {:?}", &nullifier[..8]);
        msg!("Total registrations: {}", registry.total_registrations);

        Ok(())
    }

    pub fn revoke_registration(ctx: Context<RevokeRegistration>) -> Result<()> {
        let human_record = &mut ctx.accounts.human_record;

        human_record.is_active = false;

        msg!("Registration revoked for wallet: {}", human_record.wallet);

        Ok(())
    }

    pub fn is_verified_human(ctx: Context<CheckHuman>) -> Result<bool> {
        let human_record = &ctx.accounts.human_record;

        msg!("Checking verification status for: {}", human_record.wallet);
        msg!("Is verified: {}", human_record.is_active);
        msg!("Registered at: {}", human_record.registered_at);

        Ok(human_record.is_active)
    }
}

fn hash_nullifier(nullifier: &[u8; NULLIFIER_SIZE]) -> [u8; 8] {
    let mut hash = [0u8; 8];
    hash.copy_from_slice(&nullifier[..8]);
    hash
}

#[derive(Accounts)]
pub struct InitializeRegistry<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + Registry::INIT_SPACE,
        seeds = [b"registry"],
        bump
    )]
    pub registry: Account<'info, Registry>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(nullifier: [u8; NULLIFIER_SIZE])]
pub struct RegisterHuman<'info> {
    #[account(
        mut,
        seeds = [b"registry"],
        bump = registry.bump
    )]
    pub registry: Account<'info, Registry>,

    #[account(
        init,
        payer = wallet,
        space = 8 + HumanRecord::INIT_SPACE,
        seeds = [b"human", wallet.key().as_ref()],
        bump
    )]
    pub human_record: Account<'info, HumanRecord>,

    #[account(
        init,
        payer = wallet,
        space = 8 + NullifierRecord::INIT_SPACE,
        seeds = [b"nullifier", &nullifier],
        bump
    )]
    pub nullifier_record: Account<'info, NullifierRecord>,

    #[account(mut)]
    pub wallet: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RevokeRegistration<'info> {
    #[account(
        seeds = [b"registry"],
        bump = registry.bump
    )]
    pub registry: Account<'info, Registry>,

    #[account(
        mut,
        seeds = [b"human", human_record.wallet.as_ref()],
        bump = human_record.bump,
        constraint = authority.key() == human_record.wallet || authority.key() == registry.authority @ RegistryError::Unauthorized
    )]
    pub human_record: Account<'info, HumanRecord>,

    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct CheckHuman<'info> {
    #[account(
        seeds = [b"human", human_record.wallet.as_ref()],
        bump = human_record.bump
    )]
    pub human_record: Account<'info, HumanRecord>,
}

#[account]
#[derive(InitSpace)]
pub struct Registry {
    pub authority: Pubkey,
    pub total_registrations: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct HumanRecord {
    pub wallet: Pubkey,
    pub nullifier_hash: [u8; 8],
    pub registered_at: i64,
    pub is_active: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct NullifierRecord {
    pub nullifier: [u8; NULLIFIER_SIZE],
    pub wallet: Pubkey,
    pub registered_at: i64,
    pub bump: u8,
}

#[error_code]
pub enum RegistryError {
    #[msg("Unauthorized: only wallet owner or registry authority can perform this action")]
    Unauthorized,
    #[msg("This biometric has already been registered")]
    AlreadyRegistered,
    #[msg("This wallet has already been verified")]
    WalletAlreadyVerified,
    #[msg("Invalid nullifier format")]
    InvalidNullifier,
    #[msg("Invalid ZK proof")]
    InvalidProof,
}
