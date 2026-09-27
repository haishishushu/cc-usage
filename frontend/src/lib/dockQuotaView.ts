import type { QuotaWindow } from "../types"
import { isPlanCovered } from "./planCoverage.ts"

/**
 * 停靠条额度呈现的三种形态（§2.1.3，2026-09-19 鼠鼠定版）：
 *
 * - windows   查到套餐窗口（5h / 7d），按真实水位画
 * - unlimited 已确认没有 5h / 7d 窗口：两条满格彩虹色轨道（内部沿用旧命名）
 * - unknown   未连接 / 查询中 / 临时失败 / 凭证失效：只留空轨道，不着色不画满格
 *
 * 套餐口径与 planCoverage 一致：**有 5h / 7d 窗口才算有套餐**。
 * 来源只返回总配额、月消费这类非 5h / 7d 窗口时仍画满格彩虹色，
 * 真实数据在岛卡片与主面板照常展示。
 *
 * 「失败 ≠ 满格」的约定不变：满格彩虹色只用于**已确认无 5h / 7d 窗口**，
 * 与「查询失败空轨道」「真实耗尽红色满格」三者可区分。
 */
export type DockQuotaView =
  | { type: "windows"; windows: QuotaWindow[] }
  | { type: "unlimited" }
  | { type: "unknown" }

type QuotaStateName =
  | "ok"
  | "unsupported"
  | "unauthorized"
  | "forbidden"
  | "rate_limited"
  | "failed"

export function dockQuotaView(args: {
  kind: "auth" | "api"
  /** 有可用的连接（未选择 / 已暂停都算未连接） */
  connected: boolean
  /** 该连接是否配置了额度查询：网关 API Key 为 true，官方 API Key 不查订阅额度 */
  quotaQueried: boolean
  quotaLoading: boolean
  quotaState: QuotaStateName | null
  /** 查询成功（ok）时映射出的额度窗口；其余状态传 null */
  windows: QuotaWindow[] | null
}): DockQuotaView {
  if (!args.connected) return { type: "unknown" }
  // 官方直连 API Key 不提供套餐查询；Auth 若没有查询入口，仍保持未知。
  if (!args.quotaQueried) return args.kind === "api" ? { type: "unlimited" } : { type: "unknown" }
  // 查询中、临时故障与凭证问题不得画满格。
  if (args.quotaLoading || args.quotaState === null) return { type: "unknown" }
  if (args.quotaState !== "ok" && args.quotaState !== "unsupported") {
    return { type: "unknown" }
  }
  // Auth / API Key 一律按实际窗口判定；仅有其他窗口时仍保留其真实数据在卡片中。
  return args.windows?.length && isPlanCovered(args.windows)
    ? { type: "windows", windows: args.windows }
    : { type: "unlimited" }
}
