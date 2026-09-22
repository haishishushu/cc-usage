/** 平台、连接、来源、模型的区分见需求文档 §7.2 —— 四者是不同层级，不可混用 */
export type PlatformId = "claude" | "codex" | "gemini" | "grok" | "zcode" | "trae" | "qoder" | "workbuddy"

/** 已实现并可连接 / 已实现但尚未连接 / 尚未接入（禁用） */
export type PlatformAvailability = "available" | "not-connected" | "not-integrated"

export type PlatformCapability =
  | "auth_connection"
  | "api_connection"
  | "local_sessions"
  | "subscription_quota"
  | "organization_usage"
  | "gateway_balance"
  | "realtime_delta"
  | "cost_estimate"
  | "credits"

export interface Platform {
  id: PlatformId
  name: string
  availability: PlatformAvailability
  limitation?: string | null
  capabilities: readonly PlatformCapability[]
}

/** 连接只有两种类型（§6.3）；本地会话记录是「统计来源」而非连接 */
export type ConnectionKind = "auth" | "api"

export type ConnectionStatus = "connected" | "expired" | "invalid" | "offline" | "paused"

export interface Connection {
  id: string
  platformId: PlatformId
  kind: ConnectionKind
  /** 用户填写的连接名称 */
  name: string
  /** 后端存储的原始名称；API 连接的 name 会并上脱敏标识用于展示，编辑名称时用这个 */
  baseName: string
  /** Auth：套餐名称原样透传；API：脱敏 Key 标识。不得由额度百分比推断 */
  label: string
  /** API 连接的脱敏标识，随 Key 自动生成；Auth 为空。仅供界面展示，不可编辑 */
  masked: string
  status: ConnectionStatus
  lastSyncText: string
  /** 自建网关地址；官方直连为 null */
  baseUrl: string | null
  /** 手动添加时保存的默认参数（画布 18）；本机读取与历史连接为 null */
  model: string | null
  effort: string | null
  context1m: boolean | null
}

/** 额度窗口是一组可变窗口，不是固定的两个字段（§7.4） */
export interface QuotaWindow {
  /** 窗口标识，如 5h / 7d / 1d */
  key: string
  /** 窗口说明，如「5 小时窗口」 */
  windowName: string
  /** 已用百分比；总量分母缺失时为 null，此时不生成百分比进度条 */
  usedPercent: number | null
  /** 已用 / 总量的文字描述，缺失时为 null */
  usedText: string | null
  /** 重置倒计时，未提供时为 null（显示「—」，不推测） */
  resetCountdown: string | null
  /** 标签配色：5h 淡紫、7d 淡绿，表示窗口身份，不随水位变化 */
  badgeTone: "purple" | "green" | "blue" | "neutral"
}

export type StatSourceId = string

export interface StatSource {
  id: StatSourceId
  name: string
  /** 采集范围说明 */
  scope: string
  /** 能否按当前连接区分 */
  perConnection: boolean
  /** 是否支持实时增量（不支持时不播放增量动效） */
  realtimeDelta: boolean
  lastUpdatedText: string
}

export interface SessionActivity {
  id: string
  title: string
  /** 当前会话的本轮累计或实时采集增量文本，如「+8.0K Token」 */
  deltaText: string
  state: "running" | "done" | "failed" | "unknown"
  updatedAtMs?: number
  startedAtMs?: number
  highlighted?: boolean
}

/** 余额查询状态（§2.5，五种）与水位着色（四档）相互独立 */
export type BalanceQueryState =
  | "loading"
  | "loaded"
  | "unsupported"
  | "forbidden"
  | "failed"

export interface Balance {
  queryState: BalanceQueryState
  /** 已获取时的金额文本；不可用时为 null */
  amountText: string | null
  currency: string
  /** 「账户共享余额」/「当前 Key 余额」，只按接口实际返回标注 */
  scopeLabel: string
  sourceText: string
  level: "healthy" | "low" | "empty" | "unavailable"
  /** 失败但保留旧值时的过期说明 */
  staleText?: string
}

export type StatPeriod = "today" | "week" | "month" | "total" | "custom"

export interface TokenSummary {
  period: StatPeriod
  label: string
  value: string
  hint: string
}

export interface RequestLogRow {
  id: string
  time: string
  date: string
  /** 计费模型未知时为 null（显示「—」，不以平台名或猜测模型替代） */
  model: string | null
  /** 思考强度（low / medium / high 等）。来源未提供时为 null，显示「—」 */
  effort: string | null
  /** 实际喂进模型的输入总量，各平台口径已归一（见 lib/requestInput.ts） */
  input: string | null
  /** 输入的缓存拆解，供悬停查看；来源未按缓存拆分时为 null */
  inputHint?: string | null
  output: string | null
  cost: string | null
  /** 成本为估算值时必须标识 */
  costEstimated?: boolean
  /**
   * 请求总耗时与首字延迟（毫秒原值，由表格负责格式化与分档着色）。
   * 只有经本地代理转发的请求才测得到，否则为 null，显示「—」。
   */
  durationMs: number | null
  firstTokenMs: number | null
  status: {
    kind: "success" | "rate-limited" | "failed" | "unknown"
    /** 实际响应码或真实状态；本地日志没有 HTTP 码时不自动补 200 */
    label: string
  }
}

export type LogTableState = "ready" | "loading" | "empty" | "failed" | "unsupported"

/** 灵动岛形态：自由态（收缩 / 展开）与停靠态是两个维度（§2.1.3） */
export type IslandMode = "collapsed" | "expanded" | "docked"
export type DockEdge = "top" | "bottom" | "left" | "right"
