# The Dermagraph Build Log

## Two Weeks of Development: Privacy-Preserving Biometric Identity

This document provides a technical chronicle of building Dermagraph from initial concept to working prototype during the Solana Privacy Hackathon.

---

## Week 1: Hardware & Foundations

### Days 1-2: Hardware Selection

The project began with a core constraint: build biometric identity verification without expensive specialized hardware.

**Components selected:**
- Raspberry Pi 4 (2GB) — $100
- R503 Capacitive Fingerprint Sensor — $25
- Jumper wires — $3
- USB-C power supply — $8

**Total cost: approximately $140** (compared to $500+ for alternatives like WorldCoin's Orb)

### Day 3: Hardware Assembly

The R503 sensor communicates via UART. Wiring configuration:

```
R503 Pin    Wire Color    Raspberry Pi 4
────────────────────────────────────────
VCC         Red           3.3V (Pin 1)
GND         Black         GND (Pin 6)
TX          Yellow        RX (Pin 10 / GPIO15)
RX          Green         TX (Pin 8 / GPIO14)
```

This was the first soldering work on the project. Initial attempts failed due to cold solder joints resulting in no serial communication. Third attempt established successful UART communication.

### Days 4-5: R503 Driver Development

No existing Rust libraries supported the R503. A driver was implemented from scratch based on the manufacturer's protocol documentation.

**Technical challenges addressed:**
- Baud rate negotiation (sensor defaults to 57600)
- Binary packet framing with checksums
- Image data encoding (192×192 grayscale, 36,864 bytes)
- Timing synchronization between touch detection and image capture

**Result:** Complete `r503.rs` driver implementation (359 lines)

---

## Week 1: Neural Network Development

### Days 5-6: The Cross-Finger Matching Problem

Traditional fingerprint systems treat each finger as a unique identity. This creates a vulnerability: a single user could register all ten fingers as separate identities (10-finger sybil attack).

**Solution approach:** Train a convolutional neural network that maps all fingers from the same person to similar embeddings in a 128-dimensional space, while keeping different people's embeddings distant.

```
Target behavior:
  Person A (thumb)   →  embedding_A1
  Person A (index)   →  embedding_A2  (close to A1)
  Person A (middle)  →  embedding_A3  (close to A1, A2)

  Person B (thumb)   →  embedding_B1  (distant from all A embeddings)
```

### Days 6-7: Training Dataset

No public dataset provides cross-finger pairs labeled by person identity. A custom dataset was constructed:

**Collection process:**
1. Multiple scans per finger (5 samples each)
2. Labeled by person ID and finger type
3. Positive pairs: same person, different fingers
4. Negative pairs: different persons

### Days 7-8: Model Training

**Building on Columbia University Research:**

The foundation for cross-finger matching comes from Gabe Guo et al. at Columbia University:

> "Unveiling intra-person fingerprint similarity via deep contrastive learning"
> Science Advances, 2024
> GitHub: gabeguo/FingerprintMatching

Their key finding: fingerprints from the same person share "center features" (swirl angles, curvatures in the core region) detectable by deep learning with **77% accuracy**.

**Our improvements targeting 85%+ accuracy:**

1. **Sensor-specific optimization:** Trained specifically for R503 capacitive sensor output (192×192 @ 508 DPI), rather than generic fingerprint databases
2. **Classical feature fusion:** Combined CNN features with traditional orientation and frequency maps from Gabor filter analysis
3. **Core region detection:** Implemented Poincaré index method to locate fingerprint cores, weighting center features more heavily

**Architecture:** ResNet-18 backbone with 128-dimensional embedding head

```
┌─────────────────────────────────────────────────────────────────┐
│                    FingerprintEmbedder                          │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌───────────────────┐  ┌──────────────────┐  │
│  │   ResNet18   │  │ Classical Features│  │   Core Region    │  │
│  │   Backbone   │  │ (Orientation +    │  │   Detection      │  │
│  │              │  │  Frequency maps)  │  │   (Poincare)     │  │
│  └──────┬───────┘  └────────┬──────────┘  └────────┬─────────┘  │
│         │                   │                      │            │
│         ▼                   ▼                      ▼            │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              Feature Fusion (concat → Linear → ReLU)     │   │
│  └──────────────────────────┬───────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              Projection Head → 128-dim L2-norm           │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

**Loss function:** Symmetric InfoNCE (contrastive learning)

```rust
fn symmetric_info_nce_loss(
    anchor: Tensor,      // Person A, finger X
    positive: Tensor,    // Person A, finger Y (different finger)
    negatives: Tensor,   // Other persons' fingers
    temperature: f64,
) -> Tensor
```

**Privacy-Preserving Training Integration:**

The enrollment system contributes positive pairs (same-person, different-finger) for model improvement while maintaining privacy guarantees:

- Raw fingerprint images exist in memory only during processing (~100ms)
- Images are never stored; only embeddings are temporarily retained
- Model weights contain aggregate statistical knowledge, not individual biometrics
- Each enrollment contributes training signal without exposing the underlying biometric

This enables continuous model improvement from real-world usage without building a centralized fingerprint database.

**Platform issue encountered:** On Windows, PyTorch (via tch-rs) failed to detect CUDA despite correct driver installation.

**Root cause:** The Windows PE linker removes `torch_cuda.dll` from the import table because no direct symbol references exist in the compiled binary. PyTorch uses runtime dispatch to load CUDA backends.

**Solution:** Explicitly load CUDA libraries via `LoadLibrary` before any PyTorch initialization:

```rust
#[cfg(windows)]
fn load_cuda_libraries() {
    unsafe { Library::new("torch_cuda.dll") };
    // Forces DllMain to execute, registering CUDA backend
}
```

**Training results:**
- Duration: 100 epochs, approximately 3 hours (RTX 3090)
- Final loss: 0.23
- Cross-finger accuracy: **94.6%** (vs. Columbia's 77% baseline)

The 17+ percentage point improvement over the Columbia baseline resulted from sensor-specific training and classical feature fusion.

---

## Week 2: Cryptographic Implementation

### Days 9-10: Fuzzy Extraction

Biometric measurements are inherently noisy. The same finger scanned multiple times produces slightly different embeddings. Standard cryptographic systems require exact key reproduction.

**The Biometric Entropy Problem:**

Biometrics provide surprisingly low entropy compared to cryptographic keys:

1. **Limited variation:** Fingerprints have finite minutiae patterns; the embedding space is constrained by what neural networks can learn from human fingerprints.
2. **Correlated bits:** Adjacent embedding dimensions are not independent—they encode overlapping spatial features.
3. **Unstable bits:** Some embedding dimensions fluctuate significantly between scans due to pressure, moisture, or sensor noise.
4. **Public information:** Partial fingerprint patterns may be recoverable from surfaces the user has touched.

Research estimates fingerprint entropy at 20-40 bits—far below the 128+ bits expected for cryptographic security.

Reference: Dodis et al., "Fuzzy Extractors: How to Generate Strong Keys from Biometrics," SIAM Journal on Computing.

**The XOR Locker Solution:**

Traditional fuzzy commitment schemes (binding biometrics with error-correcting codes) leak information through helper data. Statistical attacks can recover keys with minimal effort.

Reference: "Statistical Attacks on Fuzzy Commitment," IEEE (https://ieeexplore.ieee.org/document/5981720/)

We implemented the X-Lock construction from Kurbatov et al. ("Unforgettable Fuzzy Extractor," ePrint 2025/1799):

```
Enrollment:
  1. Generate random secret s (high entropy)
  2. For each bit of s, create multiple "lockers"
  3. Each locker: XOR random subset of embedding bits → stores result
  4. Helper data = locker indices + XOR results (NOT the embedding)

Verification:
  1. For each secret bit, evaluate all its lockers using new embedding
  2. Majority vote across lockers → recovers each bit of s
  3. If confidence too low → reject (prevents guessing attacks)
```

The key insight: XOR with a random subset of embedding bits produces output indistinguishable from random—unless you have an embedding with sufficient bit agreement. The helper data reveals nothing about the secret or the biometric.

**Implementation:** `fuzzy_extractor.rs` (1,622 lines)

Key components:
- Bit stability analysis across enrollment scans (identify unreliable dimensions)
- XOR-based locker generation with configurable redundancy
- Majority voting with confidence thresholds
- Optional passphrase binding for defense-in-depth

**Configuration parameters:**
- `feature_bits`: 512 (quantized embedding size)
- `entropy_bits`: 48 (extracted secret size)
- `lockers_per_bit`: 15 (redundancy for noise tolerance)
- `indices_per_locker`: 5 (bits XORed per locker)
- `min_avg_confidence`: 0.30 (rejection threshold)

**Performance metrics:**
- Intra-person similarity: 94.6%
- Bit agreement threshold: 30%
- Excluded low-margin bits: 48 of 128

### Days 10-11: Zero-Knowledge Circuit

The Noir circuit proves possession of a valid registered biometric without revealing the biometric data itself.

**Circuit: `person_identity`**

```noir
fn main(
    embedding: [Field; 32],      // Private: quantized CNN embedding
    blinding: Field,             // Private: Pedersen randomness
    merkle_proof: [Field; 20],   // Private: Merkle path

    pub commitment: Field,       // Public: Pedersen(embedding, blinding)
    pub merkle_root: Field,      // Public: identity registry root
    pub scope: Field,            // Public: application context
    pub nullifier: Field,        // Public: Poseidon(embedding, scope)
)
```

**Constraint optimization:**
- Initial implementation: 150,000 constraints
- After Poseidon optimization: 89,000 constraints
- After redundancy elimination: 45,000 constraints
- Final: 33,700 constraints

**Proving time:** Approximately 6 seconds on Raspberry Pi 4

### Days 11-12: Solana Integration

Groth16 proof verification on Solana via Sunspot Verifier CPI:

```rust
sunspot::verify(proof_bytes, public_inputs)?;
msg!("Proof verified successfully!");
```

**Issue encountered:** The `update_merkle_root` instruction failed with "InstructionFallbackNotFound".

**Root cause:** Incorrect Anchor discriminator calculation.

```
Incorrect: sha256("update_merkle_root")[0:8]
Correct:   sha256("global:update_merkle_root")[0:8]
```

Resolution required reviewing Anchor's discriminator derivation internals.

---

## Week 2: System Integration

### Days 12-13: Raspberry Pi Deployment

**Cross-compilation from Windows:**
```bash
docker run rust:latest bash -c "
  dpkg --add-architecture arm64 &&
  apt-get install gcc-aarch64-linux-gnu &&
  cargo build --release --target aarch64-unknown-linux-gnu
"
```

**Performance on Pi 4 (ARM Cortex-A72, no GPU):**
- CNN inference: 26 seconds per embedding
- Proof generation: 6 seconds

### Days 13-14: User Interface Refinement

**LED state machine for R503:**
- Breathing blue: awaiting finger placement
- Solid purple: image captured, await finger removal
- Solid blue: processing
- Green flash: operation complete

**SSE timing issue:** Server-sent events arrived after user actions due to Node.js output buffering.

Initial approach: Refactored entire sensor API to separate capture and removal detection.

Correct solution: Added `res.flush()` after SSE writes (single line fix).

The refactored code was reverted in favor of the minimal fix.

### Day 14: End-to-End Verification

First successful ZK-verified vote on Solana devnet:

```
Program log: Instruction: CastVoteWithProof
Program log: Proof verified successfully!
Program log: Groth16 proof verified successfully via Sunspot
Program log: ZK-verified vote cast! Nullifier: [19, 82, 217, ...]
```

Transaction: `38FPt6dgJahb7qtPGd4H7SNLo1qa3cEZMVKHVf4wQfHA4vF5q6eBYHdrH6BRyptR6PcERYxqcSTW34EoCz4szks3`

---

## Codebase Summary

| Component | Lines of Code | Development Time |
|-----------|---------------|------------------|
| R503 sensor driver | 359 | 2 days |
| CNN trainer (tch-rs) | 943 | 3 days |
| Fuzzy extractor | 1,622 | 2 days |
| Noir ZK circuit | 379 | 2 days |
| Daemon server | 1,281 | 3 days |
| Solana programs | 500 | 1 day |
| Web application | 2,000 | 2 days |

**Totals:** ~20,000 lines Rust, ~1,400 lines Noir, ~2,000 lines TypeScript

---

## Known Limitations

Issues identified but not resolved within the hackathon timeframe:

1. **Inference latency:** 26-second CNN inference on ARM CPU is suboptimal for production use. Potential solutions include model quantization or NPU acceleration.

2. **Liveness detection:** The current implementation cannot distinguish live fingers from high-quality replicas.

3. **Device recovery:** No mechanism exists for identity recovery if the enrollment device is lost.

4. **Multi-device support:** Identity is bound to a single device; no cross-device synchronization.

---

## Key Technical Decisions

1. **Capacitive over optical sensor:** Higher image quality, more resistant to spoofing

2. **Contrastive learning over minutiae matching:** Enables cross-finger sybil resistance

3. **Fuzzy extraction over template storage:** Biometric never stored, only helper data

4. **Noir over Circom:** Better developer experience, Rust-like syntax

5. **Sunspot over custom verifier:** Audited Groth16 verification, lower development risk

---

*Developed for the Solana Privacy Hackathon, February 2026*
