import { useEffect, useState, useRef } from "react"

interface TextScrambleProps {
  text: string
  className?: string
  scrambleOnHover?: boolean
  scrambleOnMount?: boolean
  duration?: number
  characters?: string
}

export function TextScramble({
  text,
  className = "",
  scrambleOnHover = true,
  scrambleOnMount = true,
  duration = 1000,
  characters = "!@#$%^&*()_+-=[]{}|;:,.<>?0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
}: TextScrambleProps) {
  const [displayText, setDisplayText] = useState(text)
  const [isScrambling, setIsScrambling] = useState(false)
  const intervalRef = useRef<NodeJS.Timeout | null>(null)
  const hasAnimated = useRef(false)

  const scramble = () => {
    if (isScrambling) return
    setIsScrambling(true)

    const originalText = text
    const iterations = Math.ceil(duration / 50)
    let currentIteration = 0

    intervalRef.current = setInterval(() => {
      currentIteration++
      const progress = currentIteration / iterations

      const revealedCount = Math.floor(progress * originalText.length)

      let result = ""
      for (let i = 0; i < originalText.length; i++) {
        if (originalText[i] === " ") {
          result += " "
        } else if (i < revealedCount) {
          result += originalText[i]
        } else {
          result += characters[Math.floor(Math.random() * characters.length)]
        }
      }

      setDisplayText(result)

      if (currentIteration >= iterations) {
        if (intervalRef.current) clearInterval(intervalRef.current)
        setDisplayText(originalText)
        setIsScrambling(false)
      }
    }, 50)
  }

  useEffect(() => {
    if (scrambleOnMount && !hasAnimated.current) {
      hasAnimated.current = true
      const timeout = setTimeout(scramble, 100)
      return () => clearTimeout(timeout)
    }
  }, [])

  useEffect(() => {
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [])

  const handleMouseEnter = () => {
    if (scrambleOnHover && !isScrambling) {
      scramble()
    }
  }

  return (
    <span
      className={className}
      onMouseEnter={handleMouseEnter}
    >
      {displayText}
    </span>
  )
}
