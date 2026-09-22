/**
 * 样例数据 —— 全部取自 pencil-new.pen 中的画布内容，不自行编造。
 * 需求文档反复强调：所有数值均为设计示例，不代表已接入或接口验证结果。
 */
import type {
  Balance,
  Connection,
  QuotaWindow,
  RequestLogRow,
  SessionActivity,
  StatSource,
  TokenSummary,
} from "@/types"

export const CONNECTIONS: Connection[] = [
  {
    id: "c1",
    platformId: "claude",
    kind: "auth",
    name: "个人账号",
    baseName: "个人账号",
    label: "官方订阅（20x）",
    masked: "",
    status: "connected",
    lastSyncText: "12 秒前",
    baseUrl: null,
    model: null,
    effort: null,
    context1m: null,
  },
  {
    id: "c2",
    platformId: "claude",
    kind: "api",
    name: "个人 API Key · sk-ant-****3f9a",
    baseName: "个人 API Key",
    label: "API Key",
    masked: "sk-ant-****3f9a",
    status: "connected",
    lastSyncText: "1 分钟前",
    baseUrl: null,
    model: "claude-sonnet-4-5",
    effort: "medium",
    context1m: false,
  },
  {
    id: "c3",
    platformId: "codex",
    kind: "auth",
    name: "工作账号",
    baseName: "工作账号",
    label: "官方订阅（Plus）",
    masked: "",
    status: "expired",
    lastSyncText: "2 小时前",
    baseUrl: null,
    model: null,
    effort: null,
    context1m: null,
  },
]

/** Claude 官方账号双额度（画布 01 / 03 / 08） */
export const CLAUDE_QUOTAS: QuotaWindow[] = [
  {
    key: "5h",
    windowName: "5 小时窗口 · 已用",
    usedPercent: 10,
    usedText: "已用 1.2k / 12k 消息",
    resetCountdown: "4h 51m",
    badgeTone: "purple",
  },
  {
    key: "7d",
    windowName: "7 天窗口 · 已用",
    usedPercent: 59,
    usedText: "已用 44.3k / 75k 消息",
    resetCountdown: "3d 7h",
    badgeTone: "green",
  },
]

/** Codex 双额度（画布 02 变体 A） */
export const CODEX_QUOTAS: QuotaWindow[] = [
  {
    key: "5h",
    windowName: "5 小时窗口 · 已用",
    usedPercent: 52,
    usedText: null,
    resetCountdown: "2h 30m",
    badgeTone: "purple",
  },
  {
    key: "7d",
    windowName: "7 天窗口 · 已用",
    usedPercent: 72,
    usedText: null,
    resetCountdown: "5d 8h",
    badgeTone: "green",
  },
]

/** 单额度示例（画布 02 变体 B） */
export const SINGLE_QUOTA: QuotaWindow[] = [
  {
    key: "1d",
    windowName: "1 天窗口 · 已用",
    usedPercent: 36,
    usedText: null,
    resetCountdown: "11h 2m",
    badgeTone: "blue",
  },
]

export const SESSIONS: SessionActivity[] = [
  { id: "s1", title: "修复登录问题", deltaText: "+8.0k Token", state: "running" },
  { id: "s2", title: "编写单元测试", deltaText: "+3.2k Token", state: "running" },
  {
    id: "s3",
    // 画布 03：演示「过长时单行省略并以…结尾」
    title: "整理接口文档与第三方鉴权流程说明与回归测试清单",
    deltaText: "+1.2k Token",
    state: "running",
  },
]

export const TOTAL_DELTA_TEXT = "+12.4k Token"
export const RUNNING_SESSION_TEXT = "3 个会话运行中"

export const LOCAL_SOURCE: StatSource = {
  id: "local",
  name: "本地会话记录",
  scope: "本机 Claude CLI 会话记录",
  perConnection: false,
  realtimeDelta: true,
  lastUpdatedText: "12 秒前",
}

export const API_SOURCE: StatSource = {
  id: "api",
  name: "API 汇总",
  scope: "账户共享范围",
  perConnection: false,
  realtimeDelta: false,
  lastUpdatedText: "34 秒前",
}

export const TOKEN_SUMMARIES: TokenSummary[] = [
  { period: "today", label: "今日", value: "12.84M", hint: "本地会话记录" },
  { period: "week", label: "本周", value: "84.21M", hint: "自周一 00:00 起" },
  { period: "month", label: "本月", value: "301.42M", hint: "自 09/01 起" },
  { period: "total", label: "累计", value: "1.24B", hint: "采集起点 2026/03/02" },
]

/**
 * 今日按小时分桶的 24 点趋势示例（画布 08）。只用于浏览器预览——
 * 桌面端一律走真实查询，不允许把设计示例当成真实数据（§VIEW-01）。
 *
 * 数值形状刻意贴近真实：缓存命中远大于新增输入，第 22 点的缓存创建为 null，
 * 用来验证「未知值断线」这条规则在预览里也看得见。
 */
const DEMO_FRESH_INPUT = [
  1_200, 800, 600, 600, 900, 2_100, 4_800, 12_000, 26_000, 41_000, 58_000, 63_000,
  47_000, 38_000, 72_000, 96_000, 118_000, 102_000, 84_000, 63_000, 48_000, 33_000, 21_000, 11_000,
]
const DEMO_OUTPUT = [
  400, 260, 180, 180, 300, 700, 1_500, 3_800, 8_100, 12_800, 18_000, 19_600,
  14_600, 11_800, 22_400, 29_800, 36_600, 31_700, 26_100, 19_600, 14_900, 10_200, 6_500, 3_400,
]
const DEMO_CACHE_WRITE: Array<number | null> = [
  0, 0, 0, 0, 0, 1_100, 2_600, 6_400, 13_800, 21_700, 30_600, 33_300,
  24_800, 20_100, 38_100, 50_700, 62_200, 53_900, 44_300, 33_300, 25_400, 17_400, null, 5_800,
]
const DEMO_CACHE_READ = [
  18_000, 12_000, 9_000, 9_000, 13_500, 31_500, 72_000, 180_000, 390_000, 615_000, 870_000, 945_000,
  705_000, 570_000, 1_080_000, 1_440_000, 1_770_000, 1_530_000, 1_260_000, 945_000, 720_000, 495_000, 315_000, 165_000,
]

export const DEMO_TREND = {
  labels: Array.from({ length: 24 }, (_, index) => String(index).padStart(2, "0")),
  series: [
    { key: "fresh_input", label: "新增输入", colorVar: "--chart-fresh-input", values: DEMO_FRESH_INPUT },
    { key: "output", label: "输出", colorVar: "--chart-output", values: DEMO_OUTPUT },
    { key: "cache_write", label: "缓存创建", colorVar: "--chart-cache-write", values: DEMO_CACHE_WRITE },
    { key: "cache_read", label: "缓存命中", colorVar: "--chart-cache-read", values: DEMO_CACHE_READ },
  ],
  // 倒数第二点无法估算，验证成本线的断线渲染
  cost: DEMO_FRESH_INPUT.map((value, index) =>
    index === 22 ? null : Number(((value * 3 + DEMO_OUTPUT[index] * 15) / 1_000_000).toFixed(4)),
  ),
  bucket: "1 小时",
}

/**
 * 分项示例。这里给的是「字段齐全」的正常态，方便在浏览器里核对命中率进度条的样子；
 * 字段缺失时显示「—」的那条路径由 Codex 真机数据和后端测试覆盖，趋势示例里也留了
 * 一个 null 点（第 22 点的缓存创建）验证断线渲染。
 */
export const DEMO_BREAKDOWN = {
  fresh_input: 2_491_000,
  output: 294_000,
  cache_write: 1_842_000,
  cache_read: 92_119_000,
  real_total: 94_902_897,
  requests: 568,
  cache_hit_rate: 92_119_000 / (2_491_000 + 1_842_000 + 92_119_000),
  avg_per_request: 94_902_897 / 568,
}

export const DEMO_MODELS = ["claude-opus-4-1", "claude-sonnet-4"]

export const LOG_ROWS: RequestLogRow[] = [
  {
    id: "l1",
    time: "02:18:32",
    date: "09/14",
    model: "claude-sonnet-4-5-20250929",
    effort: "high",
    input: "1,927",
    output: "173",
    cost: "$0.1334",
    durationMs: 2150,
    firstTokenMs: 340,
    status: { kind: "success", label: "200" },
  },
  {
    id: "l2",
    time: "02:17:08",
    date: "09/14",
    // 画布 08：演示计费模型列的单行省略
    model: "claude-opus-4-1-20250805-thinking-high-context",
    effort: "medium",
    input: "1,356",
    output: "1,122",
    cost: "$0.1740",
    durationMs: 64000,
    firstTokenMs: 12000,
    status: { kind: "success", label: "200" },
  },
  {
    id: "l3",
    time: "02:16:45",
    date: "09/14",
    model: "claude-sonnet-4-5-20250929",
    effort: null,
    input: null,
    output: null,
    cost: null,
    durationMs: 190000,
    firstTokenMs: 35000,
    status: { kind: "rate-limited", label: "429" },
  },
  {
    id: "l4",
    time: "02:15:02",
    date: "09/14",
    // 计费模型未知：显示「—」，不以平台名或猜测模型替代
    model: null,
    effort: "low",
    input: "842",
    output: "96",
    cost: "$0.0091",
    costEstimated: true,
    durationMs: 320000,
    firstTokenMs: 72000,
    // 本地日志没有 HTTP 响应码时按「成功」展示（能写入用量即响应已返回），不补造 200
    status: { kind: "success", label: "成功" },
  },
]

export const LOG_TOTAL_COUNT = 161
export const LOG_PAGE_SIZE = 10 // 界面上不显示该文案（§2.4）
export const LOG_PAGE_COUNT = 17

export const BALANCE: Balance = {
  queryState: "loaded",
  amountText: "$18.72",
  currency: "USD",
  scopeLabel: "账户共享余额",
  sourceText: "数据来源：服务商账单接口 · 更新于 1 分钟前",
  level: "healthy",
}

export const API_TODAY = { token: "128.4K", cost: "$1.28" }

export const TRAY_TOOLTIP = {
  app: "CC Usage",
  connection: "Claude · 官方订阅（20x）",
  quotas: [
    { key: "5h", value: "10%" },
    { key: "7d", value: "59%" },
  ],
  updated: "最近更新 12 秒前",
}
