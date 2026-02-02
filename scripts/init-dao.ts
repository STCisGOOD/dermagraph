

import {
  Connection,
  PublicKey,
  Keypair,
  Transaction,
  TransactionInstruction,
  SystemProgram,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

const DAO_VOTING_PROGRAM_ID = new PublicKey("CN5wNB5qChhKyxaFJBW7WmBvqm2b9THCGDYZnUfB3DA2");

const connection = new Connection("https://api.devnet.solana.com", "confirmed");

function loadWallet(): Keypair {
  const walletPath = path.join(os.homedir(), ".config", "solana", "id.json");
  const secretKey = JSON.parse(fs.readFileSync(walletPath, "utf-8"));
  return Keypair.fromSecretKey(new Uint8Array(secretKey));
}

function getDaoPda(authority: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("dao"), authority.toBuffer()],
    DAO_VOTING_PROGRAM_ID
  );
}

function getProposalPda(dao: PublicKey, proposalId: number): [PublicKey, number] {
  const idBuffer = Buffer.alloc(8);
  idBuffer.writeBigUInt64LE(BigInt(proposalId));
  return PublicKey.findProgramAddressSync(
    [Buffer.from("proposal"), dao.toBuffer(), idBuffer],
    DAO_VOTING_PROGRAM_ID
  );
}

const INITIALIZE_DAO_DISCRIMINATOR = Buffer.from([128, 226, 96, 90, 39, 56, 24, 196]);
const CREATE_PROPOSAL_DISCRIMINATOR = Buffer.from([132, 116, 68, 174, 216, 160, 198, 22]);

async function initializeDao(wallet: Keypair, merkleRoot: Buffer, name: string) {
  const [daoPda, bump] = getDaoPda(wallet.publicKey);

  console.log("Initializing DAO...");
  console.log("  Authority:", wallet.publicKey.toBase58());
  console.log("  DAO PDA:", daoPda.toBase58());

  const daoAccount = await connection.getAccountInfo(daoPda);
  if (daoAccount) {
    console.log("  DAO already initialized!");
    return daoPda;
  }

  const nameBuffer = Buffer.from(name, "utf-8");
  const data = Buffer.concat([
    INITIALIZE_DAO_DISCRIMINATOR,
    merkleRoot,
    Buffer.from([nameBuffer.length, 0, 0, 0]),
    nameBuffer,
  ]);

  const ix = new TransactionInstruction({
    keys: [
      { pubkey: daoPda, isSigner: false, isWritable: true },
      { pubkey: wallet.publicKey, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId: DAO_VOTING_PROGRAM_ID,
    data,
  });

  const tx = new Transaction().add(ix);
  const sig = await sendAndConfirmTransaction(connection, tx, [wallet]);
  console.log("  Initialized! Signature:", sig);

  return daoPda;
}

async function createProposal(wallet: Keypair, daoPda: PublicKey, proposalId: number, title: string, description: string) {
  const [proposalPda, bump] = getProposalPda(daoPda, proposalId);

  console.log(`Creating Proposal #${proposalId}...`);
  console.log("  Title:", title);
  console.log("  Proposal PDA:", proposalPda.toBase58());

  const proposalAccount = await connection.getAccountInfo(proposalPda);
  if (proposalAccount) {
    console.log("  Proposal already exists!");
    return proposalPda;
  }

  const titleBuffer = Buffer.from(title, "utf-8");
  const descBuffer = Buffer.from(description, "utf-8");
  const data = Buffer.concat([
    CREATE_PROPOSAL_DISCRIMINATOR,
    Buffer.from([titleBuffer.length, 0, 0, 0]),
    titleBuffer,
    Buffer.from([descBuffer.length, 0, 0, 0]),
    descBuffer,
  ]);

  const ix = new TransactionInstruction({
    keys: [
      { pubkey: daoPda, isSigner: false, isWritable: true },
      { pubkey: proposalPda, isSigner: false, isWritable: true },
      { pubkey: wallet.publicKey, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId: DAO_VOTING_PROGRAM_ID,
    data,
  });

  const tx = new Transaction().add(ix);
  const sig = await sendAndConfirmTransaction(connection, tx, [wallet]);
  console.log("  Created! Signature:", sig);

  return proposalPda;
}

async function main() {
  console.log("=== STH DAO Initialization ===\n");

  const wallet = loadWallet();
  console.log("Wallet:", wallet.publicKey.toBase58());

  const balance = await connection.getBalance(wallet.publicKey);
  console.log("Balance:", balance / 1e9, "SOL\n");

  const merkleRoot = Buffer.alloc(32);
  merkleRoot[0] = 0x21;

  const daoPda = await initializeDao(wallet, merkleRoot, "Biometric DAO");

  console.log("");

  const proposals = [
    { title: "Fund ZK Research", desc: "Allocate 10,000 tokens for zero-knowledge cryptography research." },
    { title: "Community Treasury", desc: "Create a community-managed treasury for ecosystem grants." },
    { title: "Protocol Upgrade v2", desc: "Implement biometric verification for all governance votes." },
  ];

  for (let i = 0; i < proposals.length; i++) {
    await createProposal(wallet, daoPda, i, proposals[i].title, proposals[i].desc);
    console.log("");
  }

  console.log("=== Done! ===");
  console.log("DAO PDA:", daoPda.toBase58());
}

main().catch(console.error);
