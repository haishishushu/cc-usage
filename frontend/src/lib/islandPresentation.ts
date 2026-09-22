import type { Connection } from "../types"

/** 两个平台共用连接规则；只有网关 API 同时提供额度和钱包查询。 */
export function connectionQueries(connection: Connection | null) {
  const none = { quota: null, balance: null, usage: null }
  if (!connection || connection.status === "paused") return none
  if (connection.kind === "auth") return { ...none, quota: connection.id }
  return connection.baseUrl
    ? { quota: connection.id, balance: connection.id, usage: null }
    : { ...none, usage: connection.id }
}

/**
 * 灵动岛生效连接（§7.3，2026-09-19 鼠鼠定版）：明确选择的连接优先；
 * **未选择**连接时，自动启用当前平台第一个连接成功的连接。
 * 仍不回退到本机账号或「唯一候选」；已选择的连接（含已暂停）不被悄悄顶掉，
 * 所选连接被删除时也照旧显示「所选连接不可用」。
 */
export function islandActiveConnection(
  configuredId: string | null | undefined,
  connections: readonly Connection[],
  platform: string,
): Connection | null {
  if (configuredId) {
    return connections.find((c) => c.id === configuredId) ?? null
  }
  return connections.find((c) => c.platformId === platform && c.status === "connected") ?? null
}

/** 复用真实连接状态；未知选择不能伪装成绿色成功。 */
export function islandConnectionStatus(
  status: Connection["status"] | null,
  localAuth: boolean,
  hasSavedSelection: boolean,
): { tone: "success" | "warn" | "danger"; label: string } {
  if (status === "paused") return { tone: "warn", label: "已断开" }
  if (status === "invalid") return { tone: "danger", label: "凭证无效" }
  if (status === "offline") return { tone: "warn", label: "离线" }
  if (status === "expired") return { tone: "warn", label: "凭证已过期" }
  if (status === "connected" || localAuth) return { tone: "success", label: "已连接" }
  return { tone: "warn", label: hasSavedSelection ? "所选连接不可用" : "未选择连接" }
}

function validCost(value: number | null | undefined): value is number {
  return value != null && Number.isFinite(value) && value >= 0
}

/** 实际账单优先；微小非零值与零、未知值保持区分。 */
export function islandCost(
  actual: number | null | undefined,
  estimate: { amount: number; complete: boolean } | null | undefined,
): { cost: string | null; costEstimated: boolean } {
  const estimated = !validCost(actual) && !!estimate?.complete && validCost(estimate.amount)
  const amount = validCost(actual) ? actual : estimated ? estimate!.amount : null
  if (amount === null) return { cost: null, costEstimated: false }
  const formatted = amount > 0 && amount < 0.0001
    ? "<$0.0001"
    : `$${amount.toFixed(amount > 0 && amount < 0.01 ? 4 : 2)}`
  return { cost: `${estimated ? "≈ " : ""}${formatted}`, costEstimated: estimated }
}

/**
 * 余额是查询瞬间的快照，不是实时值：网关侧每笔请求都在实时扣减，
 * 与 sub2api 面板比对时先对获取时刻再对数值，因此展示时刻本身。
 */
export function balanceSnapshotTime(fetchedAt: Date | null | undefined): string | null {
  if (!fetchedAt) return null
  return `· ${fetchedAt.toLocaleTimeString("zh-CN", { hour12: false })} 获取`
}
