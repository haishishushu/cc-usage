import type { QuotaWindow } from "../types"
import { isPlanCovered } from "./planCoverage.ts"

/**
 * 停靠条额度呈现的三种形态（§2.1.3，2026-09-19 鼠鼠定版）：
 *
 * - windows   查到套餐窗口（5h / 7d），按真实水位画
 * - unlimited API Key 无套餐：两条满格蓝色轨道，表示「无套餐上限、按量计费」
 * - unknown   未连接 / 查询中 / 临时失败 / 凭证失效：只留空轨道，不着色不画满格
 *
 * 套餐口径与 planCoverage 一致：**有 5h / 7d 窗口才算有套餐**。
 * 网关只返回总配额、月消费这类非套餐窗口时仍按无套餐画满格蓝，
 * 真实数据在岛卡片与主面板照常展示。
 *
 * 「失败 ≠ 满格」的约定不变：满格蓝只用于**确定性的无套餐结论**，
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
  // 官方订阅：查到窗口画真实水位，缺数据是「查不到」，保持空轨道不伪装
  if (args.kind !== "api") {
    return args.windows?.length ? { type: "windows", windows: args.windows } : { type: "unknown" }
  }
  // 查询中、或查询已发出但还没回来（state 为 null），都不下「无套餐」结论
  if (args.quotaLoading || (args.quotaQueried && args.quotaState === null)) return { type: "unknown" }
  if (args.quotaQueried && args.quotaState !== "ok" && args.quotaState !== "unsupported") {
    // 临时故障与凭证问题不得画满格：满格蓝只代表「确认无套餐」
    return { type: "unknown" }
  }
  // 有套餐窗口画真实水位；没有（无窗口、或只有非套餐窗口）＝无套餐 → 满格蓝
  return args.windows?.length && isPlanCovered(args.windows)
    ? { type: "windows", windows: args.windows }
    : { type: "unlimited" }
}
