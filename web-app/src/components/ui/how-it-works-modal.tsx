import { X } from "lucide-react"
import { PhaseCards } from "./phase-cards"

interface HowItWorksModalProps {
  isOpen: boolean
  onClose: () => void
}

export function HowItWorksModal({ isOpen, onClose }: HowItWorksModalProps) {
  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {}
      <div
        className="absolute inset-0 bg-black/90 backdrop-blur-sm"
        onClick={onClose}
      />

      {}
      <div className="relative w-full max-w-5xl h-[80vh] mx-4 overflow-hidden rounded-2xl border border-white/10 bg-black">
        {}
        <button
          onClick={onClose}
          className="absolute top-4 right-4 z-50 p-2 rounded-full hover:bg-white/5 transition-colors"
        >
          <X className="w-4 h-4 text-white/40" />
        </button>

        {}
        <PhaseCards />
      </div>
    </div>
  )
}
