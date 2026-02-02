import { useEffect, useRef } from "react"

interface WireBorderProps {
  className?: string
  color?: string
  glowColor?: string
  nodeCount?: number
  pulseSpeed?: number
  children?: React.ReactNode
}

export function WireBorder({
  className = "",
  color = "#EC4E02",
  glowColor = "#EC4E02",
  nodeCount = 12,
  pulseSpeed = 0.02,
  children,
}: WireBorderProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const mouseRef = useRef({ x: 0, y: 0 })
  const timeRef = useRef(0)

  useEffect(() => {
    const canvas = canvasRef.current
    const container = containerRef.current
    if (!canvas || !container) return

    const ctx = canvas.getContext("2d")
    if (!ctx) return

    const resize = () => {
      const rect = container.getBoundingClientRect()
      const dpr = window.devicePixelRatio || 1
      canvas.width = rect.width * dpr
      canvas.height = rect.height * dpr
      canvas.style.width = `${rect.width}px`
      canvas.style.height = `${rect.height}px`
      ctx.scale(dpr, dpr)
    }

    resize()
    window.addEventListener("resize", resize)

    const handleMouseMove = (e: MouseEvent) => {
      const rect = container.getBoundingClientRect()
      mouseRef.current = {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      }
    }

    container.addEventListener("mousemove", handleMouseMove)

    interface Node {
      x: number
      y: number
      connections: number[]
    }

    let nodes: Node[] = []
    let pulses: { from: number; to: number; progress: number; speed: number }[] = []

    const generateNodes = () => {
      const rect = container.getBoundingClientRect()
      const w = rect.width
      const h = rect.height
      const padding = 20
      const cornerRadius = 32

      nodes = []

      const perimeter = 2 * (w + h) - 8 * cornerRadius + 2 * Math.PI * cornerRadius
      const spacing = perimeter / nodeCount

      let distance = 0
      for (let i = 0; i < nodeCount; i++) {
        distance = i * spacing
        let x, y

        const topLength = w - 2 * cornerRadius
        const rightLength = h - 2 * cornerRadius
        const bottomLength = w - 2 * cornerRadius
        const leftLength = h - 2 * cornerRadius
        const cornerArc = (Math.PI / 2) * cornerRadius

        if (distance < topLength) {
          x = cornerRadius + distance
          y = padding
        } else if (distance < topLength + cornerArc) {
          const angle = (distance - topLength) / cornerRadius - Math.PI / 2
          x = w - cornerRadius - padding + Math.cos(angle) * cornerRadius
          y = cornerRadius + padding + Math.sin(angle) * cornerRadius
        } else if (distance < topLength + cornerArc + rightLength) {
          y = cornerRadius + (distance - topLength - cornerArc)
          x = w - padding
        } else if (distance < topLength + 2 * cornerArc + rightLength) {
          const angle = (distance - topLength - cornerArc - rightLength) / cornerRadius
          x = w - cornerRadius - padding + Math.cos(angle) * cornerRadius
          y = h - cornerRadius - padding + Math.sin(angle) * cornerRadius
        } else if (distance < topLength + 2 * cornerArc + rightLength + bottomLength) {
          x = w - cornerRadius - (distance - topLength - 2 * cornerArc - rightLength)
          y = h - padding
        } else if (distance < topLength + 3 * cornerArc + rightLength + bottomLength) {
          const angle = (distance - topLength - 2 * cornerArc - rightLength - bottomLength) / cornerRadius + Math.PI / 2
          x = cornerRadius + padding + Math.cos(angle) * cornerRadius
          y = h - cornerRadius - padding + Math.sin(angle) * cornerRadius
        } else if (distance < topLength + 3 * cornerArc + rightLength + bottomLength + leftLength) {
          y = h - cornerRadius - (distance - topLength - 3 * cornerArc - rightLength - bottomLength)
          x = padding
        } else {
          const angle = (distance - topLength - 3 * cornerArc - rightLength - bottomLength - leftLength) / cornerRadius + Math.PI
          x = cornerRadius + padding + Math.cos(angle) * cornerRadius
          y = cornerRadius + padding + Math.sin(angle) * cornerRadius
        }

        nodes.push({
          x: Math.max(padding, Math.min(w - padding, x)),
          y: Math.max(padding, Math.min(h - padding, y)),
          connections: [(i + 1) % nodeCount, (i - 1 + nodeCount) % nodeCount],
        })
      }

      for (let i = 0; i < nodeCount / 3; i++) {
        const a = Math.floor(Math.random() * nodeCount)
        const b = (a + Math.floor(nodeCount / 3) + Math.floor(Math.random() * 3)) % nodeCount
        if (!nodes[a].connections.includes(b)) {
          nodes[a].connections.push(b)
        }
      }
    }

    generateNodes()

    const spawnPulse = () => {
      const fromNode = Math.floor(Math.random() * nodes.length)
      const toNode = nodes[fromNode].connections[Math.floor(Math.random() * nodes[fromNode].connections.length)]
      pulses.push({
        from: fromNode,
        to: toNode,
        progress: 0,
        speed: pulseSpeed + Math.random() * 0.01,
      })
    }

    let animationId: number

    function draw() {
      const rect = container.getBoundingClientRect()
      const w = rect.width
      const h = rect.height

      ctx.clearRect(0, 0, w, h)

      ctx.strokeStyle = `${color}30`
      ctx.lineWidth = 1

      nodes.forEach((node, i) => {
        node.connections.forEach((connIdx) => {
          const target = nodes[connIdx]
          ctx.beginPath()
          ctx.moveTo(node.x, node.y)
          ctx.lineTo(target.x, target.y)
          ctx.stroke()
        })
      })

      nodes.forEach((node) => {
        const dx = node.x - mouseRef.current.x
        const dy = node.y - mouseRef.current.y
        const dist = Math.sqrt(dx * dx + dy * dy)
        const glow = Math.max(0, 1 - dist / 150)

        ctx.fillStyle = `${color}${Math.round((0.3 + glow * 0.7) * 255).toString(16).padStart(2, "0")}`
        ctx.beginPath()
        ctx.arc(node.x, node.y, 2 + glow * 2, 0, Math.PI * 2)
        ctx.fill()

        if (glow > 0.1) {
          ctx.shadowColor = glowColor
          ctx.shadowBlur = 10 * glow
          ctx.fill()
          ctx.shadowBlur = 0
        }
      })

      pulses = pulses.filter((pulse) => {
        pulse.progress += pulse.speed

        if (pulse.progress >= 1) {
          if (Math.random() > 0.3) {
            const nextNode = nodes[pulse.to]
            const nextTarget = nextNode.connections[Math.floor(Math.random() * nextNode.connections.length)]
            pulses.push({
              from: pulse.to,
              to: nextTarget,
              progress: 0,
              speed: pulse.speed,
            })
          }
          return false
        }

        const from = nodes[pulse.from]
        const to = nodes[pulse.to]
        const x = from.x + (to.x - from.x) * pulse.progress
        const y = from.y + (to.y - from.y) * pulse.progress

        const gradient = ctx.createRadialGradient(x, y, 0, x, y, 8)
        gradient.addColorStop(0, glowColor)
        gradient.addColorStop(1, `${glowColor}00`)

        ctx.fillStyle = gradient
        ctx.beginPath()
        ctx.arc(x, y, 8, 0, Math.PI * 2)
        ctx.fill()

        ctx.fillStyle = "#ffffff"
        ctx.beginPath()
        ctx.arc(x, y, 2, 0, Math.PI * 2)
        ctx.fill()

        return true
      })

      if (Math.random() < 0.03 && pulses.length < 8) {
        spawnPulse()
      }

      timeRef.current += 1
      animationId = requestAnimationFrame(draw)
    }

    draw()

    return () => {
      cancelAnimationFrame(animationId)
      window.removeEventListener("resize", resize)
      container.removeEventListener("mousemove", handleMouseMove)
    }
  }, [color, glowColor, nodeCount, pulseSpeed])

  return (
    <div ref={containerRef} className={`relative ${className}`}>
      <canvas
        ref={canvasRef}
        className="absolute inset-0 pointer-events-none z-0"
      />
      <div className="relative z-10">{children}</div>
    </div>
  )
}
