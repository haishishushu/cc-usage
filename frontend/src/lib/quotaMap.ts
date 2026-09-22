import type { QuotaWindowDto } from "./api"
import type { QuotaWindow } from "@/types"

/**
 * 后端额度窗口 → 界面额度窗口（§7.4）
 *
 * 窗口标签配色表示**窗口身份**，不随水位变化：5h 淡紫、7d 淡绿，其余中性。
 * 重置倒计时来源未提供时保持 null，界面显示「—」而不推测。
 */
const TONE: Record<string, QuotaWindow["badgeTone"]> = {
  "5h": "purple",
  "7d": "green",
  "7d-opus": "green",
  "7d-sonnet": "green",
  "1d": "blue",
  "30d": "blue",
}

/** 把 ISO 重置时刻换成「还剩 2h 15m」这类倒计时。 */
export function formatQuotaCountdown(iso: string | null, now = Date.now()): string | null {
  if (!iso) return null
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return null
  const ms = t - now
  // 到零后保留当前额度并提示等待服务端返回新窗口，不能伪装成“未提供时间”。
  if (ms <= 0) return "等待更新"
  const mins = Math.floor(ms / 60000)
  const d = Math.floor(mins / 1440)
  const h = Math.floor((mins % 1440) / 60)
  const m = mins % 60
  if (d > 0) return `还剩 ${d}d ${h}h`
  if (h > 0) return `还剩 ${h}h ${m}m`
  return `还剩 ${m}m`
}

export function toQuotaWindows(dto: QuotaWindowDto[], now = Date.now()): QuotaWindow[] {
  return dto.map((w) => ({
    key: w.key,
    windowName: w.window_name,
    // null 一路传下去：界面据此不画进度条，而不是当成 0%
    usedPercent: w.used_percent,
    usedText: w.amount_text,
    resetCountdown: formatQuotaCountdown(w.resets_at, now),
    badgeTone: TONE[w.key] ?? "neutral",
  }))
}
