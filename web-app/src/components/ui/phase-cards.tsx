import { useState, useEffect, useRef } from "react"
import { motion, useMotionValue, useTransform, AnimatePresence } from "framer-motion"

const phases = [
  {
    id: 1,
    name: "CAPTURE",
    file: "dermagraphd/src/xlock_auth.rs:85-106",
    code: `async fn capture_fingerprint(
    sensor: &mut Sensor,
    finger_name: &str,
) -> Result<CapturedImage> {
    info!("Waiting for {} finger...", finger_name);

    let image = sensor.capture().await
        .with_context(|| format!(
            "Failed to capture {} finger", finger_name
        ))?;

    info!("{} captured (quality: {})",
          finger_name, image.quality);

    Ok(CapturedImage {
        data: image.data,
        width: image.width as u32,
        height: image.height as u32,
        quality: image.quality,
    })
}`,
    explanation: `Your fingerprint is captured by a hardware sensor (R503 or FPC1020A). The raw 192×192 grayscale image stays on your device and never leaves. Quality is checked to ensure a good scan before proceeding.`,
  },
  {
    id: 2,
    name: "EMBED",
    file: "biometric-extract/contrastive/embedder.rs:111-137",
    code: `pub fn forward(
    &self,
    images: Tensor<B, 4>,
    classical: Tensor<B, 2>,
) -> Tensor<B, 2> {
    let cnn_features = self.backbone.forward(images);

    let classical_encoded = self.classical_encoder.forward(classical);
    let classical_encoded = relu(classical_encoded);

    let fused = Tensor::cat(vec![cnn_features, classical_encoded], 1);

    let fused = self.fusion.forward(fused);
    let fused = relu(fused);

    let proj = self.proj1.forward(fused);
    let proj = relu(proj);
    let embedding = self.proj2.forward(proj);

    l2_normalize(embedding)
}`,
    explanation: `A cross-finger CNN (ResNet18 backbone) transforms your fingerprint into a 128-dimensional embedding. Different fingers from the same person map to nearby points in embedding space, enabling any-finger verification after 3-finger enrollment.`,
  },
  {
    id: 3,
    name: "LOCK",
    file: "biometric-extract/contrastive/fuzzy_extractor.rs:404-485",
    code: `fn gen_internal(
    &self,
    biometric: &[bool],
    additional_entropy: Option<&[u8]>,
    existing_beta: Option<&[bool]>,
) -> Result<(HelperData, [u8; 32]), XLockError> {
    let mut rng = ChaCha20Rng::from_entropy();
    let beta: Vec<bool> = match existing_beta {
        Some(b) => b.to_vec(),
        None => {
            let mut fresh_beta = vec![false; self.config.entropy_bits];
            for bit in &mut fresh_beta { *bit = rng.gen(); }
            fresh_beta
        }
    };

    for i in 0..self.config.entropy_bits {
        for j in 0..self.config.lockers_per_bit {
            let mut locker = false;
            for &idx in &indices[i][j] {
                locker ^= biometric[idx as usize];
            }
            vault[i][j] = locker ^ beta[i];
        }
    }

    let secret_key = self.derive_key(&beta, additional_entropy);
    Ok((helper_data, secret_key))
}`,
    explanation: `X-Lock fuzzy extractor creates 15 "lockers" per entropy bit using XOR operations. Multi-finger enrollment reuses the same β across all 3 fingers, so any finger recovers the same secret. Tolerates ~5% bit-flip noise via majority voting.`,
  },
  {
    id: 4,
    name: "DERIVE",
    file: "biometric-extract/contrastive/fuzzy_extractor.rs:987-1000",
    code: `pub fn derive_scoped_nullifier(
    key: &[u8; 32],
    scope: &str,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"dermagraph-scoped-nullifier-v1");
    hasher.update(key);
    hasher.update(scope.as_bytes());

    let hash = hasher.finalize();
    let mut nullifier = [0u8; 32];
    nullifier.copy_from_slice(&hash);
    nullifier
}`,
    explanation: `Your biometric key is combined with a "scope" string (like "dao:proposal:42") to create a unique nullifier. Same person + same scope = same nullifier (prevents double-voting). Different scopes = completely unlinkable identifiers.`,
  },
  {
    id: 5,
    name: "PROVE",
    file: "noir-witness/src/person_witness.rs:27-45",
    code: `
pub struct PersonCircuitWitness {
    pub embedding: CircuitEmbedding,
    pub blinding: Fr,
    pub merkle_path: Vec<Fr>,
    pub merkle_indices: Vec<bool>,
    pub commitment: Fr,
    pub merkle_root: Fr,
    pub scope: Fr,
    pub nullifier: Fr,
}`,
    explanation: `A zero-knowledge proof is generated locally. The witness includes the quantized embedding (private), Merkle proof of registration, and scope. The circuit verifies nullifier derivation WITHOUT revealing identity or biometric data.`,
  },
  {
    id: 6,
    name: "VERIFY",
    file: "solana/dermagraph-verifier/src/lib.rs:47-80",
    code: `
pub fn authenticate(
    ctx: Context<Authenticate>,
    proof: Vec<u8>,
    nullifier: [u8; 32],
    scope_hash: [u8; 32],
    merkle_root: [u8; 32],
) -> Result<()> {
    require!(
        registry.merkle_root == merkle_root,
        DermagraphError::InvalidMerkleRoot
    );

    require!(
        !nullifier_account.is_used,
        DermagraphError::NullifierAlreadyUsed
    );

    verify_proof(&proof, &nullifier, ..)?;

    nullifier_account.is_used = true;
    nullifier_account.used_at = Clock::get()?
        .unix_timestamp;

    Ok(())
}`,
    explanation: `The Solana smart contract verifies the ZK proof against the registry's Merkle root and checks that the nullifier hasn't been used. If valid, the nullifier is marked as "spent", preventing double-voting while maintaining complete anonymity.`,
  },
]

function useTypingAnimation(code: string, key: number, speed: number = 8) {
  const [displayedCode, setDisplayedCode] = useState("")
  const indexRef = useRef(0)

  useEffect(() => {
    setDisplayedCode("")
    indexRef.current = 0

    const interval = setInterval(() => {
      if (indexRef.current < code.length) {
        setDisplayedCode(code.slice(0, indexRef.current + 1))
        indexRef.current++
      } else {
        clearInterval(interval)
      }
    }, speed)

    return () => clearInterval(interval)
  }, [code, key, speed])

  return displayedCode
}

export function PhaseCards() {
  const [cards, setCards] = useState(phases)
  const [dragDirection, setDragDirection] = useState<'left' | 'right' | null>(null)
  const [showInfo, setShowInfo] = useState(false)

  const dragX = useMotionValue(0)
  const rotateY = useTransform(dragX, [-200, 0, 200], [-15, 0, 15])

  const frontCard = cards[0]
  const displayedCode = useTypingAnimation(frontCard.code, frontCard.id, 8)

  const offset = 3
  const scaleStep = 0.04
  const dimStep = 0.12
  const swipeThreshold = 50

  const spring = {
    type: 'spring' as const,
    stiffness: 200,
    damping: 28
  }

  const moveToEnd = () => {
    setCards(prev => [...prev.slice(1), prev[0]])
  }

  const moveToStart = () => {
    setCards(prev => [prev[prev.length - 1], ...prev.slice(0, -1)])
  }

  const handleDragEnd = (_: any, info: any) => {
    const velocity = info.velocity.x
    const offsetX = info.offset.x

    if (Math.abs(offsetX) > swipeThreshold || Math.abs(velocity) > 500) {
      if (offsetX < 0 || velocity < 0) {
        setDragDirection('left')
        setTimeout(() => {
          moveToEnd()
          setDragDirection(null)
        }, 150)
      } else {
        setDragDirection('right')
        setTimeout(() => {
          moveToStart()
          setDragDirection(null)
        }, 150)
      }
    }
    dragX.set(0)
  }

  return (
    <div className="relative w-full h-full flex items-center justify-center overflow-hidden">
      {}
      <div className="relative w-[85%] h-[85%]">
        <AnimatePresence>
          {cards.map((phase, i) => {
            const isFront = i === 0
            const brightness = Math.max(0.4, 1 - i * dimStep)
            const baseZ = cards.length - i

            return (
              <motion.div
                key={phase.id}
                className="absolute inset-0 rounded-xl border border-white/10 overflow-hidden"
                style={{
                  cursor: isFront ? 'grab' : 'default',
                  touchAction: 'none',
                  rotateY: isFront ? rotateY : 0,
                  transformPerspective: 1000,
                  background: 'rgba(0, 0, 0, 0.95)',
                  boxShadow: isFront
                    ? '0 0 60px rgba(236, 78, 2, 0.15), 0 0 120px rgba(236, 78, 2, 0.05)'
                    : 'none',
                }}
                animate={{
                  right: `${i * -offset}%`,
                  scale: 1 - i * scaleStep,
                  filter: `brightness(${brightness})`,
                  zIndex: baseZ,
                  opacity: dragDirection && isFront ? 0 : 1,
                }}
                exit={{
                  opacity: 0,
                  scale: 0.9,
                  transition: { duration: 0.2 }
                }}
                transition={spring}
                drag={isFront ? 'x' : false}
                dragConstraints={{ left: 0, right: 0 }}
                dragElastic={0.7}
                onDrag={(_, info) => {
                  if (isFront) {
                    dragX.set(info.offset.x)
                  }
                }}
                onDragEnd={isFront ? handleDragEnd : undefined}
                whileDrag={
                  isFront
                    ? {
                        zIndex: cards.length + 1,
                        cursor: 'grabbing',
                        scale: 1.02,
                      }
                    : {}
                }
                onHoverStart={() => isFront && setShowInfo(true)}
                onHoverEnd={() => setShowInfo(false)}
              >
                {}
                <div className="h-full flex flex-col p-8">
                  {}
                  <div className="mb-6">
                    <span className="font-mono text-xs text-white/30 uppercase tracking-wider">
                      Phase {phase.id} of {phases.length}
                    </span>
                    <h2 className="font-mono text-3xl font-bold text-white mt-2 mb-3">
                      {phase.name}
                    </h2>
                    <span className="font-mono text-xs text-white/20">
                      {phase.file}
                    </span>
                  </div>

                  {}
                  <div className="flex-1 relative rounded-lg border border-white/5 overflow-hidden">
                    <pre className="p-6 font-mono text-xs leading-relaxed text-white/60 overflow-auto h-full">
                      <code>
                        {isFront ? displayedCode : phase.code}
                        {isFront && <span className="animate-pulse text-white/30">▌</span>}
                      </code>
                    </pre>

                    {}
                    <motion.div
                      className="absolute inset-0 bg-black/95 flex items-center justify-center p-12"
                      initial={{ opacity: 0 }}
                      animate={{ opacity: isFront && showInfo ? 1 : 0 }}
                      transition={{ duration: 0.2 }}
                      style={{ pointerEvents: isFront && showInfo ? 'auto' : 'none' }}
                    >
                      <div className="max-w-lg text-center">
                        <p className="font-mono text-[10px] text-white/30 uppercase tracking-widest mb-4">
                          What this does
                        </p>
                        <p className="text-sm text-white/80 leading-relaxed">
                          {phase.explanation}
                        </p>
                      </div>
                    </motion.div>
                  </div>
                </div>
              </motion.div>
            )
          })}
        </AnimatePresence>
      </div>

      {}
      <div className="absolute bottom-6 left-1/2 -translate-x-1/2 flex gap-2">
        {phases.map((phase) => (
          <div
            key={phase.id}
            className={`h-1 rounded-full transition-all duration-300 ${
              phase.id === cards[0].id
                ? 'bg-white/60 w-6'
                : 'bg-white/20 w-1'
            }`}
          />
        ))}
      </div>

      {}
      <div className="absolute bottom-6 right-8 font-mono text-[10px] text-white/20 uppercase tracking-wider">
        Drag to navigate • Hover for details
      </div>
    </div>
  )
}

export function PhaseCardsInline() {
  return <PhaseCards />
}
