
> Dermagraph: Privacy-Preserving Biometric Identity for Solana

Aztec: ZK with Noir


Dermagraph uses Noir to build the first **biometric identity system with on-chain ZK verification** on Solana.


| Requirement | Dermagraph Implementation |
|-------------|---------------------------|
| Uses Noir | Yes: `person_identity` circuit (33,700 constraints) |
| ZK on Solana | Yes: Groth16 proofs verified via Sunspot |
| Non-financial | Yes: Identity/governance, not DeFi |

**Technical Highlights:**
- Custom Noir circuit for privacy-preserving identity proofs
- Poseidon hash for commitments and nullifiers
- Sparse Merkle tree for identity registry (depth 20, ~1M capacity)
- Groth16 proof generation on Raspberry Pi (~4 seconds)

---


**Supported by Light Protocol & Solana Foundation**

Dermagraph brings **hardware-backed privacy** to Solana with real fingerprint sensors.

**Why This Qualifies:**

| Requirement | Dermagraph Implementation |
|-------------|---------------------------|
| Privacy on Solana | Yes: Biometric never leaves device |
| Novel tech | Yes: First fingerprint → ZK → Solana pipeline |
| Working demo | Yes: Live on devnet with real hardware |
| Open source | Yes: MIT licensed |

**Privacy Properties:**
1. **Data minimization**: Only 324-byte proof goes on-chain
2. **Unlinkability**: Different scopes produce different nullifiers
3. **Zero-knowledge**: Proof reveals nothing about biometric
4. **Hardware isolation**: Biometric processing on air-gapped Pi

---

## What We Built

### End-to-End System

```
Fingerprint Sensor → Raspberry Pi → ZK Proof → Solana Transaction
      R503            dermagraphd     Noir        On-chain
```

### Components

| Component | Tech Stack | Lines of Code |
|-----------|------------|---------------|
| Cryptographic core | Rust (BN254, Poseidon) | ~5,000 |
| Noir circuit | Noir | ~500 |
| Daemon | Rust (Axum, Tokio) | ~3,000 |
| Solana program | Anchor | ~500 |
| Frontend | React, TypeScript | ~2,000 |
| Hardware driver | Rust (tokio-serial) | ~800 |

### Novel Contributions

1. **Cross-finger sybil resistance**: CNN embeddings where same person → same nullifier regardless of which finger

2. **X-Lock fuzzy extractor**: Handles biometric noise without leaking helper data

3. **Persistent Merkle tree**: Stable merkle_root across proof generations

4. **Edge device proving**: Groth16 on Raspberry Pi (not cloud)

---

## Demo

### Live Transaction

**TX:** [`38FPt6dgJahb7qtPGd4H7SNLo1qa3cEZ...`](https://explorer.solana.com/tx/38FPt6dgJahb7qtPGd4H7SNLo1qa3cEZMVKHVf4wQfHA4vF5q6eBYHdrH6BRyptR6PcERYxqcSTW34EoCz4szks3?cluster=devnet)


### Program IDs

| Program | Address |
|---------|---------|
| DAO Voting | `CN5wNB5qChhKyxaFJBW7WmBvqm2b9THCGDYZnUfB3DA2` |
| Sunspot Verifier | `BUwQwQYN3XHK7zLxGSkP9ajtfqtif4CrnH74vceVPHSh` |

---

## Why Dermagraph Should Win

### 1. Solves a Real Problem

**Sybil attacks cost the ecosystem millions:**
- Airdrop farming (one person, 100 wallets)
- DAO vote manipulation
- Fake account spam

**Dermagraph provides cryptographic proof of uniqueness** without sacrificing privacy.

### 2. Novel Technical Approach

No other project combines:
- Real hardware (fingerprint sensor)
- ZK proofs (Noir/Groth16)
- On-chain verification (Solana)
- Edge computing (Raspberry Pi)

### 3. Production-Ready Architecture

It's a production system:
- Encrypted storage with XChaCha20-Poly1305
- Atomic file operations
- Rate limiting and error handling
- Comprehensive logging

### 4. Open Source & Extensible

MIT licensed. Any project can integrate:
- Sybil-resistant airdrops
- One-person-one-vote DAOs
- Proof of humanity for social graphs
- Quadratic funding with genuine users


---

## Try It Yourself

```bash
git clone https://github.com/STCisGOOD/dermagraph.git
cd dermagraph

# Run with mock sensor (no hardware needed)
cargo run -p dermagraphd -- --sensor mock

# Or deploy to Raspberry Pi with real sensor
./scripts/deploy-to-pi.sh
```

---

## Links

- **GitHub**: [github.com/STCisGOOD/dermagraph](https://github.com/STCisGOOD/dermagraph)
- **Demo Video**: [x.com/stcisgood](https://x.com/stcisgood/status/2018140868868293118)
- **Live Transaction**: [Solana Explorer](https://explorer.solana.com/tx/38FPt6dgJahb7qtPGd4H7SNLo1qa3cEZMVKHVf4wQfHA4vF5q6eBYHdrH6BRyptR6PcERYxqcSTW34EoCz4szks3?cluster=devnet)

---

