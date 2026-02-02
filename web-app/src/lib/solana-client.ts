

import {
  Connection,
  PublicKey,
  Transaction,
  TransactionInstruction,
  SystemProgram,
  clusterApiUrl,
  ComputeBudgetProgram,
} from "@solana/web3.js";

export const STH_REGISTRY_PROGRAM_ID = new PublicKey(
  "BBgRCsGAHwyie2F3kTf2ahNiBJwxzr6f6oF536ZMNMzG"
);
export const DAO_VOTING_PROGRAM_ID = new PublicKey(
  "CN5wNB5qChhKyxaFJBW7WmBvqm2b9THCGDYZnUfB3DA2"
);
export const ZK_VERIFIER_PROGRAM_ID = new PublicKey(
  "BUwQwQYN3XHK7zLxGSkP9ajtfqtif4CrnH74vceVPHSh"
);

export const DAO_AUTHORITY = new PublicKey(
  "7d5L3D7u34tTwkS7DWX9Hph6bfPWy7pvuH7S741ovwxi"
);
export const DAO_PDA = new PublicKey(
  "5Tx9iumm663649UcBboLn6rgAqGhxdAFeTqJiCp7848F"
);

export const connection = new Connection(clusterApiUrl("devnet"), "confirmed");

export type VoteChoice = "yes" | "no" | "abstain";

export function hexToBytes(hex: string): Uint8Array {
  const cleanHex = hex.startsWith("0x") ? hex.slice(2) : hex;
  const bytes = new Uint8Array(cleanHex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(cleanHex.substr(i * 2, 2), 16);
  }
  return bytes;
}

export function scopeToBytes(scope: string): Uint8Array {
  const bytes = new Uint8Array(32);
  const encoder = new TextEncoder();
  const encoded = encoder.encode(scope);
  bytes.set(encoded.slice(0, 31), 1);
  return bytes;
}

export function getRegistryPda(): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("registry")],
    STH_REGISTRY_PROGRAM_ID
  );
}

export function getHumanRecordPda(wallet: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("human"), wallet.toBuffer()],
    STH_REGISTRY_PROGRAM_ID
  );
}

export function getNullifierRecordPda(
  nullifier: Uint8Array
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("nullifier"), nullifier],
    STH_REGISTRY_PROGRAM_ID
  );
}

export function getDaoPda(authority: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("dao"), authority.toBuffer()],
    DAO_VOTING_PROGRAM_ID
  );
}

export function getProposalPda(
  daoPda: PublicKey,
  proposalId: number
): [PublicKey, number] {
  const idBuffer = Buffer.alloc(8);
  idBuffer.writeBigUInt64LE(BigInt(proposalId));

  return PublicKey.findProgramAddressSync(
    [Buffer.from("proposal"), daoPda.toBuffer(), idBuffer],
    DAO_VOTING_PROGRAM_ID
  );
}

export function getVoteNullifierPda(
  proposalPda: PublicKey,
  nullifier: Uint8Array
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("nullifier"), proposalPda.toBuffer(), nullifier],
    DAO_VOTING_PROGRAM_ID
  );
}

export async function isVerifiedHuman(wallet: PublicKey): Promise<boolean> {
  try {
    const [humanRecordPda] = getHumanRecordPda(wallet);
    const accountInfo = await connection.getAccountInfo(humanRecordPda);

    if (!accountInfo) {
      return false;
    }

    const isActive = accountInfo.data[8 + 32 + 8 + 8] === 1;
    return isActive;
  } catch {
    return false;
  }
}

export async function isRegistrationNullifierUsed(
  nullifier: string
): Promise<boolean> {
  const nullifierBytes = hexToBytes(nullifier);
  const [nullifierRecordPda] = getNullifierRecordPda(nullifierBytes);

  const accountInfo = await connection.getAccountInfo(nullifierRecordPda);
  return accountInfo !== null;
}

export function buildRegisterHumanTransaction(
  wallet: PublicKey,
  nullifier: string
): Transaction {
  const nullifierBytes = hexToBytes(nullifier);

  if (nullifierBytes.length !== 32) {
    throw new Error("Nullifier must be exactly 32 bytes (64 hex characters)");
  }

  const [registryPda] = getRegistryPda();
  const [humanRecordPda] = getHumanRecordPda(wallet);
  const [nullifierRecordPda] = getNullifierRecordPda(nullifierBytes);

  const discriminator = Buffer.from([
    0x2d, 0x4c, 0x5b, 0x9f, 0x1e, 0x3a, 0x8b, 0x7c,
  ]);

  const data = Buffer.concat([discriminator, Buffer.from(nullifierBytes)]);

  const instruction = new TransactionInstruction({
    keys: [
      { pubkey: registryPda, isSigner: false, isWritable: true },
      { pubkey: humanRecordPda, isSigner: false, isWritable: true },
      { pubkey: nullifierRecordPda, isSigner: false, isWritable: true },
      { pubkey: wallet, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId: STH_REGISTRY_PROGRAM_ID,
    data,
  });

  const transaction = new Transaction().add(instruction);
  transaction.feePayer = wallet;

  return transaction;
}

export async function hasVotedOnProposal(
  proposalPda: PublicKey,
  nullifier: string
): Promise<boolean> {
  const nullifierBytes = hexToBytes(nullifier);
  const [voteNullifierPda] = getVoteNullifierPda(proposalPda, nullifierBytes);

  const accountInfo = await connection.getAccountInfo(voteNullifierPda);
  return accountInfo !== null;
}

export function buildCastVoteTransaction(
  voter: PublicKey,
  daoAuthority: PublicKey,
  proposalId: number,
  nullifier: string,
  choice: VoteChoice
): Transaction {
  const nullifierBytes = hexToBytes(nullifier);

  if (nullifierBytes.length !== 32) {
    throw new Error("Nullifier must be exactly 32 bytes (64 hex characters)");
  }

  const [daoPda] = getDaoPda(daoAuthority);
  const [proposalPda] = getProposalPda(daoPda, proposalId);
  const [voteNullifierPda] = getVoteNullifierPda(proposalPda, nullifierBytes);

  const discriminator = Buffer.from([20, 212, 15, 189, 69, 180, 69, 151]);

  const choiceMap: Record<VoteChoice, number> = {
    yes: 0,
    no: 1,
    abstain: 2,
  };

  const data = Buffer.concat([
    discriminator,
    Buffer.from(nullifierBytes),
    Buffer.from([choiceMap[choice]]),
  ]);

  const instruction = new TransactionInstruction({
    keys: [
      { pubkey: daoPda, isSigner: false, isWritable: false },
      { pubkey: proposalPda, isSigner: false, isWritable: true },
      { pubkey: voteNullifierPda, isSigner: false, isWritable: true },
      { pubkey: voter, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId: DAO_VOTING_PROGRAM_ID,
    data,
  });

  const transaction = new Transaction().add(instruction);
  transaction.feePayer = voter;

  return transaction;
}

export function buildCastVoteWithProofTransaction(
  voter: PublicKey,
  daoAuthority: PublicKey,
  proposalId: number,
  proof: Uint8Array,
  nullifier: Uint8Array,
  commitment: Uint8Array,
  merkleRoot: Uint8Array,
  scope: Uint8Array,
  choice: VoteChoice
): Transaction {
  if (proof.length !== 324) {
    throw new Error(`Proof must be exactly 324 bytes (Groth16), got ${proof.length}`);
  }

  if (nullifier.length !== 32) {
    throw new Error("Nullifier must be exactly 32 bytes");
  }

  if (commitment.length !== 32) {
    throw new Error("Commitment must be exactly 32 bytes");
  }

  if (merkleRoot.length !== 32) {
    throw new Error("Merkle root must be exactly 32 bytes");
  }

  if (scope.length !== 32) {
    throw new Error("Scope must be exactly 32 bytes");
  }

  const [daoPda] = getDaoPda(daoAuthority);
  const [proposalPda] = getProposalPda(daoPda, proposalId);
  const [voteNullifierPda] = getVoteNullifierPda(proposalPda, nullifier);

  const discriminator = Buffer.from([0x76, 0x44, 0x5e, 0xd5, 0xa4, 0x10, 0xc1, 0x5a]);

  const choiceMap: Record<VoteChoice, number> = {
    yes: 0,
    no: 1,
    abstain: 2,
  };

  const proofLenBuffer = Buffer.alloc(4);
  proofLenBuffer.writeUInt32LE(proof.length);

  const data = Buffer.concat([
    discriminator,
    proofLenBuffer,
    Buffer.from(proof),
    Buffer.from(nullifier),
    Buffer.from(commitment),
    Buffer.from(scope),
    Buffer.from([choiceMap[choice]]),
  ]);

  const instruction = new TransactionInstruction({
    keys: [
      { pubkey: daoPda, isSigner: false, isWritable: false },
      { pubkey: proposalPda, isSigner: false, isWritable: true },
      { pubkey: voteNullifierPda, isSigner: false, isWritable: true },
      { pubkey: voter, isSigner: true, isWritable: true },
      { pubkey: ZK_VERIFIER_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId: DAO_VOTING_PROGRAM_ID,
    data,
  });

  const computeBudgetIx = ComputeBudgetProgram.setComputeUnitLimit({
    units: 1_400_000,
  });

  const transaction = new Transaction()
    .add(computeBudgetIx)
    .add(instruction);
  transaction.feePayer = voter;

  return transaction;
}

export function buildUpdateMerkleRootTransaction(
  authority: PublicKey,
  newMerkleRoot: Uint8Array
): Transaction {
  if (newMerkleRoot.length !== 32) {
    throw new Error("Merkle root must be exactly 32 bytes");
  }

  const [daoPda] = getDaoPda(authority);

  const discriminator = Buffer.from([0x1e, 0x73, 0x28, 0x78, 0x5e, 0x68, 0x4e, 0xf3]);

  const data = Buffer.concat([discriminator, Buffer.from(newMerkleRoot)]);

  const instruction = new TransactionInstruction({
    keys: [
      { pubkey: daoPda, isSigner: false, isWritable: true },
      { pubkey: authority, isSigner: true, isWritable: false },
    ],
    programId: DAO_VOTING_PROGRAM_ID,
    data,
  });

  const transaction = new Transaction().add(instruction);
  transaction.feePayer = authority;

  return transaction;
}

export async function getProposalVotes(proposalPda: PublicKey): Promise<{
  yes: number;
  no: number;
  abstain: number;
} | null> {
  const accountInfo = await connection.getAccountInfo(proposalPda);
  if (!accountInfo) {
    return null;
  }

  return null;
}
