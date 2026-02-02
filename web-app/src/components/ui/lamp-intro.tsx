import { motion } from "framer-motion"
import { cn } from "@/lib/utils"

interface LampIntroProps {
  children: React.ReactNode
  className?: string
  onAnimationComplete?: () => void
}

export function LampIntro({ children, className, onAnimationComplete }: LampIntroProps) {
  return (
    <div
      className={cn(
        "relative flex min-h-[70vh] flex-col items-center justify-center overflow-hidden w-full z-0",
        className
      )}
    >
      <div className="relative flex w-full flex-1 scale-y-125 items-center justify-center isolate z-0">
        {}
        <motion.div
          initial={{ opacity: 0, width: "8rem" }}
          animate={{ opacity: [0, 0.3, 0.1, 0.5, 0.2, 1], width: "30rem" }}
          transition={{
            opacity: {
              duration: 1.5,
              times: [0, 0.2, 0.3, 0.5, 0.7, 1],
              ease: "easeInOut",
            },
            width: {
              delay: 0.8,
              duration: 0.8,
              ease: "easeInOut",
            },
          }}
          style={{
            backgroundImage: `conic-gradient(from 70deg at center top, var(--tw-gradient-stops))`,
          }}
          className="absolute inset-auto right-1/2 h-56 overflow-visible w-[30rem] bg-gradient-conic from-primary via-transparent to-transparent"
        >
          <div className="absolute w-[100%] left-0 bg-background h-40 bottom-0 z-20 [mask-image:linear-gradient(to_top,white,transparent)]" />
          <div className="absolute w-40 h-[100%] left-0 bg-background bottom-0 z-20 [mask-image:linear-gradient(to_right,white,transparent)]" />
        </motion.div>

        {}
        <motion.div
          initial={{ opacity: 0, width: "8rem" }}
          animate={{ opacity: [0, 0.3, 0.1, 0.5, 0.2, 1], width: "30rem" }}
          transition={{
            opacity: {
              duration: 1.5,
              times: [0, 0.2, 0.3, 0.5, 0.7, 1],
              ease: "easeInOut",
            },
            width: {
              delay: 0.8,
              duration: 0.8,
              ease: "easeInOut",
            },
          }}
          style={{
            backgroundImage: `conic-gradient(from 290deg at center top, var(--tw-gradient-stops))`,
          }}
          className="absolute inset-auto left-1/2 h-56 w-[30rem] bg-gradient-conic from-transparent via-transparent to-primary"
        >
          <div className="absolute w-40 h-[100%] right-0 bg-background bottom-0 z-20 [mask-image:linear-gradient(to_left,white,transparent)]" />
          <div className="absolute w-[100%] right-0 bg-background h-40 bottom-0 z-20 [mask-image:linear-gradient(to_top,white,transparent)]" />
        </motion.div>

        {}
        <div className="absolute top-1/2 h-48 w-full translate-y-12 scale-x-150 bg-background blur-2xl"></div>
        <div className="absolute top-1/2 z-50 h-48 w-full bg-transparent opacity-10 backdrop-blur-md"></div>

        {}
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: [0, 0.2, 0, 0.4, 0.1, 0.5] }}
          transition={{
            duration: 1.5,
            times: [0, 0.2, 0.3, 0.5, 0.7, 1],
            ease: "easeInOut",
          }}
          className="absolute inset-auto z-50 h-36 w-[28rem] -translate-y-1/2 rounded-full bg-primary opacity-50 blur-3xl"
        ></motion.div>

        {}
        <motion.div
          initial={{ width: "4rem", opacity: 0 }}
          animate={{ width: "16rem", opacity: [0, 0.3, 0, 0.5, 0.2, 1] }}
          transition={{
            width: {
              delay: 0.8,
              duration: 0.8,
              ease: "easeInOut",
            },
            opacity: {
              duration: 1.5,
              times: [0, 0.2, 0.3, 0.5, 0.7, 1],
              ease: "easeInOut",
            },
          }}
          className="absolute inset-auto z-30 h-36 w-64 -translate-y-[6rem] rounded-full bg-orange-400 blur-2xl"
        ></motion.div>

        {}
        <motion.div
          initial={{ width: "8rem", opacity: 0 }}
          animate={{ width: "30rem", opacity: [0, 0.5, 0, 0.8, 0.3, 1] }}
          transition={{
            width: {
              delay: 0.8,
              duration: 0.8,
              ease: "easeInOut",
            },
            opacity: {
              duration: 1.5,
              times: [0, 0.2, 0.3, 0.5, 0.7, 1],
              ease: "easeInOut",
            },
          }}
          onAnimationComplete={onAnimationComplete}
          className="absolute inset-auto z-50 h-0.5 w-[30rem] -translate-y-[7rem] bg-primary"
        ></motion.div>

        {}
        <div className="absolute inset-auto z-40 h-44 w-full -translate-y-[12.5rem] bg-background"></div>
      </div>

      {}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{
          delay: 1.6,
          duration: 0.8,
          ease: "easeOut",
        }}
        className="relative z-50 flex -translate-y-60 flex-col items-center w-full"
      >
        {children}
      </motion.div>
    </div>
  )
}
