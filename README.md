# Dermagraph [WINNER OF MOST CREATIVE] https://x.com/solana_devs/status/2022387550824927345

**Privacy-Preserving Biometric Identity for Solana**

> Your fingerprint becomes your cryptographic identity—without ever leaving your device.


https://github.com/user-attachments/assets/4190471e-2499-4369-a8e1-662d5362f04c


[![Solana](https://img.shields.io/badge/Solana-Devnet-9945FF?logo=solana)](https://solana.com)
[![Noir](https://img.shields.io/badge/Noir-ZK%20Circuits-000000)](https://noir-lang.org)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

Dermagraph is a **biometric identity layer** that enables sybil-resistant, privacy-preserving authentication on Solana. It uses **zero-knowledge proofs** (Noir/Groth16) to prove you're a unique human without revealing your biometric data.

Your fingerprint generates a **deterministic nullifier** through zero-knowledge cryptography. The same person always produces the same nullifier for a given context—but different contexts are unlinkable.

---



---

## Demo: DAO Voting with Biometric Verification

https://x.com/stcisgood/status/2018140868868293118

**What you're seeing:**
1. User scans fingerprint on R503 sensor
2. Daemon generates Noir witness + Groth16 proof
3. Proof submitted to Solana with vote
4. Sunspot verifier confirms on-chain
5. Vote recorded with nullifier (prevents double-voting)

---

## Quick Start

### Prerequisites

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Node.js 18+
# https://nodejs.org/

# Solana CLI
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"

# Noir (nargo)
curl -L https://raw.githubusercontent.com/noir-lang/noirup/main/install | bash
noirup

# Sunspot (Groth16 prover for Solana)
cargo install sunspot-cli
```

### 1. Clone & Download Model Weights

```bash
git clone https://github.com/STCisGOOD/dermagraph.git
cd dermagraph

# Download pre-trained CNN weights from release (~45MB)
mkdir -p crates/biometric-extract/checkpoints
curl -L -o crates/biometric-extract/checkpoints/best_burn.safetensors \
  https://github.com/STCisGOOD/dermagraph/releases/download/v1.0.0/best_burn.safetensors
```

### 2. Configure Environment

```bash
# Set up web app environment
cd web-app
cp .env.example .env.local

# Edit .env.local:
# - Get a Privy App ID from https://dashboard.privy.io
# - Set VITE_DAEMON_URL=http://localhost:31415 for local dev
# - Set VITE_USE_REAL_SOLANA=false for mock mode (no SOL needed)
```

### 3. Build

```bash
cd ..  # back to repo root

# Build Rust crates
cargo build --release

# Build Noir circuit
cd circuits/person_identity && nargo compile && cd ../..

# Install frontend deps
cd web-app && npm install && cd ..

# Install bridge server deps
cd bridge-server && npm install && cd ..
```

### 4. Run the Demo (Mock Sensor)

```bash
# Terminal 1: Daemon with mock sensor
cargo run --release -p dermagraphd -- --sensor mock

# Terminal 2: Bridge server
cd bridge-server && npm start

# Terminal 3: Frontend
cd web-app && npm run dev
```

Open http://localhost:5173 and connect your wallet!

> **Note:** Mock mode simulates fingerprint scans. For real hardware setup with Raspberry Pi + R503 sensor, see [SETUP.md](./SETUP.md).

---

## Hardware Setup

**Minimum Hardware (~$140):**
- R503 Capacitive Fingerprint Sensor (~$25)
- Raspberry Pi 4 (~$100)
- MicroSD card (~$8)
- Jumper wires (~$3)

**Wiring:**
```
R503          Raspberry Pi
────          ────────────
VCC (Red)  →  3.3V (Pin 1)
GND (Black)→  GND (Pin 6)
TX (Yellow)→  GPIO15/RX (Pin 10)
RX (Green) →  GPIO14/TX (Pin 8)
```


---

## Security Properties

| Property | Mechanism |
|----------|-----------|
| **Biometric Privacy** | ZK proofs—embedding never leaves device |
| **Sybil Resistance** | Deterministic nullifiers from biometric |
| **Unlinkability** | Different scope → different nullifier |
| **Non-Transferability** | Requires physical finger + device |
| **Liveness** | Capacitive sensor detects real finger |
| **Double-Vote Prevention** | Nullifier stored on-chain |

---

## API Reference

### Daemon Endpoints

```http
POST /enroll-fingers-stream
Content-Type: application/json

{"scope": "dermagraph:identity:v1"}

# Response: SSE stream with enrollment progress
```

```http
POST /verify-finger
Content-Type: application/json

{"scope": "dao:proposal:0", "passphrase": null}

# Response:
{
  "success": true,
  "data": {
    "verified": true,
    "nullifier": "0x8fd2f7ab...",
    "matched_finger": "index"
  }
}
```

```http
POST /prove-person
Content-Type: application/json

{"scope": "dao:proposal:0"}

# Response:
{
  "success": true,
  "data": {
    "proof": "0x0a1d8388...",
    "nullifier": "0x1352d91f...",
    "merkle_root": "0x04493830...",
    "commitment": "0x2310e8fd..."
  }
}
```

---

## Roadmap

- [x] Core cryptographic primitive (Poseidon-based)
- [x] Noir ZK circuit for person identity
- [x] R503 sensor integration
- [x] On-chain Groth16 verification (Sunspot)
- [x] DAO voting demo
- [ ] Mobile app (iOS/Android)
- [ ] Multi-device sync
- [ ] Decentralized identity registry
- [ ] Integration with Light Protocol compressed accounts

---

## Research Foundation

Dermagraph builds on peer-reviewed research:

1. **Cross-Finger Matching**
   Guo et al. "Unveiling intra-person fingerprint similarity via deep contrastive learning"
   *Science Advances* (2024) — [DOI](https://www.science.org/doi/10.1126/sciadv.adi0329)

2. **Fuzzy Extractors**
   Dodis et al. "Fuzzy Extractors: How to Generate Strong Keys from Biometrics"
   *SIAM Journal on Computing* (2008)

3. **X-Lock Construction**
   Kurbatov et al. "Unforgettable Fuzzy Extractor: Practical Construction and Security Model"
   *IACR ePrint* (2025) — [ePrint 2025/1799](https://eprint.iacr.org/2025/1799)

4. **Poseidon Hash**
   Grassi et al. "Poseidon: A New Hash Function for Zero-Knowledge Proof Systems"
   *USENIX Security* (2021)

---

## Team

Built for the [Solana Privacy Hackathon](https://solana.com/privacyhack#resources) by:

- **[@STCisGOOD](https://github.com/STCisGOOD)** — Building systems that amplify individual expression and cultural evolution.

---

## License

MIT License. See [LICENSE](./LICENSE).

