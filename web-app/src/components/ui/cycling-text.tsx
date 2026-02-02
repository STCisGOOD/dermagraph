import { useEffect, useState, useRef } from "react"
import { motion, AnimatePresence } from "framer-motion"

interface CyclingTextProps {
  words: string[]
  className?: string
  interval?: number
  stopAfterCycle?: boolean
}

export function CyclingText({
  words,
  className = "",
  interval = 800,
  stopAfterCycle = true
}: CyclingTextProps) {
  const [currentIndex, setCurrentIndex] = useState(0)
  const [isAnimating, setIsAnimating] = useState(true)
  const cycleCount = useRef(0)

  useEffect(() => {
    if (!isAnimating) return

    const timer = setInterval(() => {
      setCurrentIndex((prev) => {
        const nextIndex = (prev + 1) % words.length

        if (nextIndex === 0) {
          cycleCount.current++
          if (stopAfterCycle && cycleCount.current >= 1) {
            setIsAnimating(false)
            return 0
          }
        }

        return nextIndex
      })
    }, interval)

    return () => clearInterval(timer)
  }, [words, interval, stopAfterCycle, isAnimating])

  return (
    <span className={`inline-block relative ${className}`}>
      <AnimatePresence mode="popLayout">
        <motion.span
          key={currentIndex}
          initial={{ y: 20, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          exit={{ y: -20, opacity: 0 }}
          transition={{ duration: 0.3, ease: "easeInOut" }}
          className="inline-block"
        >
          {words[currentIndex]}
        </motion.span>
      </AnimatePresence>
    </span>
  )
}
