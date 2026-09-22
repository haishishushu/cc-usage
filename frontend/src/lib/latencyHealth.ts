/**
 * 请求延迟分档与配色（对齐 sub2api 的 `frontend/src/utils/latencyHealth.ts`）。
 *
 * 首字与总耗时用两套阈值：等首字等 10 秒已经难受，而整条请求跑 60 秒
 * （含多轮工具调用）却很常见，用同一把尺子会让总耗时列长期泛红。
 *
 * 阈值取「达到即进档」：恰好 10s 记 warn，而不是 good。
 */
export type LatencySeverity = "good" | "warn" | "slow" | "critical"

export const FIRST_TOKEN_THRESHOLDS_MS = { warn: 10_000, slow: 30_000, critical: 60_000 } as const
export const DURATION_THRESHOLDS_MS = { warn: 60_000, slow: 180_000, critical: 300_000 } as const

function classify(ms: number, thresholds: { warn: number; slow: number; critical: number }): LatencySeverity {
  // 由重到轻依次判断，命中即返回
  if (!Number.isFinite(ms) || ms < 0) return "good"
  if (ms >= thresholds.critical) return "critical"
  if (ms >= thresholds.slow) return "slow"
  if (ms >= thresholds.warn) return "warn"
  return "good"
}

export const firstTokenSeverity = (ms: number): LatencySeverity => classify(ms, FIRST_TOKEN_THRESHOLDS_MS)
export const durationSeverity = (ms: number): LatencySeverity => classify(ms, DURATION_THRESHOLDS_MS)

/** 文字配色。深色主题下调淡一档，避免在深底上刺眼。 */
export const LATENCY_TEXT_CLASSES: Record<LatencySeverity, string> = {
  good: "text-emerald-600 dark:text-emerald-400",
  warn: "text-amber-600 dark:text-amber-400",
  slow: "text-orange-600 dark:text-orange-400",
  critical: "text-red-600 dark:text-red-400",
}

/** 毫秒 → 人读时长。1 秒以内给毫秒，避免「0.0s」这种看不出差别的写法。 */
export function formatLatency(ms: number | null): string | null {
  if (ms === null || !Number.isFinite(ms) || ms < 0) return null
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
  const minutes = Math.floor(ms / 60_000)
  const seconds = Math.round((ms % 60_000) / 1000)
  return seconds === 0 ? `${minutes}m` : `${minutes}m${seconds}s`
}
