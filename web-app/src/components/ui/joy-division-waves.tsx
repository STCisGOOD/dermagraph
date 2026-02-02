import { useEffect, useRef } from "react"

interface JoyDivisionWavesProps {
  className?: string
  width?: number
  height?: number
  color?: string
  lineCount?: number
  animated?: boolean
  mouseReactive?: boolean
}

export function JoyDivisionWaves({
  className = "",
  width = 400,
  height = 300,
  color = "#EC4E02",
  lineCount = 40,
  animated = true,
  mouseReactive = true,
}: JoyDivisionWavesProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const mouseRef = useRef({ x: width / 2, y: height / 2 })
  const timeRef = useRef(0)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const ctx = canvas.getContext("2d")
    if (!ctx) return

    const dpr = window.devicePixelRatio || 1
    canvas.width = width * dpr
    canvas.height = height * dpr
    canvas.style.width = `${width}px`
    canvas.style.height = `${height}px`
    ctx.scale(dpr, dpr)

    const handleMouseMove = (e: MouseEvent) => {
      const rect = canvas.getBoundingClientRect()
      mouseRef.current = {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      }
    }

    if (mouseReactive) {
      canvas.addEventListener("mousemove", handleMouseMove)
    }

    let animationId: number

    function draw() {
      ctx.clearRect(0, 0, width, height)

      const lineSpacing = height / (lineCount + 2)
      const points = 100

      for (let line = 0; line < lineCount; line++) {
        const baseY = lineSpacing * (line + 1.5)

        ctx.beginPath()
        ctx.strokeStyle = color
        ctx.lineWidth = 1.2

        for (let i = 0; i <= points; i++) {
          const x = (i / points) * width
          const normalizedX = i / points

          const centerInfluence = Math.sin(normalizedX * Math.PI)
          const lineProgress = line / lineCount

          const noise1 = Math.sin(normalizedX * 8 + timeRef.current * 0.02 + line * 0.5) * 0.3
          const noise2 = Math.sin(normalizedX * 15 + timeRef.current * 0.03 - line * 0.3) * 0.15
          const noise3 = Math.sin(normalizedX * 25 + timeRef.current * 0.01 + line * 0.8) * 0.1

          let mouseInfluence = 0
          if (mouseReactive) {
            const dx = x - mouseRef.current.x
            const dy = baseY - mouseRef.current.y
            const dist = Math.sqrt(dx * dx + dy * dy)
            mouseInfluence = Math.max(0, 1 - dist / 150) * 30 * Math.sin(dist * 0.05 - timeRef.current * 0.1)
          }

          const mainAmplitude = centerInfluence * (20 + Math.sin(lineProgress * Math.PI) * 25)
          const totalNoise = (noise1 + noise2 + noise3) * mainAmplitude

          const y = baseY - mainAmplitude - totalNoise - mouseInfluence

          if (i === 0) {
            ctx.moveTo(x, y)
          } else {
            ctx.lineTo(x, y)
          }
        }

        ctx.stroke()

        ctx.lineTo(width, height)
        ctx.lineTo(0, height)
        ctx.closePath()
        ctx.fillStyle = "#050505"
        ctx.fill()
      }

      if (animated) {
        timeRef.current += 1
      }

      animationId = requestAnimationFrame(draw)
    }

    draw()

    return () => {
      cancelAnimationFrame(animationId)
      if (mouseReactive) {
        canvas.removeEventListener("mousemove", handleMouseMove)
      }
    }
  }, [width, height, color, lineCount, animated, mouseReactive])

  return (
    <canvas
      ref={canvasRef}
      className={className}
      style={{ display: "block" }}
    />
  )
}
