/**
 * 后端调用封装。
 *
 * 浏览器里（`pnpm dev` 预览）没有 Tauri，所有命令会被标记为不可用，
 * 调用方回落到 `mock/data.ts` 的设计示例——预览页因此仍然可用。
 */

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window

export const localCodexQuota = (force = false) => invoke<QuotaStateDto>("local_codex_quota", { force })

export const localGrokQuota = (force = false) => invoke<QuotaStateDto>("local_grok_quota", { force })

/** 套餐查询辅助凭证的掩码视图；凭证原值永不出后端 */
export interface PlanQueryStatusDto {
  zhipu_team_organization_id: string
  zhipu_team_project_id: string
  volc_access_key_masked: string
  has_volc_secret: boolean
  volc_secret_tail: string
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) throw new Error("not-tauri")
  const { invoke } = await import("@tauri-apps/api/core")
  return invoke<T>(cmd, args)
}

export interface PeriodTotals {
  today: number | null
  week: number | null
  month: number | null
  total: number | null
  /** 自定义区间合计；未使用自定义时间时为 null */
  custom: number | null
  collected_since: string | null
}

export interface TrendPoint {
  label: string
  tokens: number | null
  /** 新增输入：已扣除缓存重读，两个平台口径一致 */
  fresh_input: number | null
  output: number | null
  cache_write: number | null
  cache_read: number | null
  /** 该桶的估算费用；桶内有无价目模型或缺字段时为 null，图上断线而不画 0 */
  cost: number | null
}

export interface Trend {
  points: TrendPoint[]
  /** 实际分桶粒度，界面需明示（§7.5） */
  bucket: string
}

/**
 * 区间内的 Token 分项。每一项都可能为 null —— 来源没提供该字段时保持未知，
 * 界面显示「—」，不能当成真实的 0（§DATA-09）。
 */
export interface UsageBreakdown {
  fresh_input: number | null
  output: number | null
  cache_write: number | null
  cache_read: number | null
  /** 真实消耗，与四张周期卡同源 */
  real_total: number | null
  /** 请求条数。本地记录可精确计数，不会未知 */
  requests: number
  /** 缓存命中率 0–1；任一分量未知则为 null，不画进度条 */
  cache_hit_rate: number | null
  /** 平均每次请求消耗；无请求或总量未知时为 null */
  avg_per_request: number | null
}

export interface SourceMetrics {
  credits: number | null
  credit_requests: number
  context_ratio: number | null
  context_at_ms: number | null
}

export interface ApiLogRow {
  native_outcome?: "success" | "failed" | "unknown" | null
  input_semantics?: "includes_cache" | "excludes_cache" | "unknown"
  id: number
  time: string
  date: string
  model: string | null
  input: number | null
  output: number | null
  cache_read: number | null
  cache_write: number | null
  total: number | null
  session_id: string | null
  platform: string
  /** 估算金额（美元）；模型不在价目表内时为 null */
  cost_estimate: number | null
  /** 思考强度（low / medium / high 等），来源原样透传；加列前的历史记录为 null */
  effort: string | null
  /** 请求总耗时（毫秒）。只有经本地代理转发的请求才有，否则 null */
  duration_ms: number | null
  /** 首字延迟（毫秒），同上 */
  first_token_ms: number | null
  /** 上游真实 HTTP 响应码，同上 */
  status_code: number | null
}

export interface LogPage {
  rows: ApiLogRow[]
  total_count: number
  page: number
  page_size: number
  page_count: number
}

export interface ActiveSession {
  session_id: string
  title: string
  delta_tokens: number
  last_seen_ms: number
  started_at_ms?: number | null
  running_tokens?: number | null
  state: "running" | "done" | "failed" | "recent"
}

export interface LiveUsageDto {
  platform: string
  cursor: number
  delta_tokens: number
  delta_sessions: ActiveSession[]
  active_sessions: ActiveSession[]
  running_tokens?: number | null
  active_window_seconds: number
  today_tokens: number | null
  realtime_delta: boolean
  initial: boolean
}

/** 接入方式只有官方订阅（Auth）与 API Key 两种 */
export interface ConnectionInfo {
  kind: "auth" | "api" | null
  label: string | null
}

/** 连接（§6.3）。**后端刻意不返回凭证原值**，只有 masked */
export interface ConnectionDto {
  id: string
  platform: string
  kind: "auth" | "api"
  name: string
  label: string
  masked: string
  status: "connected" | "expired" | "invalid" | "offline" | "paused"
  last_sync_text: string | null
  /** sub2api 等自建网关地址；地址不是凭证，可回显 */
  base_url: string | null
  /** 手动添加时保存的默认参数；本机读取与历史连接为 null */
  model: string | null
  effort: string | null
  context_1m: boolean | null
}

export interface FetchResult {
  ok: boolean
  masked: string | null
  message: string
  /** 扫描过的路径与环境变量，界面需说明读取范围 */
  scanned: string[]
  /** 本次是否真的写入了变更；按钮据此区分「已更新」与「已是最新」 */
  changed: boolean
}

/** 本机自动发现的连接候选，用于「+ 添加连接」预填 */
export interface Candidate {
  base_url?: string | null
  platform: string
  kind: "auth" | "api"
  suggested_name: string
  masked: string
}

/** 自定义时间范围（§2.3）。end 为 null 表示跟随当前时刻 */
export interface CustomRange {
  start: number
  end: number | null
}

export interface ScanResult {
  files_scanned: number
  records_inserted: number
  claude_found: boolean
  codex_found: boolean
  errors: string[]
}

export interface CollectionStatusDto {
  ok: boolean
  errors: string[]
  last_success_ms: number | null
  /** 出错文件所属的平台（claude / codex）：采集异常按平台定界，不跨平台污染会话状态 */
  failed_sources: string[]
  /** 连续失败轮数；单次瞬时错误（下一轮心跳自愈）不置异常 */
  consecutive_failures: number
}

export interface DataInfo {
  path: string
  size_bytes: number
}

export interface SourceInfoDto {
  id: string
  platform: string
  name: string
  available: boolean
  capabilities: string[]
  reason: string | null
}

export type MainIntentDto =
  | { kind: "overview"; settings: AppSettings }
  | { kind: "settings"; section: string }
  | { kind: "about"; version: string }

/** 在系统默认浏览器打开链接。桌面端走 opener 插件，浏览器预览用新标签 */
export async function openUrl(url: string): Promise<void> {
  if (!isTauri) {
    window.open(url, "_blank", "noopener,noreferrer")
    return
  }
  const { openUrl: open } = await import("@tauri-apps/plugin-opener")
  await open(url)
}

/** 一个额度窗口。usedPercent 为 null 时**不画进度条**（§7.4） */
export interface QuotaWindowDto {
  key: string
  window_name: string
  used_percent: number | null
  amount_text: string | null
  resets_at: string | null
}

/**
 * 额度查询状态（§2.5）。五种失败态各带原因，界面显示「—」+ 原因，
 * 绝不渲染为 0% 或满格。
 */
export type QuotaStateDto =
  | { state: "ok"; windows: QuotaWindowDto[]; plan: string | null }
  | { state: "unsupported"; reason: string }
  | { state: "forbidden"; reason: string }
  | { state: "unauthorized"; reason: string }
  | { state: "rate_limited"; reason: string }
  | { state: "failed"; reason: string }

/** 余额查询状态。真实余额为零显示 0.00，查不到才显示「—」 */
export type BalanceStateDto =
  | { state: "ok"; balance: number; currency: string; used: number | null }
  | { state: "unsupported"; reason: string }
  | { state: "forbidden"; reason: string }
  | { state: "unauthorized"; reason: string }
  | { state: "rate_limited"; reason: string }
  | { state: "failed"; reason: string }

/** 直连 API Key 的官方组织用量。普通 Key 无管理权限时返回 forbidden。 */
export type ApiUsageStateDto =
  | {
      state: "ok"
      input_tokens: number
      output_tokens: number
      cache_read_tokens: number
      cache_write_tokens: number
      total_tokens: number
      cost_usd: number | null
      cost_reason: string | null
      source: string
    }
  | { state: "unsupported"; reason: string }
  | { state: "forbidden"; reason: string }
  | { state: "unauthorized"; reason: string }
  | { state: "rate_limited"; reason: string }
  | { state: "failed"; reason: string }

/** 估算费用（§2.4）。estimated 恒为 true —— 本地记录不含账单 */
export interface CostEstimateDto {
  amount: number
  estimated: boolean
  /** 价目表未覆盖模型的 Token 数；不为零时界面需说明 */
  uncovered_tokens: number
  complete: boolean
}

/** 停靠边缘（§2.1.3） */
export type DockEdgeDto = "top" | "bottom" | "left" | "right"

export interface SnapResult {
  edge: DockEdgeDto | null
  offset: number
}

/** 持久化设置（§2.6 / §7.3）。托盘与设置界面共享同一份值 */
export interface AppSettings {
  silent_startup: boolean
  island_platform: string
  island_kind: "auth" | "api"
  island_connection_id: string | null
  island_connection_name: string | null
  island_source_id: string | null
  always_on_top: boolean
  theme: "light" | "dark" | "system"
  island_opacity: number
  island_scale: number
  island_shrink_scale: number
  refresh_minutes: number
  balance_alert_threshold: number | null
  balance_alert_currency: string
  dock_enabled: boolean
  retention_days: number | null
  dock: { edge: DockEdgeDto | null; offset: number; monitor: string | null }
  /** 免打扰：只暂停提示与动效，不停止采集与统计 */
  dnd: boolean
  island_visible: boolean
  /** 本地代理：默认关闭，不开启时应用保持纯只读采集行为 */
  proxy_enabled: boolean
  proxy_port: number
  proxy_fallback_direct: boolean
  /** 接管时保存的真实上游（网关地址，非凭证） */
  proxy_claude_upstream: string | null
  proxy_codex_upstream: string | null
}

export interface ProxyStatusDto {
  running: boolean
  port: number
  fallback_reason: string | null
  last_error: string | null
  claude_upstream: string | null
  codex_upstream: string | null
  claude_taken_over: boolean
  codex_taken_over: boolean
}

export interface CleanupPreviewDto {
  cutoff_ms: number
  requests: number
  usage_events: number
  sessions: number
}

export interface ImportSummaryDto {
  requests_changed: number
  sessions_changed: number
}

export interface UpdateCheckDto {
  available: boolean
  current_version: string
  available_version: string
  /** Release notes 摘要；发布未提供时为 null */
  notes: string | null
  pub_date: string | null
}

/** install_update_and_restart 期间由后端 emit 的下载进度（事件名 update-download-progress） */
export interface UpdateProgressDto {
  downloaded: number
  total: number | null
}

/** 通用事件订阅。返回取消订阅函数 */
export async function listenEvent<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  if (!isTauri) return () => {}
  const { listen } = await import("@tauri-apps/api/event")
  return listen<T>(name, (e) => handler(e.payload))
}

/**
 * 订阅后端推送的实时用量。Rust 侧监听 jsonl 写入，写入即推送，
 * 因此这里不做任何定时轮询。返回取消订阅函数。
 */
export async function listenLiveUsage(
  handler: (u: LiveUsageDto) => void,
): Promise<() => void> {
  return listenEvent<LiveUsageDto>("live-usage", handler)
}

export const api = {
  sourceMetrics: (platform: string, period: string, custom: CustomRange | null, model: string | null, queryEndMs: number) => invoke<SourceMetrics>("source_metrics", { platform, period, custom, model, queryEndMs }),
  openMainPanel: () => invoke<void>("open_main_panel"),
  takeMainIntent: () => invoke<MainIntentDto | null>("take_main_intent"),
  menuAction: (id: string) => invoke<void>("menu_action", { id }),
  menuFit: (rootHeight: number) => invoke<{ root_x: number; root_y: number; root_width: number; sub_width: number; side: "left" | "right" }>("menu_fit", { rootHeight }),
  menuShow: () => invoke<void>("menu_show"),
  menuClose: () => invoke<void>("menu_close"),
  menuRefreshing: () => invoke<boolean>("menu_refreshing"),
  islandMenu: (refreshing: boolean) => invoke<void>("island_menu", { refreshing }),
  panelMenu: () => invoke<void>("panel_menu"),
  islandDrag: () => invoke<void>("island_drag"),
  mainDrag: () => invoke<void>("main_drag"),
  islandTopmost: (on: boolean) => invoke<AppSettings>("island_topmost", { on }),
  setDockEnabled: (on: boolean) => invoke<AppSettings>("set_dock_enabled", { on }),
  setTheme: (theme: "light" | "dark" | "system") => invoke<AppSettings>("set_theme", { theme }),
  setBalanceAlert: (threshold: number | null, currency: string) => invoke<AppSettings>("set_balance_alert", { threshold, currency }),
  getPlanQueryStatus: () => invoke<PlanQueryStatusDto>("get_plan_query_status"),
  /** volcSecret 传 null 表示保持不变（不必重填），空串表示清除 */
  setPlanQuery: (input: { zhipuTeamOrganizationId: string; zhipuTeamProjectId: string; volcAccessKeyId: string; volcSecretAccessKey: string | null }) =>
    invoke<PlanQueryStatusDto>("set_plan_query", {
      zhipuTeamOrganizationId: input.zhipuTeamOrganizationId,
      zhipuTeamProjectId: input.zhipuTeamProjectId,
      volcAccessKeyId: input.volcAccessKeyId,
      volcSecretAccessKey: input.volcSecretAccessKey,
    }),
  setDisplayPreferences: (opacity: number, scale: number, shrinkScale: number, refreshMinutes: number) => invoke<AppSettings>("set_display_preferences", { opacity, scale, shrinkScale, refreshMinutes }),
  setRetentionDays: (days: number | null) => invoke<AppSettings>("set_retention_days", { days }),
  cleanupPreview: (days: number) => invoke<CleanupPreviewDto>("cleanup_preview", { days }),
  cleanupHistory: (days: number) => invoke<CleanupPreviewDto>("cleanup_history", { days }),
  readLocalConnections: (platform: string, kind: "auth" | "api", name?: string | null) => invoke<{ added: number; existing: number; warnings?: string[] }>("read_local_connections", { platform, kind, name: name ?? null }),
  importDataFile: () => invoke<ImportSummaryDto | null>("import_data_file"),
  exportDataFile: (platform: string | null) => invoke<string | null>("export_data_file", { platform }),
  exportData: (platform: string | null) => invoke<string>("export_data", { platform }),
  importData: (json: string) => invoke<ImportSummaryDto>("import_data", { json }),
  scanLocalSessions: () => invoke<ScanResult>("scan_local_sessions"),
  collectionStatus: () => invoke<CollectionStatusDto>("collection_status"),
  dataInfo: () => invoke<DataInfo>("data_info"),
  openDataDirectory: () => invoke<void>("open_data_directory"),
  autostartStatus: () => invoke<boolean>("autostart_status"),
  setAutostart: (on: boolean) => invoke<boolean>("set_autostart", { on }),
  /** 只查询是否有新版本，不下载（§更新：启动 1 秒后与设置页手动检查共用） */
  checkAppUpdate: () => invoke<UpdateCheckDto>("check_app_update_available"),
  /** 下载 → 校验签名 → 安装；Windows 上成功返回前后进程会直接重启退出 */
  installUpdate: () => invoke<boolean>("install_update_and_restart"),
  setSilentStartup: (on: boolean) => invoke<AppSettings>("set_silent_startup", { on }),
  tokenTotals: (platform: string, custom: CustomRange | null, model: string | null, queryEndMs: number) =>
    invoke<PeriodTotals>("token_totals", { platform, custom, model, queryEndMs }),
  usageBreakdown: (
    platform: string,
    period: string,
    custom: CustomRange | null,
    model: string | null,
    queryEndMs: number,
  ) => invoke<UsageBreakdown>("usage_breakdown", { platform, period, custom, model, queryEndMs }),
  listModels: (platform: string, period: string, custom: CustomRange | null, queryEndMs: number) =>
    invoke<string[]>("list_models", { platform, period, custom, queryEndMs }),
  usageTrend: (
    platform: string,
    period: string,
    custom: CustomRange | null,
    model: string | null,
    queryEndMs: number,
  ) => invoke<Trend>("usage_trend", { platform, period, custom, model, queryEndMs }),
  requestLog: (
    platform: string,
    period: string,
    page: number,
    custom: CustomRange | null,
    model: string | null,
    queryEndMs: number,
  ) => invoke<LogPage>("request_log", { platform, period, page, custom, model, queryEndMs }),
  connectionQuota: (id: string, force = false) => invoke<QuotaStateDto>("connection_quota", { id, force }),
  connectionApiUsage: (id: string, force = false) => invoke<ApiUsageStateDto>("connection_api_usage", { id, force }),
  connectionBalance: (id: string, force = false) => invoke<BalanceStateDto>("connection_balance", { id, force }),
  costEstimate: (
    platform: string,
    period: string,
    custom: CustomRange | null,
    model: string | null,
    queryEndMs: number,
  ) => invoke<CostEstimateDto>("cost_estimate", { platform, period, custom, model, queryEndMs }),
  dockHover: () => invoke<DockEdgeDto | null>("dock_hover"),
  dockRelease: () => invoke<SnapResult | null>("dock_release"),
  dockUndock: () => invoke<void>("dock_undock"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  listSources: (platform: string) => invoke<SourceInfoDto[]>("list_sources", { platform }),
  setIslandPlatform: (platform: string) =>
    invoke<AppSettings>("set_island_platform", { platform }),
  setIslandConnection: (id: string | null) =>
    invoke<AppSettings>("set_island_connection", { id }),
  setIslandSource: (id: string | null) =>
    invoke<AppSettings>("set_island_source", { id }),
  setDnd: (on: boolean) => invoke<AppSettings>("set_dnd", { on }),
  proxyStatus: () => invoke<ProxyStatusDto>("proxy_status"),
  /** 开关切换即执行 CLI 配置接管（或还原直连）并启动（或停止）监听；失败原因直接抛出 */
  setProxy: (port: number, enabled: boolean, fallbackDirect: boolean) =>
    invoke<AppSettings>("set_proxy", { port, enabled, fallbackDirect }),
  listConnections: () => invoke<ConnectionDto[]>("list_connections"),
  addConnection: (input: {
    platform: string
    kind: "auth" | "api"
    name: string
    secret?: string | null
    base_url?: string | null
    model?: string | null
    effort?: string | null
    context_1m?: boolean | null
  }) => invoke<string>("add_connection", { input }),
  /** 「获取模型」：用用户填写的地址与 Key 拉取网关模型列表（画布 18）。Tauri 参数走 camelCase */
  fetchRemoteModels: (platform: string, base_url: string, secret: string) =>
    invoke<string[]>("fetch_remote_models", { platform, baseUrl: base_url, secret }),
  setConnectionPaused: (id: string, paused: boolean) => invoke<void>("set_connection_paused", { id, paused }),
  /** 启用连接：把凭证与地址写入对应 CLI 的配置文件（cc-switch 式切换），返回说明文案 */
  enableConnection: (id: string) => invoke<string>("enable_connection", { id }),
  /** 编辑回显：返回连接保存的完整凭证（编辑弹窗回填与小眼睛用） */
  revealConnectionSecret: (id: string) => invoke<string>("reveal_connection_secret", { id }),
  /** 检测连接：用保存的凭证对上游做一次轻量验证并计时，无副作用 */
  testConnection: (id: string) =>
    invoke<{ ok: boolean; latency_ms: number; message: string | null }>("test_connection", { id }),
  /** 当前模型支持的思考强度档位（数据库规则匹配；非 Tauri 环境抛错由调用方兜底） */
  getEffortOptions: (model?: string | null) => invoke<string[]>("get_effort_options", { model: model ?? null }),
  /** 「获取强度」：调官方 /v1/models 解析各模型的 capabilities.effort 并存入本机数据库 */
  refreshEffortLevels: (platform: string, base_url: string, secret: string) =>
    invoke<string>("refresh_effort_levels", { platform, baseUrl: base_url, secret }),
  /** 统一编辑连接：完整快照式更新；secret 空串表示不换 Key，地址或 Key 变化时后端先验证 */
  updateConnection: (input: {
    id: string
    name: string
    base_url?: string | null
    secret?: string | null
    model?: string | null
    effort?: string | null
    context_1m?: boolean | null
  }) => invoke<void>("update_connection", { input }),
  removeConnection: (id: string) => invoke<void>("remove_connection", { id }),
  fetchCredentials: (id: string) => invoke<FetchResult>("fetch_credentials", { id }),
  replaceApiKey: (id: string, secret: string) => invoke<void>("replace_api_key", { id, secret }),
  renameConnection: (id: string, name: string) => invoke<void>("rename_connection", { id, name }),
  discoverConnections: () => invoke<Candidate[]>("discover_connections"),
  connectionKind: (platform: string) =>
    invoke<ConnectionInfo>("connection_kind", { platform }),
  logFront: (msg: string) => invoke<void>("log_front", { msg }).catch(() => {}),
  liveUsage: (platform: string, since: number | null) =>
    invoke<LiveUsageDto>("live_usage", { platform, since }),
}

/** 紧凑格式：K / M / B，保留 Token 单位或所属统计标题（§3） */
export function compactTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(2)}B`
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

/** 千位分隔 + 等宽数字由 CSS 负责；缺失值由调用方显示「—」 */
export function grouped(n: number): string {
  return n.toLocaleString("en-US")
}
