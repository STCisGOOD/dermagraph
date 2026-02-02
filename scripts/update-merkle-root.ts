

import {
  Connection,
  PublicKey,
  Keypair,
  Transaction,
  TransactionInstruction,
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

const UPDATE_MERKLE_ROOT_DISCRIMINATOR = Buffer.from([0xc3, 0xad, 0x26, 0x3c, 0xf2, 0xcb, 0x9e, 0x5d]);

function hexToBytes(hex: string): Buffer {
  const cleanHex = hex.startsWith("0x") ? hex.slice(2) : hex;
  return Buffer.from(cleanHex, "hex");
}

async function updateMerkleRoot(wallet: Keypair, newMerkleRoot: Buffer) {
  const [daoPda] = getDaoPda(wallet.publicKey);

  console.log("Updating DAO Merkle Root...");
  console.log("  Authority:", wallet.publicKey.toBase58());
  console.log("  DAO PDA:", daoPda.toBase58());
  console.log("  New Merkle Root:", "0x" + newMerkleRoot.toString("hex"));

  const daoAccount = await connection.getAccountInfo(daoPda);
  if (!daoAccount) {
    throw new Error("DAO not initialized! Run init-dao.ts first.");
  }

  const currentRoot = daoAccount.data.slice(8 + 32, 8 + 32 + 32);
  console.log("  Current Merkle Root:", "0x" + currentRoot.toString("hex"));

  const data = Buffer.concat([
    UPDATE_MERKLE_ROOT_DISCRIMINATOR,
    newMerkleRoot,
  ]);

  const ix = new TransactionInstruction({
    keys: [
      { pubkey: daoPda, isSigner: false, isWritable: true },
      { pubkey: wallet.publicKey, isSigner: true, isWritable: false },
    ],
    programId: DAO_VOTING_PROGRAM_ID,
    data,
  });

  const tx = new Transaction().add(ix);
  const sig = await sendAndConfirmTransaction(connection, tx, [wallet]);
  console.log("\n  ✓ Updated! Signature:", sig);
  console.log("  Explorer: https://explorer.solana.com/tx/" + sig + "?cluster=devnet");
}

async function main() {
  console.log("=== Update DAO Merkle Root ===\n");

  const merkleRootHex = process.argv[2];
  if (!merkleRootHex) {
    console.error("Usage: npx ts-node scripts/update-merkle-root.ts <merkle_root_hex>");
    console.error("Example: npx ts-node scripts/update-merkle-root.ts 04493830da2e456e8f11d5f697afd756252b5c6c1f35e0873f722f178597ec95");
    process.exit(1);
  }

  const merkleRoot = hexToBytes(merkleRootHex);
  if (merkleRoot.length !== 32) {
    console.error("Error: Merkle root must be exactly 32 bytes (64 hex characters)");
    console.error("Got:", merkleRoot.length, "bytes");
    process.exit(1);
  }

  const wallet = loadWallet();
  console.log("Wallet:", wallet.publicKey.toBase58());

  const balance = await connection.getBalance(wallet.publicKey);
  console.log("Balance:", balance / 1e9, "SOL\n");

  await updateMerkleRoot(wallet, merkleRoot);

  console.log("\n=== Done! ===");
}

main().catch(console.error);
