import { cn } from "@/lib/utils"
import { compactTokens } from "@/lib/api"

export const DELTA_WIDTH = 112
export type DeltaPhase = "idle" | "enter" | "hold" | "leave"

/** 插值由 useLiveUsage 统一维护，布局切换和组件重挂载不会重播。 */
export function AnimatedTokens({ tokens, text, className }: {
  tokens?: number | null
  text?: string | null
  className?: string
}) {
  const label = tokens != null ? `+${compactTokens(tokens)} Token` : text
  return (
    <span
      data-token-value={tokens ?? undefined}
      title={tokens != null ? `+${tokens.toLocaleString()} Token` : undefined}
      className={cn("tnum font-mono text-xs font-medium text-success-text", className)}
    >
      {label}
    </span>
  )
}

export function TokenDelta({ text, tokens, phase = "hold", className }: {
  text?: string | null
  tokens?: number | null
  phase?: DeltaPhase
  className?: string
}) {
  return (
    <div
      className={cn("flex shrink-0 flex-col items-end justify-center", className)}
      style={{ width: DELTA_WIDTH, minHeight: 18 }}
    >
      {phase !== "idle" && (
        <AnimatedTokens
          tokens={tokens}
          text={text}
          className={cn(
            "transition-[transform,opacity] motion-reduce:transition-none motion-reduce:transform-none",
            phase === "enter" && "translate-y-1 opacity-0 duration-200",
            phase === "hold" && "translate-y-0 opacity-100 duration-200",
            phase === "leave" && "translate-y-0 opacity-0 duration-300",
          )}
        />
      )}
    </div>
  )
}
