/**
 * 额度水位着色 —— 需求文档 §3「额度水位着色（已确认）」
 *
 * 已用 < 75%        绿 success    正常
 * 已用 75% – 89%    琥珀 warn     接近上限
 * 已用 >= 90%       红 danger     即将耗尽 / 已耗尽（100%）
 *
 * 阈值对 5h、7d 及其他所有额度窗口一致，不因窗口长短改变。
 * 着色只作用于进度条填充；周期标签（5h 淡紫 / 7d 淡绿）与百分比文字颜色不变。
 */
export type QuotaLevel = "normal" | "warning" | "critical"

export const QUOTA_WARN_THRESHOLD = 75
export const QUOTA_CRITICAL_THRESHOLD = 90

export function quotaLevel(usedPercent: number): QuotaLevel {
  if (usedPercent >= QUOTA_CRITICAL_THRESHOLD) return "critical"
  if (usedPercent >= QUOTA_WARN_THRESHOLD) return "warning"
  return "normal"
}

/** 进度条填充色的 class */
export function quotaBarClass(usedPercent: number): string {
  switch (quotaLevel(usedPercent)) {
    case "critical":
      return "bg-danger"
    case "warning":
      return "bg-warn"
    default:
      return "bg-success"
  }
}

/** 主面板宽额度行的等级文字（灵动岛收缩态不显示，见 §3） */
export function quotaLevelLabel(usedPercent: number): string | null {
  switch (quotaLevel(usedPercent)) {
    case "critical":
      return usedPercent >= 100 ? "已耗尽" : "即将耗尽"
    case "warning":
      return "接近上限"
    default:
      return "正常"
  }
}

export function quotaLevelChipClass(usedPercent: number): string {
  switch (quotaLevel(usedPercent)) {
    case "critical":
      return "bg-danger-soft text-danger"
    case "warning":
      return "bg-warn-soft text-warn"
    default:
      return "bg-success-soft text-success-text"
  }
}

/**
 * 余额水位着色 —— 需求文档 §2.5「余额水位着色（已确认）」
 * 与查询状态相互独立。告警阈值待确认，阈值未定前不得声称某个余额「充足」。
 */
export type BalanceLevel = "healthy" | "low" | "empty" | "unavailable"

export function balanceTextClass(level: BalanceLevel): string {
  switch (level) {
    case "healthy":
      return "text-success-text"
    case "low":
      return "text-warn"
    case "empty":
      return "text-danger"
    default:
      return "text-text-tertiary"
  }
}
