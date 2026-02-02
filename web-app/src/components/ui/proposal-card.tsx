import { ArrowRight } from "lucide-react"
import { GrainGradient } from "@paper-design/shaders-react"

export interface Proposal {
  id: number
  title: string
  description: string
  yesVotes: number
  noVotes: number
  status: "active" | "passed" | "rejected"
}

interface ProposalCardProps {
  proposal: Proposal
  onSelect: () => void
  isSelected: boolean
}

export function ProposalCard({ proposal, onSelect, isSelected }: ProposalCardProps) {
  const totalVotes = proposal.yesVotes + proposal.noVotes
  const yesPercentage = totalVotes > 0 ? (proposal.yesVotes / totalVotes) * 100 : 50

  return (
    <button
      onClick={onSelect}
      style={{ backgroundColor: 'rgba(0, 0, 0, 0.85)' }}
      className={`
        group relative w-full text-left border border-black transition-all duration-300 noise
        ${isSelected ? "bg-primary/5 -translate-y-1" : "hover:bg-primary/5 hover:-translate-y-1"}
      `}
    >
      {}
      {}
      <span className={`absolute top-0 left-0 h-px bg-primary shadow-[0_0_8px_rgba(236,78,2,0.6)] transition-all duration-700 ease-out ${isSelected ? 'w-full' : 'w-0 group-hover:w-full'}`} />
      {}
      <span className={`absolute bottom-0 right-0 h-px bg-primary shadow-[0_0_8px_rgba(236,78,2,0.6)] transition-all duration-700 ease-out ${isSelected ? 'w-full' : 'w-0 group-hover:w-full'}`} />
      {}
      <span className={`absolute bottom-0 left-0 w-px bg-primary shadow-[0_0_8px_rgba(236,78,2,0.6)] transition-all duration-700 ease-out delay-150 ${isSelected ? 'h-full' : 'h-0 group-hover:h-full'}`} />
      {}
      <span className={`absolute top-0 right-0 w-px bg-primary shadow-[0_0_8px_rgba(236,78,2,0.6)] transition-all duration-700 ease-out delay-150 ${isSelected ? 'h-full' : 'h-0 group-hover:h-full'}`} />

      {}
      <div className="relative flex items-center justify-between px-4 py-2 border-b border-black overflow-hidden">
        {}
        <div className="absolute inset-0 opacity-30">
          <GrainGradient
            style={{ width: "100%", height: "100%" }}
            colors={["#EC4E02", "#ff6b35", "#1a1a2e"]}
            colorBack="#0a0a0a"
            softness={0.8}
            intensity={0.2}
            noise={0.4}
            shape="wave"
            speed={0.3}
            offsetX={proposal.id * 0.3}
            offsetY={0.1}
          />
        </div>
        <span className="relative z-10 font-mono text-xs text-muted-foreground uppercase tracking-wider">
          Proposal_{proposal.id.toString().padStart(3, "0")}
        </span>
        <span className={`
          relative z-10 font-mono text-xs uppercase tracking-wider
          ${proposal.status === "active"
            ? "text-primary"
            : proposal.status === "passed"
              ? "text-success"
              : "text-destructive"
          }
        `}>
          [{proposal.status}]
        </span>
      </div>

      <div className="p-4">
        {}
        <h3 className="font-mono text-lg font-bold text-foreground mb-2 uppercase leading-tight">
          {proposal.title}
        </h3>

        {}
        <p className="font-sans text-sm text-muted-foreground mb-6 line-clamp-2 leading-relaxed">
          {proposal.description}
        </p>

        {}
        <div className="mb-4 space-y-1">
          {[...Array(5)].map((_, i) => {
            const barProgress = Math.min(100, yesPercentage + (Math.random() * 10 - 5))
            return (
              <div key={i} className="h-px bg-black relative overflow-hidden">
                <div
                  className="absolute left-0 top-0 h-full bg-primary/60 transition-all duration-500"
                  style={{ width: `${barProgress}%` }}
                />
              </div>
            )
          })}
        </div>

        {}
        <div className="flex justify-between font-mono text-xs mb-4">
          <span className="text-success">
            YES: {proposal.yesVotes}
          </span>
          <span className="text-destructive">
            NO: {proposal.noVotes}
          </span>
        </div>

        {}
        <div className="flex items-center justify-between pt-4 border-t border-black">
          <span className="font-mono text-xs text-muted-foreground">
            {totalVotes} VOTES_CAST
          </span>
          <span className="flex items-center gap-1 font-mono text-xs text-primary opacity-0 group-hover:opacity-100 transition-opacity uppercase">
            Vote
            <ArrowRight className="w-3 h-3" />
          </span>
        </div>
      </div>
    </button>
  )
}
