import { useState } from "react"

const glowPulseStyle = `
  @keyframes glowPulse {
    0%, 100% {
      filter: invert(1) contrast(1.5) brightness(0.4)
        drop-shadow(0 0 6px rgba(236, 78, 2, 0.5))
        drop-shadow(0 0 10px rgba(236, 78, 2, 0.2))
        drop-shadow(0 4px 8px rgba(0, 0, 0, 0.5));
    }
    50% {
      filter: invert(1) contrast(1.5) brightness(0.5)
        drop-shadow(0 0 14px rgba(236, 78, 2, 0.8))
        drop-shadow(0 0 24px rgba(236, 78, 2, 0.4))
        drop-shadow(0 4px 8px rgba(0, 0, 0, 0.5));
    }
  }
`

interface FingerprintWavesProps {
  className?: string
  size?: number
  color?: string
  state?: "idle" | "hover" | "scanning" | "success" | "error"
}

export function FingerprintWaves({
  className = "",
  size = 400,
  state = "idle",
}: FingerprintWavesProps) {
  const [isHovered, setIsHovered] = useState(false)

  const shouldAnimate = isHovered || state === "scanning" || state === "success" || state === "error"

  return (
    <div
      className={`cursor-pointer ${className}`}
      style={{
        width: size,
        height: size,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        overflow: "hidden",
        position: "relative",
      }}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      <style>{glowPulseStyle}</style>
      {}
      <img
        src="/fingerprint.png"
        alt="Fingerprint"
        style={{
          position: "relative",
          zIndex: 10,
          width: "90%",
          height: "90%",
          objectFit: "contain",
          imageRendering: "pixelated",
          transition: "transform 0.3s ease, filter 0.3s ease",
          transform: shouldAnimate ? "scale(1.02)" : "scale(1)",
          animation: "glowPulse 2s ease-in-out infinite",
          mixBlendMode: "hard-light",
          opacity: 0.7,
        }}
      />
    </div>
  )
}
