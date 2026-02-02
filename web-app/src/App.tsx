import { useState, useEffect, useMemo, useCallback } from "react"
import { PrivyProvider, usePrivy } from "@privy-io/react-auth"
import { useSolanaWallets } from "@privy-io/react-auth/solana"
import { PublicKey } from "@solana/web3.js"
import { GrainGradient } from "@paper-design/shaders-react"

import { HeroBiometric } from "@/components/ui/hero-biometric"
import { ProposalCard, type Proposal } from "@/components/ui/proposal-card"
import { VotingModal } from "@/components/ui/voting-modal"
import { HowItWorksModal } from "@/components/ui/how-it-works-modal"
import { TextScramble } from "@/components/ui/text-scramble"
import { HelpCircle, Github, Twitter } from "lucide-react"
import {
  connection,
  buildRegisterHumanTransaction,
  buildCastVoteTransaction,
  buildCastVoteWithProofTransaction,
  isVerifiedHuman,
  hasVotedOnProposal,
  getProposalPda,
  getDaoPda,
  DAO_AUTHORITY,
  hexToBytes,
  scopeToBytes,
  type VoteChoice,
} from "@/lib/solana-client"

const DAEMON_URL = import.meta.env.VITE_DAEMON_URL || "http://localhost:3001"
const USE_REAL_SOLANA = import.meta.env.VITE_USE_REAL_SOLANA === "true"
const PRIVY_APP_ID = import.meta.env.VITE_PRIVY_APP_ID

if (!PRIVY_APP_ID) {
  throw new Error("Missing VITE_PRIVY_APP_ID - see .env.example")
}
interface AuthResponse {
  success: boolean
  data?: {
    verified: boolean
    nullifier: string | null
    matched_finger: string | null
    confidence: number | null
  }
  error?: string
}

const DEMO_PROPOSALS: Proposal[] = [
  {
    id: 0,
    title: "Fund ZK Research",
    description:
      "Allocate 10,000 tokens for zero-knowledge cryptography research to advance privacy-preserving voting.",
    yesVotes: 142,
    noVotes: 23,
    status: "active",
  },
  {
    id: 1,
    title: "Community Treasury",
    description:
      "Create a community-managed treasury for ecosystem grants and developer incentives.",
    yesVotes: 89,
    noVotes: 67,
    status: "active",
  },
  {
    id: 2,
    title: "Protocol Upgrade v2",
    description:
      "Implement biometric verification for all governance votes using Noir ZK proofs on Solana.",
    yesVotes: 201,
    noVotes: 45,
    status: "active",
  },
]

export default function App() {
  return (
    <PrivyProvider
      appId={PRIVY_APP_ID}
      config={{
        appearance: {
          theme: "dark",
          accentColor: "#EC4E02",
          logo: "/dermagraph-logo.png",
        },
        loginMethods: ["email", "wallet", "google", "twitter"],
        embeddedWallets: {
          createOnLogin: "off",
        },
        solanaClusters: [
          { name: "devnet", rpcUrl: "https://api.devnet.solana.com" },
        ],
      }}
    >
      <DaoVotingApp />
    </PrivyProvider>
  )
}

function DaoVotingApp() {
  const { login, logout, authenticated, ready: privyReady, user } = usePrivy()
  const { wallets: solanaWallets, createWallet, ready: solanaReady } = useSolanaWallets()

  const [proposals, setProposals] = useState<Proposal[]>(DEMO_PROPOSALS)
  const [selectedProposal, setSelectedProposal] = useState<Proposal | null>(null)
  const [votingState, setVotingState] = useState<
    "idle" | "scanning" | "proving" | "submitting" | "success" | "error" | "rejected"
  >("idle")
  const [noirProof, setNoirProof] = useState<string | null>(null)
  const [noirCommitment, setNoirCommitment] = useState<string | null>(null)
  const [nullifier, setNullifier] = useState<string | null>(null)
  const [errorMessage, setErrorMessage] = useState<string>("")
  const [usedNullifiers, setUsedNullifiers] = useState<Set<string>>(new Set())
  const [isRegistered, setIsRegistered] = useState(false)
  const [txSignature, setTxSignature] = useState<string | null>(null)
  const [showHowItWorks, setShowHowItWorks] = useState(false)
  const [walletCreating, setWalletCreating] = useState(false)
  const [walletError, setWalletError] = useState<string | null>(null)

  const [currentFinger, setCurrentFinger] = useState(0)
  const [enrollmentPhase, setEnrollmentPhase] = useState<"idle" | "capturing" | "captured" | "lift" | "processing" | "complete">("idle")
  const [processingProgress, setProcessingProgress] = useState(0)
  const [processingStep, setProcessingStep] = useState("")

  const solanaWallet = useMemo(() => solanaWallets[0], [solanaWallets, solanaReady])

  useEffect(() => {
    if (authenticated && solanaReady && solanaWallets.length === 0 && !walletCreating && !walletError) {
      setWalletCreating(true)
      createWallet()
        .then(() => {
          setWalletCreating(false)
          setWalletError(null)
        })
        .catch((err: Error) => {
          setWalletCreating(false)
          setWalletError(err.message || "Failed to create wallet")
        })
    }
  }, [authenticated, solanaReady, solanaWallets.length, createWallet, walletCreating, walletError])

  useEffect(() => {
    checkRegistration()
  }, [])

  const checkRegistration = useCallback(async () => {
    try {
      const response = await fetch(`${DAEMON_URL}/status`)
      const data = await response.json()
      const daemonRegistered = data.data?.registered === true
      setIsRegistered(daemonRegistered)

      if (USE_REAL_SOLANA && solanaWallet && daemonRegistered) {
        const walletPubkey = new PublicKey(solanaWallet.address)
        await isVerifiedHuman(walletPubkey)
      }
    } catch {
    }
  }, [solanaWallet])

  const handleVote = async (choice: "yes" | "no" | "abstain") => {
    if (!selectedProposal) return

    setVotingState("scanning")
    setErrorMessage("")
    setNullifier(null)
    setNoirProof(null)
    setNoirCommitment(null)
    setTxSignature(null)

    try {
      const scope = `dao:proposal:${selectedProposal.id}`
      const response = await fetch(`${DAEMON_URL}/verify-finger`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          scope,
          passphrase: null,
        }),
      })

      const data: AuthResponse = await response.json()

      if (!data.success || !data.data) {
        throw new Error(data.error || "Authentication failed")
      }

      if (!data.data.verified || !data.data.nullifier) {
        throw new Error("Fingerprint verification failed - try again")
      }

      const xlockNullifier = data.data.nullifier
      let receivedNullifier = xlockNullifier
      let generatedProof: string | null = null
      let generatedCommitment: string | null = null
      let generatedMerkleRoot: string | null = null

      setVotingState("proving")

      try {
        const proveResponse = await fetch(`${DAEMON_URL}/prove-person`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ scope }),
        })

        const proveData = await proveResponse.json()

        if (proveData.success && proveData.data) {
          const { proof, nullifier: noirNullifier, merkle_root, commitment } = proveData.data

          if (proof && proof.length > 10) {
            generatedProof = proof
            generatedCommitment = commitment
            generatedMerkleRoot = merkle_root
            receivedNullifier = noirNullifier || xlockNullifier
            setNoirProof(proof)
            setNoirCommitment(commitment)
          } else {
            if (noirNullifier) {
              receivedNullifier = noirNullifier
            }
          }
        }
      } catch {
      }

      setNullifier(receivedNullifier)

      const voteKey = `${selectedProposal.id}:${receivedNullifier}`
      if (usedNullifiers.has(voteKey)) {
        setVotingState("rejected")
        setErrorMessage("You already voted on this proposal.")
        return
      }

      if (USE_REAL_SOLANA && solanaWallet) {
        const [daoPda] = getDaoPda(DAO_AUTHORITY)
        const [proposalPda] = getProposalPda(daoPda, selectedProposal.id)
        const alreadyVoted = await hasVotedOnProposal(proposalPda, receivedNullifier)
        if (alreadyVoted) {
          setVotingState("rejected")
          setErrorMessage("This biometric has already voted on this proposal (on-chain).")
          return
        }
      }

      setVotingState("submitting")

      if (USE_REAL_SOLANA && solanaWallet) {
        const walletPubkey = new PublicKey(solanaWallet.address)
        let transaction

        if (generatedProof && generatedCommitment && generatedMerkleRoot) {
          const proofBytes = hexToBytes(generatedProof)
          const nullifierBytes = hexToBytes(receivedNullifier)
          const commitmentBytes = hexToBytes(generatedCommitment)
          const merkleRootBytes = hexToBytes(generatedMerkleRoot)
          const scopeBytes = scopeToBytes(scope)

          transaction = buildCastVoteWithProofTransaction(
            walletPubkey,
            DAO_AUTHORITY,
            selectedProposal.id,
            proofBytes,
            nullifierBytes,
            commitmentBytes,
            merkleRootBytes,
            scopeBytes,
            choice as VoteChoice
          )
        } else {
          transaction = buildCastVoteTransaction(
            walletPubkey,
            DAO_AUTHORITY,
            selectedProposal.id,
            receivedNullifier,
            choice as VoteChoice
          )
        }

        const { blockhash } = await connection.getLatestBlockhash()
        transaction.recentBlockhash = blockhash

        const signedTx = await solanaWallet.signTransaction(transaction)
        const signature = await connection.sendRawTransaction(signedTx.serialize())
        await connection.confirmTransaction(signature, "confirmed")

        setTxSignature(signature)
      } else {
        await new Promise((resolve) => setTimeout(resolve, 2000))
      }

      setUsedNullifiers((prev) => new Set(prev).add(`${selectedProposal.id}:${receivedNullifier}`))

      setProposals((prev) =>
        prev.map((p) => {
          if (p.id === selectedProposal.id) {
            return {
              ...p,
              yesVotes: choice === "yes" ? p.yesVotes + 1 : p.yesVotes,
              noVotes: choice === "no" ? p.noVotes + 1 : p.noVotes,
            }
          }
          return p
        })
      )

      setVotingState("success")
    } catch (e: any) {
      setVotingState("error")
      setErrorMessage(e.message || "Failed to cast vote")
    }
  }

  const handleRegister = async () => {
    setVotingState("scanning")
    setErrorMessage("")
    setTxSignature(null)
    setCurrentFinger(0)
    setEnrollmentPhase("capturing")

    const enrollmentScope = "dermagraph:identity:v1"

    try {
      const enrollmentResult = await new Promise<{ nullifier: string; similarity: number }>((resolve, reject) => {
        const url = `${DAEMON_URL}/enroll-fingers-stream?scope=${encodeURIComponent(enrollmentScope)}`
        const eventSource = new EventSource(url)

        eventSource.onmessage = (event) => {
          try {
            const data = JSON.parse(event.data)

            switch (data.event) {
              case "ready":
                const fingerIndex = data.data.finger === "thumb" ? 0
                  : data.data.finger === "index" ? 1
                  : data.data.finger === "middle" ? 2 : 0
                setCurrentFinger(fingerIndex)
                setEnrollmentPhase("capturing")
                break

              case "captured":
                setEnrollmentPhase("captured")
                break

              case "lift":
                setEnrollmentPhase("lift")
                break

              case "processing":
                setEnrollmentPhase("processing")
                if (data.data?.percent !== undefined) {
                  setProcessingProgress(data.data.percent)
                }
                if (data.data?.step) {
                  setProcessingStep(data.data.step)
                }
                break

              case "complete":
                eventSource.close()
                resolve({
                  nullifier: data.data.nullifier,
                  similarity: data.data.similarity
                })
                break

              case "error":
                eventSource.close()
                reject(new Error(data.data.message || "Enrollment failed"))
                break
            }
          } catch {
          }
        }

        eventSource.onerror = async () => {
          eventSource.close()
          try {
            const statusRes = await fetch(`${DAEMON_URL}/status`)
            const statusData = await statusRes.json()
            if (statusData.data?.registered) {
              resolve({ nullifier: "recovered", similarity: 0 })
              return
            }
          } catch {
          }
          reject(new Error("Connection to fingerprint scanner lost"))
        }

        setTimeout(async () => {
          eventSource.close()
          try {
            const statusRes = await fetch(`${DAEMON_URL}/status`)
            const statusData = await statusRes.json()
            if (statusData.data?.registered) {
              resolve({ nullifier: "recovered", similarity: 0 })
              return
            }
          } catch {
          }
          reject(new Error("Enrollment timed out - please try again"))
        }, 180000)
      })

      if (USE_REAL_SOLANA && solanaWallet && enrollmentResult.nullifier) {
        setVotingState("submitting")

        const walletPubkey = new PublicKey(solanaWallet.address)
        const transaction = buildRegisterHumanTransaction(
          walletPubkey,
          enrollmentResult.nullifier
        )

        const { blockhash } = await connection.getLatestBlockhash()
        transaction.recentBlockhash = blockhash

        const signedTx = await solanaWallet.signTransaction(transaction)
        const signature = await connection.sendRawTransaction(signedTx.serialize())
        await connection.confirmTransaction(signature, "confirmed")

        setTxSignature(signature)
      }

      setEnrollmentPhase("complete")
      setIsRegistered(true)
      setVotingState("idle")
    } catch (e: any) {
      setVotingState("error")
      setEnrollmentPhase("idle")
      setErrorMessage(e.message || "Registration failed")
    }
  }

  return (
    <div className="min-h-screen bg-background noise relative">
      {}
      <div className="fixed inset-0 z-0 pointer-events-none opacity-60">
        <GrainGradient
          style={{ width: "100%", height: "100%" }}
          colors={["#EC4E02", "#ff6b35", "#1a1a2e", "#0f0f23"]}
          colorBack="#0a0a0a"
          softness={0.6}
          intensity={0.3}
          noise={0.15}
          shape="corners"
          speed={0.3}
        />
      </div>

      {}
      <header className="sticky top-0 z-50 w-full backdrop-blur-xl bg-background/70 border-b border-white/5">
        <div className="container mx-auto flex h-20 items-center px-6 relative">

          {}
          <button
            onClick={() => setShowHowItWorks(true)}
            className="absolute left-1/2 -translate-x-1/2 hidden md:flex items-center h-10 px-6 rounded-full overflow-hidden bg-zinc-900 transition-all duration-200 group"
          >
            {}
            <div className="absolute inset-0 bg-gradient-to-r from-zinc-700 via-orange-600 to-zinc-700 opacity-50 group-hover:opacity-80 blur-sm transition-opacity duration-500" />

            {}
            <div className="relative flex items-center justify-center gap-2">
              <HelpCircle className="h-4 w-4 text-white" />
              <span className="text-white font-mono text-sm">How it works</span>
            </div>
          </button>

          {}
          <div className="absolute right-6 flex items-center gap-3">
            {!privyReady ? (
              <div className="text-zinc-500 text-sm">Loading...</div>
            ) : authenticated ? (
              <div className="flex items-center gap-3">
                {solanaWallet ? (
                  <a
                    href={`https://orbmarkets.io/address/${solanaWallet.address}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-zinc-400 text-xs font-mono bg-zinc-900 px-3 py-1.5 rounded-full border border-green-500/30 hover:border-green-400/50 hover:text-green-300 transition-all duration-200 cursor-pointer"
                  >
                    <span className="text-green-400">◉</span>{" "}
                    {solanaWallet.address.slice(0, 4)}...{solanaWallet.address.slice(-4)}
                  </a>
                ) : walletError ? (
                  <div
                    className="text-red-400 text-xs font-mono bg-zinc-900 px-3 py-1.5 rounded-full border border-red-500/30 cursor-pointer"
                    onClick={() => setWalletError(null)}
                    title="Click to retry"
                  >
                    ⚠ Wallet error
                  </div>
                ) : walletCreating ? (
                  <div className="text-zinc-400 text-xs font-mono bg-zinc-900 px-3 py-1.5 rounded-full border border-orange-500/30 animate-pulse">
                    Creating wallet...
                  </div>
                ) : !solanaReady ? (
                  <div className="text-zinc-400 text-xs font-mono bg-zinc-900 px-3 py-1.5 rounded-full border border-zinc-500/30">
                    Initializing...
                  </div>
                ) : null}
                <button
                  onClick={logout}
                  className="text-zinc-400 hover:text-white text-sm transition-colors"
                >
                  Logout
                </button>
              </div>
            ) : (
              <button
                onClick={login}
                className="relative flex items-center gap-2 h-10 px-6 rounded-full overflow-hidden bg-zinc-900/80 border border-orange-500/30 transition-all duration-300 group hover:border-orange-500/60 hover:scale-[1.02]"
              >
                {}
                <div className="absolute inset-0 bg-gradient-to-r from-orange-600/0 via-orange-500/40 to-orange-600/0 opacity-0 group-hover:opacity-100 blur-sm transition-opacity duration-500 animate-pulse" />
                {}
                <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white/5 to-transparent -translate-x-full group-hover:translate-x-full transition-transform duration-1000" />
                {}
                <div className="relative flex items-center gap-2">
                  <svg className="w-4 h-4 text-orange-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M19 21V5a2 2 0 0 0-2-2H7a2 2 0 0 0-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v5m-4 0h4"/>
                  </svg>
                  <span className="text-white font-mono text-sm tracking-wide">Connect_Wallet</span>
                </div>
              </button>
            )}
          </div>
        </div>
      </header>

      {}
      <main className="container mx-auto px-4 pb-16 relative z-10">
        {}
        <HeroBiometric
          onRegister={handleRegister}
          isRegistered={isRegistered}
          isScanning={votingState === "scanning" && !selectedProposal}
          currentFinger={currentFinger}
          enrollmentPhase={enrollmentPhase}
          processingProgress={processingProgress}
          processingStep={processingStep}
        />

        {}
        <section className="py-8">
          <div className="flex items-center justify-between mb-8 border-b border-border pb-4">
            <div>
              <h2 className="font-mono text-xl font-bold uppercase tracking-wider">Active_Proposals</h2>
              <p className="font-sans text-muted-foreground text-sm mt-1">
                Cast your vote with biometric verification
              </p>
            </div>
            <div className="font-mono text-xs text-primary uppercase">
              [<TextScramble text={`${proposals.filter((p) => p.status === "active").length}_Active`} duration={1200} />]
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {proposals.map((proposal) => (
              <ProposalCard
                key={proposal.id}
                proposal={proposal}
                onSelect={() => setSelectedProposal(proposal)}
                isSelected={selectedProposal?.id === proposal.id}
              />
            ))}
          </div>
        </section>
      </main>

      {}
      <footer className="border-t border-border py-6 relative z-10">
        <div className="container mx-auto px-4 flex items-center justify-between">
          <p className="font-mono text-xs uppercase tracking-wider">
            <span className="text-primary">Dermagraph</span>
            <span className="text-muted-foreground">
          </p>
          {}
          <div className="flex items-center gap-4">
            <a
              href="https://github.com/STCisGOOD"
              target="_blank"
              rel="noopener noreferrer"
              className="text-muted-foreground hover:text-primary transition-colors"
            >
              <Github className="h-5 w-5" />
            </a>
            <a
              href="https://x.com/stcisgood"
              target="_blank"
              rel="noopener noreferrer"
              className="text-muted-foreground hover:text-primary transition-colors"
            >
              <Twitter className="h-5 w-5" />
            </a>
          </div>
          <div className="flex items-center gap-4 font-mono text-xs text-muted-foreground">
            <span>ZK_Proofs</span>
            <span className="text-border">|</span>
            <span>Sybil_Resistant</span>
            <span className="text-border">|</span>
            <span className="text-primary">On_Chain</span>
          </div>
        </div>
      </footer>

      {}
      {selectedProposal && (
        <VotingModal
          proposal={selectedProposal}
          votingState={votingState}
          nullifier={nullifier}
          errorMessage={errorMessage}
          onVote={handleVote}
          onClose={() => {
            setSelectedProposal(null)
            setVotingState("idle")
            setNullifier(null)
            setNoirProof(null)
            setNoirCommitment(null)
            setTxSignature(null)
          }}
          isRegistered={isRegistered}
          txSignature={txSignature}
          noirProof={noirProof}
        />
      )}

      {}
      <HowItWorksModal
        isOpen={showHowItWorks}
        onClose={() => setShowHowItWorks(false)}
      />
    </div>
  )
}
