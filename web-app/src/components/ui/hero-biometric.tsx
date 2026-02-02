import { Fingerprint, ArrowRight, Check } from "lucide-react"
import { useState, useEffect } from "react"
import { Button } from "./button"
import { FingerprintWaves } from "./fingerprint-waves"
import { TextScramble } from "./text-scramble"
import { CyclingText } from "./cycling-text"
import { Dithering, GrainGradient } from "@paper-design/shaders-react"

const FINGER_NAMES = ["Thumb", "Index", "Middle"] as const
type FingerName = typeof FINGER_NAMES[number]

interface HeroBiometricProps {
  onRegister: () => void
  isRegistered: boolean
  isScanning: boolean
  currentFinger?: number
  enrollmentPhase?: "idle" | "capturing" | "captured" | "lift" | "processing" | "complete"
  processingProgress?: number
  processingStep?: string
}

export function HeroBiometric({
  onRegister,
  isRegistered,
  isScanning,
  currentFinger = 0,
  enrollmentPhase = "idle",
  processingProgress = 0,
  processingStep = ""
}: HeroBiometricProps) {
  const [isHovered, setIsHovered] = useState(false)

  const getStepLabel = (step: string): string => {
    switch (step) {
      case "loading_model": return "Loading neural network..."
      case "thumb_embedding": return "Processing thumb..."
      case "index_embedding": return "Processing index finger..."
      case "middle_embedding": return "Processing middle finger..."
      case "xlock_enrollment": return "Generating cryptographic key..."
      case "finalizing": return "Finalizing identity..."
      default: return "Processing..."
    }
  }

  const getState = () => {
    if (isScanning) return "scanning"
    if (isRegistered) return "success"
    return "idle"
  }

  const getStatusText = () => {
    if (isRegistered) return "HASH_VERIFIED"
    if (!isScanning) return "READY"

    if (enrollmentPhase === "capturing") {
      return `SCAN_${FINGER_NAMES[currentFinger].toUpperCase()}`
    }
    if (enrollmentPhase === "captured") {
      return "CAPTURED!"
    }
    if (enrollmentPhase === "lift") {
      return "LIFT_FINGER"
    }
    if (enrollmentPhase === "processing") {
      return "PROCESSING..."
    }
    return "SCANNING..."
  }

  const getDescription = () => {
    if (isRegistered) {
      return "Biometric registered. Vote on any proposal. Your identity stays private. Your vote counts once."
    }
    if (isScanning && enrollmentPhase === "capturing") {
      return `Place your ${FINGER_NAMES[currentFinger].toLowerCase()} finger on the sensor. (${currentFinger + 1} of 3)`
    }
    if (isScanning && enrollmentPhase === "captured") {
      return `${FINGER_NAMES[currentFinger]} captured!`
    }
    if (isScanning && enrollmentPhase === "lift") {
      return `Lift your ${FINGER_NAMES[currentFinger].toLowerCase()} finger from the sensor.`
    }
    if (isScanning && enrollmentPhase === "processing") {
      return "Running neural network on ARM CPU (no GPU). Each finger's embedding requires ~10 seconds of secure, local computation."
    }
    return "Register your fingerprint. Create a cryptographic proof. No personal data stored. Just math."
  }

  return (
    <section className="py-8 w-full flex flex-col items-center gap-6">
      {}
      <div
        className="w-full max-w-sm border border-black bg-card"
        onMouseEnter={() => setIsHovered(true)}
        onMouseLeave={() => setIsHovered(false)}
      >
        <div className="relative flex items-center justify-center p-6 overflow-hidden bg-background aspect-square">
          {}
          <div className="absolute inset-0 z-0 pointer-events-none opacity-40 dark:opacity-30 mix-blend-multiply dark:mix-blend-screen">
            <Dithering
              colorBack="#00000000"
              colorFront="#EC4E02"
              shape="warp"
              type="4x4"
              speed={isHovered || isScanning ? 0.6 : 0.2}
              style={{ width: "100%", height: "100%" }}
              minPixelRatio={1}
            />
          </div>

          <div className="relative z-10">
            <FingerprintWaves
              size={240}
              ridgeCount={140}
              color="#EC4E02"
              state={getState()}
            />
          </div>

          {}
          <div className="absolute top-3 left-3 z-10">
            <p className="font-mono text-xs text-primary uppercase tracking-widest">
              DERMAGRAPH_
            </p>
          </div>

          <div className="absolute bottom-3 left-3 z-10">
            <p className="font-mono text-xs text-primary">
              {getStatusText()}
            </p>
          </div>

          <div className="absolute bottom-3 right-3 z-10">
            <p className="font-mono text-xs text-muted-foreground">
              {isScanning && enrollmentPhase === "capturing"
                ? `${currentFinger + 1}/3_FINGERS`
                : "256_BIT"}
            </p>
          </div>
        </div>
      </div>

      {}
      {isScanning && (
        <div className="flex items-center justify-center gap-6">
          {FINGER_NAMES.map((finger, idx) => {
            const allComplete = enrollmentPhase === "processing" || enrollmentPhase === "complete"
            const isComplete = idx < currentFinger || allComplete
            const isCurrent = idx === currentFinger && !allComplete
            const isJustCaptured = isCurrent && (enrollmentPhase === "captured" || enrollmentPhase === "lift")

            return (
              <div key={finger} className="flex flex-col items-center gap-2">
                {}
                <div className={`
                  w-8 h-8 border-2 flex items-center justify-center
                  transition-all duration-300
                  ${isComplete || isJustCaptured
                    ? 'border-primary bg-primary/20'
                    : isCurrent
                      ? 'border-primary wire-glow'
                      : 'border-muted/30'
                  }
                `}>
                  {isComplete || isJustCaptured ? (
                    <Check className="w-4 h-4 text-primary" />
                  ) : isCurrent ? (
                    <Fingerprint className="w-4 h-4 text-primary animate-pulse" />
                  ) : (
                    <span className="font-mono text-xs text-muted-foreground/50">{idx + 1}</span>
                  )}
                </div>
                {}
                <span className={`font-mono text-[10px] uppercase tracking-wider ${
                  isComplete ? 'text-primary' : isCurrent ? 'text-primary' : 'text-muted-foreground/40'
                }`}>
                  {isComplete ? "Done!" : finger}
                </span>
              </div>
            )
          })}
        </div>
      )}

      {}
      <div className="w-full max-w-4xl flex items-center justify-between border-b border-black pb-4">
        <div>
          <h1 className="font-mono text-xl font-bold uppercase tracking-wider">
            {isRegistered ? (
              <>
                <span className="text-foreground">Identity </span>
                <span className="text-primary">Verified_</span>
              </>
            ) : isScanning ? (
              <>
                <span className="text-foreground">Multi-Finger </span>
                <span className="text-primary">Enrollment_</span>
              </>
            ) : (
              <>
                <span className="text-foreground">Proof of </span>
                <span className="text-primary">Humanity_</span>
              </>
            )}
          </h1>
          <p className="font-sans text-muted-foreground text-sm mt-1">
            {isRegistered
              ? "Your identity is verified. Vote on proposals below."
              : isScanning
                ? `Scanning finger ${currentFinger + 1} of 3...`
                : "Register your fingerprint to create a cryptographic proof"
            }
          </p>
        </div>
        <div className="font-mono text-xs text-primary uppercase">
          [<TextScramble text="ZK_Proof" duration={1200} />]
        </div>
      </div>

      {}
      <div className="w-full max-w-4xl border border-black noise" style={{ backgroundColor: 'rgba(8, 8, 8, 0.92)' }}>
        <div className="p-8 lg:p-10">
          {}
          <p className="font-sans text-muted-foreground text-lg max-w-xl mx-auto mb-8 leading-relaxed text-center italic font-medium">
            {getDescription()}
          </p>

          {}
          <div className="flex justify-center mb-8">
            {!isRegistered ? (
              <div className="flex flex-col items-center gap-3">
                <button
                  onClick={onRegister}
                  disabled={isScanning}
                  className="relative h-12 px-8 rounded-full overflow-hidden bg-zinc-900 transition-all duration-200 group disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {}
                  <div className="absolute inset-0 bg-gradient-to-r from-zinc-700 via-orange-600 to-zinc-700 opacity-50 group-hover:opacity-80 blur-sm transition-opacity duration-500" />

                  {}
                  <div className="relative flex items-center justify-center gap-3">
                    {isScanning ? (
                      <>
                        {(enrollmentPhase === "captured" || enrollmentPhase === "lift") ? (
                          <Check className="w-4 h-4 text-primary" />
                        ) : (
                          <Fingerprint className="w-4 h-4 text-white animate-pulse" />
                        )}
                        <span className="text-white font-mono text-sm uppercase tracking-wider">
                          {enrollmentPhase === "capturing"
                            ? `Scan_${FINGER_NAMES[currentFinger]}`
                            : enrollmentPhase === "captured"
                              ? "Captured!"
                              : enrollmentPhase === "lift"
                                ? "Lift_Finger!"
                                : enrollmentPhase === "processing"
                                  ? "Processing_"
                                  : "Scanning_"
                          }
                        </span>
                      </>
                    ) : (
                      <>
                        <Fingerprint className="w-4 h-4 text-white" />
                        <span className="text-white font-mono text-sm uppercase tracking-wider">Register_Biometric</span>
                        <ArrowRight className="w-4 h-4 text-white/90" />
                      </>
                    )}
                  </div>
                </button>

                {}
                {enrollmentPhase === "processing" && (
                  <div className="w-72 flex flex-col items-center gap-2">
                    {}
                    <span className="font-mono text-xs text-muted-foreground">
                      {getStepLabel(processingStep)}
                    </span>
                    {}
                    <div className="w-full h-1.5 bg-muted/30 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-primary transition-all duration-500 ease-out"
                        style={{ width: `${processingProgress}%` }}
                      />
                    </div>
                    {}
                    <span className="font-mono text-sm text-primary font-bold">
                      {processingProgress}%
                    </span>
                  </div>
                )}
              </div>
            ) : (
              <div className="flex items-center gap-3 font-mono text-sm font-bold tracking-wide">
                <span className="text-primary">SYBIL_RESISTANT</span>
                <span className="text-muted-foreground">|</span>
                <span className="text-primary">ZERO_KNOWLEDGE</span>
              </div>
            )}
          </div>

          {}
          <div className="flex justify-center gap-12 pt-6 border-t border-black">
            <div className="text-center w-24">
              <p className="font-serif text-2xl italic text-foreground">Scan</p>
              <p className="font-mono text-[10px] text-muted-foreground uppercase tracking-widest">Fingerprint</p>
            </div>
            <div className="text-center w-24">
              <p className="font-serif text-2xl italic text-foreground">Prove</p>
              <p className="font-mono text-[10px] text-muted-foreground uppercase tracking-widest">Humanity</p>
            </div>
            <div className="text-center w-24">
              <p className="font-serif text-2xl italic text-primary h-8 flex items-center justify-center overflow-hidden">
                <CyclingText words={["Vote", "Pay", "Verify", "Approve"]} interval={1200} stopAfterCycle={true} />
              </p>
              <p className="font-mono text-[10px] text-muted-foreground uppercase tracking-widest">Anonymously</p>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
