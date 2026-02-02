import { motion, AnimatePresence } from "framer-motion"
import { X, Check, AlertTriangle, Copy, Fingerprint, ExternalLink } from "lucide-react"
import { useState } from "react"
import { Button } from "./button"
import { GrainGradient } from "@paper-design/shaders-react"
import type { Proposal } from "./proposal-card"

const glowPulseStyle = `
  @keyframes glowPulse {
    0%, 100% {
      filter: invert(1) contrast(1.3) brightness(0.6)
        drop-shadow(0 0 10px rgba(236, 78, 2, 0.6))
        drop-shadow(0 0 20px rgba(236, 78, 2, 0.3))
        drop-shadow(0 4px 8px rgba(0, 0, 0, 0.5));
    }
    50% {
      filter: invert(1) contrast(1.3) brightness(0.75)
        drop-shadow(0 0 18px rgba(236, 78, 2, 0.8))
        drop-shadow(0 0 36px rgba(236, 78, 2, 0.4))
        drop-shadow(0 4px 8px rgba(0, 0, 0, 0.5));
    }
  }

  @keyframes scanLine {
    0% { top: 0%; opacity: 0; }
    10% { opacity: 1; }
    90% { opacity: 1; }
    100% { top: 100%; opacity: 0; }
  }
`

type VotingState = "idle" | "scanning" | "proving" | "submitting" | "success" | "error" | "rejected"

interface VotingModalProps {
  proposal: Proposal
  votingState: VotingState
  nullifier: string | null
  errorMessage: string
  onVote: (choice: "yes" | "no" | "abstain") => void
  onClose: () => void
  isRegistered: boolean
  txSignature?: string | null
  noirProof?: string | null
}

export function VotingModal({
  proposal,
  votingState,
  nullifier,
  errorMessage,
  onVote,
  onClose,
  isRegistered,
  txSignature,
  noirProof,
}: VotingModalProps) {
  const [copied, setCopied] = useState(false)

  const copyNullifier = () => {
    if (nullifier) {
      navigator.clipboard.writeText(nullifier)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    }
  }

  const getFingerprintState = () => {
    if (votingState === "scanning" || votingState === "proving" || votingState === "submitting") return "scanning"
    if (votingState === "success") return "success"
    if (votingState === "error" || votingState === "rejected") return "error"
    return "idle"
  }

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 z-50 flex items-center justify-center p-4"
        onClick={onClose}
      >
        {}
        <div className="absolute inset-0 bg-black/90 backdrop-blur-sm" />

        {}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 20 }}
          transition={{ duration: 0.2 }}
          onClick={(e) => e.stopPropagation()}
          className="relative w-full max-w-2xl overflow-hidden border-2 border-border bg-card"
        >
          {}
          <div className="flex items-center justify-between px-4 py-2 border-b border-border bg-muted/30">
            <span className="font-mono text-xs text-muted-foreground uppercase tracking-wider">
              Proposal_{proposal.id.toString().padStart(3, "0")}
            </span>
            <button
              onClick={onClose}
              className="p-1 hover:bg-muted transition-colors"
            >
              <X className="h-4 w-4 text-muted-foreground" />
            </button>
          </div>

          <div className="grid md:grid-cols-2">
            {}
            <div className="relative flex items-center justify-center h-64 md:h-auto md:min-h-[400px] border-b md:border-b-0 md:border-r border-border overflow-hidden bg-background">
              <style>{glowPulseStyle}</style>
            {}
              <div className="absolute inset-0 pointer-events-none">
                <GrainGradient
                  style={{ width: "100%", height: "100%" }}
                  colors={["#EC4E02", "#ff6b35", "#1a1a2e", "#0f0f23"]}
                  colorBack="#0a0a0a"
                  softness={0.6}
                  intensity={0.3}
                  noise={0.15}
                  shape="wave"
                  speed={votingState === "scanning" || votingState === "proving" || votingState === "submitting" ? 1.5 : 0.5}
                  offsetX={0.5}
                  offsetY={0.12}
                />
              </div>

              {}
              <img
                src="/fingerprint.png"
                alt="Fingerprint"
                style={{
                  position: "relative",
                  zIndex: 10,
                  width: 280,
                  height: 280,
                  objectFit: "contain",
                  transition: "transform 0.3s ease",
                  transform: votingState === "scanning" || votingState === "proving" || votingState === "submitting" ? "scale(1.02)" : "scale(1)",
                  mixBlendMode: "hard-light",
                  animation: "glowPulse 2s ease-in-out infinite",
                }}
              />

              {}
              <div className="absolute bottom-4 left-4 z-10">
                <p className="font-mono text-xs text-muted-foreground uppercase tracking-widest">
                  {votingState === "idle" && "Ready_"}
                  {votingState === "scanning" && "Scanning_"}
                  {votingState === "proving" && "ZK_Proving_"}
                  {votingState === "submitting" && "Submitting_"}
                  {votingState === "success" && "Verified_"}
                  {votingState === "error" && "Error_"}
                  {votingState === "rejected" && "Rejected_"}
                </p>
              </div>
            </div>

            {}
            <div className="p-6">
              <h2 className="font-mono text-lg font-bold uppercase mb-4">{proposal.title}</h2>

              {}
              {votingState === "idle" && (
                <>
                  {!isRegistered ? (
                    <div className="py-8">
                      <div className="w-12 h-12 border-2 border-warning flex items-center justify-center mb-4">
                        <AlertTriangle className="h-6 w-6 text-warning" />
                      </div>
                      <p className="font-sans text-muted-foreground">
                        Register biometric identity first to vote.
                      </p>
                    </div>
                  ) : (
                    <>
                      <p className="font-sans text-sm text-muted-foreground mb-6 leading-relaxed">
                        {proposal.description}
                      </p>

                      <div className="grid grid-cols-3 gap-3 mb-6">
                        {}
                        <button
                          onClick={() => onVote("yes")}
                          className="group relative p-4 border-2 border-success/50 bg-transparent hover:bg-success/10 hover:border-success transition-all duration-200"
                        >
                          <div className="flex flex-col items-center gap-3">
                            <Check className="h-6 w-6 text-success" />
                            <span className="font-mono text-xs uppercase tracking-wider text-success">
                              Yes_
                            </span>
                          </div>
                        </button>

                        {}
                        <button
                          onClick={() => onVote("no")}
                          className="group relative p-4 border-2 border-destructive/50 bg-transparent hover:bg-destructive/10 hover:border-destructive transition-all duration-200"
                        >
                          <div className="flex flex-col items-center gap-3">
                            <X className="h-6 w-6 text-destructive" />
                            <span className="font-mono text-xs uppercase tracking-wider text-destructive">
                              No_
                            </span>
                          </div>
                        </button>

                        {}
                        <button
                          onClick={() => onVote("abstain")}
                          className="group relative p-4 border-2 border-muted-foreground/30 bg-transparent hover:bg-muted/20 hover:border-muted-foreground/60 transition-all duration-200"
                        >
                          <div className="flex flex-col items-center gap-3">
                            <span className="text-muted-foreground text-xl">○</span>
                            <span className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
                              Abstain_
                            </span>
                          </div>
                        </button>
                      </div>

                      <p className="font-mono text-xs text-muted-foreground uppercase">
                        Fingerprint_Required
                      </p>
                    </>
                  )}
                </>
              )}

              {votingState === "scanning" && (
                <div className="py-8">
                  <div className="relative w-16 h-16 border-2 border-primary bg-primary/5 flex items-center justify-center mb-6 wire-glow overflow-hidden">
                    <Fingerprint className="w-8 h-8 text-primary" />
                    {}
                    <div
                      className="absolute left-0 right-0 h-1 bg-gradient-to-b from-transparent via-primary to-transparent"
                      style={{ animation: 'scanLine 1.5s ease-in-out infinite' }}
                    />
                  </div>
                  <h3 className="font-mono text-lg font-bold uppercase mb-2">Verifying_</h3>
                  <p className="font-sans text-sm text-muted-foreground mb-3">
                    Place any enrolled finger on sensor
                  </p>
                  <div className="space-y-1 text-left">
                    <p className="font-mono text-xs text-muted-foreground/70">
                      → Sensor captures fingerprint
                    </p>
                    <p className="font-mono text-xs text-muted-foreground/70">
                      → CNN generates embedding (~10s)
                    </p>
                    <p className="font-mono text-xs text-muted-foreground/70">
                      → X-Lock derives nullifier
                    </p>
                  </div>
                </div>
              )}

              {votingState === "proving" && (
                <div className="py-8">
                  <div className="relative w-16 h-16 border-2 border-purple-500 bg-purple-500/10 flex items-center justify-center mb-6 overflow-hidden">
                    <div className="w-8 h-8 text-purple-400 font-mono text-lg font-bold">zk</div>
                    {}
                    <div
                      className="absolute inset-0 bg-gradient-to-r from-transparent via-purple-500/30 to-transparent"
                      style={{ animation: 'scanLine 0.8s ease-in-out infinite' }}
                    />
                  </div>
                  <h3 className="font-mono text-lg font-bold uppercase mb-2 text-purple-400">Generating_Proof_</h3>
                  <p className="font-sans text-sm text-muted-foreground mb-3">
                    Creating zero-knowledge proof with Noir
                  </p>
                  <div className="space-y-1 text-left">
                    <p className="font-mono text-xs text-purple-400/70">
                      → Embedding stays private
                    </p>
                    <p className="font-mono text-xs text-purple-400/70">
                      → Poseidon nullifier derived
                    </p>
                    <p className="font-mono text-xs text-purple-400/70">
                      → Groth16 proof generated
                    </p>
                  </div>
                </div>
              )}

              {votingState === "submitting" && (
                <div className="py-8">
                  <div className="relative w-16 h-16 border-2 border-primary bg-primary/10 flex items-center justify-center mb-6 overflow-hidden">
                    <Fingerprint className="w-8 h-8 text-primary animate-pulse" />
                    {}
                    <div
                      className="absolute left-0 right-0 h-1 bg-gradient-to-b from-transparent via-primary to-transparent"
                      style={{ animation: 'scanLine 1s ease-in-out infinite' }}
                    />
                  </div>
                  <h3 className="font-mono text-lg font-bold uppercase mb-2">Submitting_</h3>
                  <p className="font-sans text-sm text-muted-foreground mb-4">
                    Recording anonymous vote on-chain
                  </p>
                  {nullifier && (
                    <div className="p-3 border border-border bg-muted/30">
                      <span className="font-mono text-xs text-muted-foreground block mb-1">NULLIFIER:</span>
                      <code className="font-mono text-xs text-primary break-all">
                        {nullifier.slice(0, 24)}...
                      </code>
                    </div>
                  )}
                </div>
              )}

              {votingState === "success" && (
                <div className="py-8">
                  <div className="w-16 h-16 border-2 border-success bg-success/5 flex items-center justify-center mb-6">
                    <Check className="w-8 h-8 text-success" />
                  </div>
                  <h3 className="font-mono text-lg font-bold text-success uppercase mb-4">Vote_Recorded</h3>

                  <div className="space-y-2 text-sm mb-6">
                    <p className="flex items-center gap-2 font-mono text-xs">
                      <span className={`w-1.5 h-1.5 ${noirProof ? 'bg-purple-500' : 'bg-success'}`} />
                      {noirProof ? "ZK_PROVEN (NOIR)" : "ANONYMOUS_VERIFIED"}
                    </p>
                    <p className="flex items-center gap-2 font-mono text-xs">
                      <span className="w-1.5 h-1.5 bg-success" />
                      DOUBLE_VOTE_PROTECTED
                    </p>
                    <p className="flex items-center gap-2 font-mono text-xs">
                      <span className="w-1.5 h-1.5 bg-success" />
                      {txSignature ? "ON_CHAIN_CONFIRMED" : "UNLINKABLE"}
                    </p>
                    {noirProof && (
                      <p className="flex items-center gap-2 font-mono text-xs text-purple-400">
                        <span className="w-1.5 h-1.5 bg-purple-500" />
                        EMBEDDING_PRIVATE
                      </p>
                    )}
                  </div>

                  {txSignature && (
                    <div className="p-3 border border-success/30 bg-success/5 mb-4">
                      <div className="flex items-center justify-between mb-1">
                        <span className="font-mono text-xs text-success uppercase">TX_SIGNATURE:</span>
                        <a
                          href={`https://explorer.solana.com/tx/${txSignature}?cluster=devnet`}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="p-1 hover:bg-muted flex items-center gap-1 text-xs text-success"
                        >
                          <ExternalLink className="h-3 w-3" />
                          View
                        </a>
                      </div>
                      <code className="font-mono text-xs text-muted-foreground break-all">
                        {txSignature.slice(0, 32)}...
                      </code>
                    </div>
                  )}

                  {nullifier && (
                    <div className="p-3 border border-border bg-muted/30">
                      <div className="flex items-center justify-between mb-1">
                        <span className="font-mono text-xs text-muted-foreground">NULLIFIER:</span>
                        <button
                          onClick={copyNullifier}
                          className="p-1 hover:bg-muted flex items-center gap-1"
                        >
                          {copied ? (
                            <>
                              <Check className="h-3 w-3 text-success" />
                              <span className="font-mono text-xs text-success">Copied!</span>
                            </>
                          ) : (
                            <Copy className="h-3 w-3 text-muted-foreground" />
                          )}
                        </button>
                      </div>
                      <code className="font-mono text-xs text-primary break-all">
                        {nullifier}
                      </code>
                    </div>
                  )}
                </div>
              )}

              {votingState === "rejected" && (
                <div className="py-8">
                  <div className="w-16 h-16 border-2 border-warning bg-warning/5 flex items-center justify-center mb-6">
                    <AlertTriangle className="w-8 h-8 text-warning" />
                  </div>
                  <h3 className="font-mono text-lg font-bold text-warning uppercase mb-2">
                    Double_Vote_Detected
                  </h3>
                  <p className="font-sans text-sm text-muted-foreground mb-4">{errorMessage}</p>

                  <div className="p-3 border border-warning/30 bg-warning/5">
                    <p className="font-mono text-xs text-warning uppercase mb-1">
                      Sybil_Resistance_Active
                    </p>
                    <p className="font-sans text-xs text-muted-foreground">
                      Same nullifier — you already voted.
                    </p>
                  </div>
                </div>
              )}

              {votingState === "error" && (
                <div className="py-8">
                  <div className="w-16 h-16 border-2 border-destructive bg-destructive/5 flex items-center justify-center mb-6">
                    <X className="w-8 h-8 text-destructive" />
                  </div>
                  <h3 className="font-mono text-lg font-bold text-destructive uppercase mb-2">Error_</h3>
                  <p className="font-sans text-sm text-muted-foreground mb-6">{errorMessage}</p>
                  <Button onClick={onClose} variant="outline">
                    Try_Again
                  </Button>
                </div>
              )}
            </div>
          </div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  )
}
