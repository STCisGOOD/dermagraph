
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::invoke;

declare_id!("CN5wNB5qChhKyxaFJBW7WmBvqm2b9THCGDYZnUfB3DA2");

pub const ZK_VERIFIER_PROGRAM_ID: &str = "BUwQwQYN3XHK7zLxGSkP9ajtfqtif4CrnH74vceVPHSh";

pub const NULLIFIER_SIZE: usize = 32;

pub const MAX_TITLE_LENGTH: usize = 64;

pub const GROTH16_PROOF_SIZE: usize = 324;

#[program]
pub mod dao_voting {
    use super::*;

    pub fn initialize_dao(
        ctx: Context<InitializeDao>,
        merkle_root: [u8; 32],
        name: String,
    ) -> Result<()> {
        let dao = &mut ctx.accounts.dao;
        dao.authority = ctx.accounts.authority.key();
        dao.merkle_root = merkle_root;
        dao.name = name;
        dao.proposal_count = 0;
        dao.bump = ctx.bumps.dao;

        msg!("DAO initialized: {}", dao.name);
        msg!("Merkle root: {:?}", merkle_root);

        Ok(())
    }

    pub fn update_merkle_root(
        ctx: Context<UpdateMerkleRoot>,
        new_merkle_root: [u8; 32],
    ) -> Result<()> {
        let dao = &mut ctx.accounts.dao;
        dao.merkle_root = new_merkle_root;

        msg!("Merkle root updated");

        Ok(())
    }

    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        title: String,
        description: String,
    ) -> Result<()> {
        require!(title.len() <= MAX_TITLE_LENGTH, DaoError::TitleTooLong);

        let dao = &mut ctx.accounts.dao;
        let proposal = &mut ctx.accounts.proposal;

        proposal.dao = dao.key();
        proposal.id = dao.proposal_count;
        proposal.title = title.clone();
        proposal.description = description;
        proposal.yes_votes = 0;
        proposal.no_votes = 0;
        proposal.abstain_votes = 0;
        proposal.status = ProposalStatus::Active;
        proposal.created_at = Clock::get()?.unix_timestamp;
        proposal.bump = ctx.bumps.proposal;

        dao.proposal_count += 1;

        msg!("Proposal #{} created: {}", proposal.id, title);

        Ok(())
    }

    pub fn cast_vote(
        ctx: Context<CastVote>,
        nullifier: [u8; 32],
        vote_choice: VoteChoice,
    ) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let nullifier_account = &mut ctx.accounts.nullifier_account;

        require!(
            proposal.status == ProposalStatus::Active,
            DaoError::ProposalNotActive
        );

        nullifier_account.nullifier = nullifier;
        nullifier_account.proposal = proposal.key();
        nullifier_account.vote_choice = vote_choice.clone();
        nullifier_account.timestamp = Clock::get()?.unix_timestamp;
        nullifier_account.bump = ctx.bumps.nullifier_account;

        match vote_choice {
            VoteChoice::Yes => proposal.yes_votes += 1,
            VoteChoice::No => proposal.no_votes += 1,
            VoteChoice::Abstain => proposal.abstain_votes += 1,
        }

        msg!("Vote cast! Nullifier: {:?}", &nullifier[..8]);
        msg!("Current tally - Yes: {}, No: {}, Abstain: {}",
            proposal.yes_votes, proposal.no_votes, proposal.abstain_votes);

        Ok(())
    }

    pub fn cast_vote_with_proof(
        ctx: Context<CastVoteWithProof>,
        proof: Vec<u8>,
        nullifier: [u8; 32],
        commitment: [u8; 32],
        scope: [u8; 32],
        vote_choice: VoteChoice,
    ) -> Result<()> {
        let dao = &ctx.accounts.dao;
        let proposal = &mut ctx.accounts.proposal;
        let nullifier_account = &mut ctx.accounts.nullifier_account;

        require!(
            proposal.status == ProposalStatus::Active,
            DaoError::ProposalNotActive
        );

        require!(
            proof.len() == GROTH16_PROOF_SIZE,
            DaoError::InvalidProofSize
        );

        verify_groth16_proof(
            &proof,
            &nullifier,
            &commitment,
            &dao.merkle_root,
            &scope,
        )?;

        msg!("ZK proof verified on-chain!");

        nullifier_account.nullifier = nullifier;
        nullifier_account.proposal = proposal.key();
        nullifier_account.vote_choice = vote_choice.clone();
        nullifier_account.timestamp = Clock::get()?.unix_timestamp;
        nullifier_account.bump = ctx.bumps.nullifier_account;

        match vote_choice {
            VoteChoice::Yes => proposal.yes_votes += 1,
            VoteChoice::No => proposal.no_votes += 1,
            VoteChoice::Abstain => proposal.abstain_votes += 1,
        }

        msg!("ZK-verified vote cast! Nullifier: {:?}", &nullifier[..8]);
        msg!("Current tally - Yes: {}, No: {}, Abstain: {}",
            proposal.yes_votes, proposal.no_votes, proposal.abstain_votes);

        Ok(())
    }

    pub fn close_proposal(ctx: Context<CloseProposal>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;

        require!(
            proposal.status == ProposalStatus::Active,
            DaoError::ProposalNotActive
        );

        if proposal.yes_votes > proposal.no_votes {
            proposal.status = ProposalStatus::Passed;
        } else {
            proposal.status = ProposalStatus::Rejected;
        }

        msg!("Proposal #{} closed. Status: {:?}", proposal.id, proposal.status);
        msg!("Final tally - Yes: {}, No: {}, Abstain: {}",
            proposal.yes_votes, proposal.no_votes, proposal.abstain_votes);

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(merkle_root: [u8; 32], name: String)]
pub struct InitializeDao<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + Dao::INIT_SPACE,
        seeds = [b"dao", authority.key().as_ref()],
        bump
    )]
    pub dao: Account<'info, Dao>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateMerkleRoot<'info> {
    #[account(
        mut,
        seeds = [b"dao", authority.key().as_ref()],
        bump = dao.bump,
        has_one = authority
    )]
    pub dao: Account<'info, Dao>,

    pub authority: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(title: String, description: String)]
pub struct CreateProposal<'info> {
    #[account(
        mut,
        seeds = [b"dao", authority.key().as_ref()],
        bump = dao.bump,
        has_one = authority
    )]
    pub dao: Account<'info, Dao>,

    #[account(
        init,
        payer = authority,
        space = 8 + Proposal::INIT_SPACE,
        seeds = [b"proposal", dao.key().as_ref(), &dao.proposal_count.to_le_bytes()],
        bump
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(nullifier: [u8; 32], vote_choice: VoteChoice)]
pub struct CastVote<'info> {
    #[account(
        seeds = [b"dao", dao.authority.as_ref()],
        bump = dao.bump
    )]
    pub dao: Account<'info, Dao>,

    #[account(
        mut,
        seeds = [b"proposal", dao.key().as_ref(), &proposal.id.to_le_bytes()],
        bump = proposal.bump,
        has_one = dao
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        init,
        payer = voter,
        space = 8 + NullifierRecord::INIT_SPACE,
        seeds = [b"nullifier", proposal.key().as_ref(), &nullifier],
        bump
    )]
    pub nullifier_account: Account<'info, NullifierRecord>,

    #[account(mut)]
    pub voter: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(proof: Vec<u8>, nullifier: [u8; 32], commitment: [u8; 32], scope: [u8; 32], vote_choice: VoteChoice)]
pub struct CastVoteWithProof<'info> {
    #[account(
        seeds = [b"dao", dao.authority.as_ref()],
        bump = dao.bump
    )]
    pub dao: Account<'info, Dao>,

    #[account(
        mut,
        seeds = [b"proposal", dao.key().as_ref(), &proposal.id.to_le_bytes()],
        bump = proposal.bump,
        has_one = dao
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        init,
        payer = voter,
        space = 8 + NullifierRecord::INIT_SPACE,
        seeds = [b"nullifier", proposal.key().as_ref(), &nullifier],
        bump
    )]
    pub nullifier_account: Account<'info, NullifierRecord>,

    #[account(mut)]
    pub voter: Signer<'info>,

    pub zk_verifier: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CloseProposal<'info> {
    #[account(
        seeds = [b"dao", dao.authority.as_ref()],
        bump = dao.bump,
        has_one = authority
    )]
    pub dao: Account<'info, Dao>,

    #[account(
        mut,
        seeds = [b"proposal", dao.key().as_ref(), &proposal.id.to_le_bytes()],
        bump = proposal.bump,
        has_one = dao
    )]
    pub proposal: Account<'info, Proposal>,

    pub authority: Signer<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct Dao {
    pub authority: Pubkey,
    pub merkle_root: [u8; 32],
    #[max_len(64)]
    pub name: String,
    pub proposal_count: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Proposal {
    pub dao: Pubkey,
    pub id: u64,
    #[max_len(64)]
    pub title: String,
    #[max_len(512)]
    pub description: String,
    pub yes_votes: u64,
    pub no_votes: u64,
    pub abstain_votes: u64,
    pub status: ProposalStatus,
    pub created_at: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct NullifierRecord {
    pub nullifier: [u8; 32],
    pub proposal: Pubkey,
    pub vote_choice: VoteChoice,
    pub timestamp: i64,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, InitSpace, Debug)]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, InitSpace, Debug)]
pub enum VoteChoice {
    Yes,
    No,
    Abstain,
}

#[error_code]
pub enum DaoError {
    #[msg("Proposal is not active")]
    ProposalNotActive,
    #[msg("Title too long (max 64 characters)")]
    TitleTooLong,
    #[msg("Invalid merkle proof")]
    InvalidMerkleProof,
    #[msg("Invalid nullifier")]
    InvalidNullifier,
    #[msg("Invalid ZK proof size (expected 324 bytes)")]
    InvalidProofSize,
    #[msg("ZK proof verification failed")]
    ZkProofVerificationFailed,
}

fn verify_groth16_proof(
    proof: &[u8],
    nullifier: &[u8; 32],
    commitment: &[u8; 32],
    merkle_root: &[u8; 32],
    scope: &[u8; 32],
) -> Result<()> {
    let mut public_witness = Vec::with_capacity(12 + 4 * 32);

    let nr_inputs: u32 = 4;
    public_witness.extend_from_slice(&nr_inputs.to_be_bytes());
    public_witness.extend_from_slice(&[0u8; 8]);

    public_witness.extend_from_slice(commitment);
    public_witness.extend_from_slice(merkle_root);
    public_witness.extend_from_slice(scope);
    public_witness.extend_from_slice(nullifier);

    let mut instruction_data = Vec::with_capacity(proof.len() + public_witness.len());
    instruction_data.extend_from_slice(proof);
    instruction_data.extend_from_slice(&public_witness);

    let zk_verifier_pubkey = ZK_VERIFIER_PROGRAM_ID
        .parse::<Pubkey>()
        .map_err(|_| DaoError::ZkProofVerificationFailed)?;

    let verify_ix = Instruction {
        program_id: zk_verifier_pubkey,
        accounts: vec![],
        data: instruction_data,
    };

    invoke(&verify_ix, &[]).map_err(|e| {
        msg!("ZK proof verification failed: {:?}", e);
        DaoError::ZkProofVerificationFailed
    })?;

    msg!("Groth16 proof verified successfully via Sunspot");
    Ok(())
}
