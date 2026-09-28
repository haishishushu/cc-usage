import { cn } from "@/lib/utils"
import { compactTokens } from "@/lib/api"
import { useLiveCount } from "@/lib/liveCounts"

export const DELTA_WIDTH = 112
export type DeltaPhase = "idle" | "enter" | "hold" | "leave"

/**
 * 插值由 useLiveUsage 统一维护，布局切换和组件重挂载不会重播。
 * 给出 `countKey` 时只有本组件订阅逐帧数值，父组件不随追数重渲染。
 */
export function AnimatedTokens({ tokens: fixed, text, countKey, className }: {
  tokens?: number | null
  text?: string | null
  countKey?: string
  className?: string
}) {
  const live = useLiveCount(countKey)
  const tokens = countKey ? (live ?? 0) : fixed
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

export function TokenDelta({ text, tokens, countKey, phase = "hold", className }: {
  text?: string | null
  tokens?: number | null
  countKey?: string
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
          countKey={countKey}
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
