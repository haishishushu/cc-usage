import type { BalanceStateDto, QuotaStateDto } from "./api.ts"
import { isPlanCovered } from "./planCoverage.ts"
import { remainingDisplay } from "./remainingDisplay.ts"
import { remainingPercent } from "./quota.ts"

export type ConnectionMetricView = {
  mode: "plan" | "metered" | "native" | "unknown"
  items: { label: string; value: string; title?: string }[]
  reason: string | null
}

export function connectionMetricView(
  kind: "auth" | "api",
  quota: QuotaStateDto | null,
  balance: BalanceStateDto | null,
  todayTokens: number | null,
  native?: { platform: "zcode" | "workbuddy"; totalTokens: number | null; credits: number | null },
): ConnectionMetricView {
  if (native?.platform === "workbuddy") {
    return {
      mode: "native",
      items: [
        { label: "本机累计 Token", value: native.totalTokens?.toLocaleString("en-US") ?? "—" },
        { label: "累计上报积分", value: native.credits?.toLocaleString("zh-CN", { maximumFractionDigits: 4 }) ?? "—", title: "本机会话上报的已用积分，不是账号剩余积分" },
      ],
      reason: null,
    }
  }
  if (native?.platform === "zcode") {
    const display = remainingDisplay("zcode", "auth", quota)
    if (display.state === "ok") {
      return {
        mode: "native",
        items: display.rows.map((row) => ({ label: row.label, value: row.value, title: row.usedText ?? undefined })),
        reason: null,
      }
    }
    return {
      mode: "native",
      items: [{ label: "本机累计 Token", value: native.totalTokens?.toLocaleString("en-US") ?? "—" }],
      reason: display.state === "loading" ? null : display.reason,
    }
  }
  const windows = quota?.state === "ok" ? quota.windows : null
  if (kind === "auth" || isPlanCovered(windows)) {
    const item = (key: "5h" | "7d", label: string) => {
      const window = windows?.find((entry) => entry.key === key)
      const percent = window?.used_percent
      return {
        label,
        value: percent != null && Number.isFinite(percent) ? `剩余 ${Math.round(remainingPercent(percent))}%` : window?.amount_text ?? "—",
        title: window?.amount_text ?? undefined,
      }
    }
    return {
      mode: "plan",
      items: [item("5h", "5 小时"), item("7d", "周额度")],
      reason: quota && quota.state !== "ok" ? quota.reason : null,
    }
  }
  if (!quota || (quota.state !== "ok" && quota.state !== "unsupported")) {
    return { mode: "unknown", items: [{ label: "额度类型", value: "—" }], reason: quota && "reason" in quota ? quota.reason : null }
  }
  return {
    mode: "metered",
    items: [
      { label: "本机今日 Token", value: todayTokens == null ? "—" : todayTokens.toLocaleString("en-US") },
      { label: "剩余额度", value: balance?.state === "ok" ? `${balance.balance.toFixed(2)} ${balance.currency}` : "—" },
    ],
    reason: balance?.state === "ok" ? null : balance?.reason ?? "当前连接未提供可查询的在线剩余额度",
  }
}
