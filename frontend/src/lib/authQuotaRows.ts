import type { QuotaWindow } from "../types"

/** Auth 保留额度布局，未知值不能伪装成 API 指标或 0%。 */
export function authQuotaRows(windows?: QuotaWindow[], missingText: string | null = null): QuotaWindow[] {
  const defaults: QuotaWindow[] = [
    { key: "5h", windowName: "5 小时额度", usedPercent: null, usedText: missingText, resetCountdown: null, badgeTone: "purple" },
    { key: "7d", windowName: "周额度", usedPercent: null, usedText: missingText, resetCountdown: null, badgeTone: "green" },
  ]
  if (missingText === null && windows?.some(w => w.key !== "5h" && w.key !== "7d")) return windows.slice(0, 2)
  return defaults.map(empty => windows?.find(w => w.key === empty.key) ?? empty)
}
