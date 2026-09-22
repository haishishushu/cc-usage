import { useEffect, useRef, useState } from "react"
import { totalInputTokens } from "./requestInput.ts"
import {
  api,
  compactTokens,
  grouped,
  isTauri,
  type CustomRange,
  type LogPage,
  type Trend,
  type UsageBreakdown,
} from "./api"
import {
  DEMO_BREAKDOWN,
  DEMO_MODELS,
  DEMO_TREND,
  LOG_PAGE_COUNT,
  LOG_ROWS,
  LOG_TOTAL_COUNT,
  TOKEN_SUMMARIES,
} from "@/mock/data"
import type { PlatformId, RequestLogRow, StatPeriod, TokenSummary } from "@/types"

/**
 * 真实数据接入。桌面端走 Tauri 命令，浏览器预览回落到设计示例。
 * `live` 标记当前用的是哪一种，界面据此说明来源，不把示例伪装成真实数据。
 */

const PERIOD_HINT: Record<string, string> = {
  today: "本地会话记录",
  week: "自周一 00:00 起",
  month: "自月初起",
  total: "采集起点 —",
}

export function useTokenSummaries(
  platform: PlatformId,
  custom: CustomRange | null,
  refreshKey: number,
  queryEndMs: number,
  model: string | null = null,
) {
  const [data, setData] = useState<TokenSummary[]>(isTauri ? [] : TOKEN_SUMMARIES)
  const [live, setLive] = useState(false)
  const [status, setStatus] = useState<"loading" | "ready" | "empty" | "failed">(isTauri ? "loading" : "ready")
  const [error, setError] = useState<string | null>(null)

  const completedQuery = useRef<string | null>(null)
  useEffect(() => {
    let cancelled = false
    if (!isTauri) return
    const query = JSON.stringify([platform, custom?.start, custom?.end, model])
    if (completedQuery.current !== query) {
      setData([])
      setLive(false)
      setStatus("loading")
    }
    setError(null)
    api
      .tokenTotals(platform, custom, model, queryEndMs)
      .then((t) => {
        if (cancelled) return
        completedQuery.current = query
        setData([
          { period: "today", label: "今日", value: t.today === null ? "—" : compactTokens(t.today), hint: PERIOD_HINT.today },
          { period: "week", label: "本周", value: t.week === null ? "—" : compactTokens(t.week), hint: PERIOD_HINT.week },
          { period: "month", label: "本月", value: t.month === null ? "—" : compactTokens(t.month), hint: PERIOD_HINT.month },
          {
            period: "total",
            label: "累计",
            value: t.total === null ? "—" : compactTokens(t.total),
            // 累计从现有最早记录起，并展示采集起点，不能称为账号终生消耗（§7.5）
            hint: t.collected_since ? `采集起点 ${t.collected_since}` : "暂无记录",
          },
        ])
        setLive(true)
        setStatus([t.today, t.week, t.month, t.total].some((value) => value === null || value > 0) ? "ready" : "empty")
      })
      .catch((reason) => {
        if (cancelled) return
        setData([])
        setLive(false)
        setStatus("failed")
        setError(String(reason))
      })
    return () => {
      cancelled = true
    }
  }, [platform, custom, refreshKey, queryEndMs, model])

  return { summaries: data, live, status, error }
}

/**
 * 区间内的 Token 分项。未知字段保持 null 一路透传到界面显示「—」，
 * 任何一层都不得把「来源没说」补成 0。
 */
export function useUsageBreakdown(
  platform: PlatformId,
  period: StatPeriod,
  custom: CustomRange | null,
  refreshKey: number,
  queryEndMs: number,
  model: string | null = null,
) {
  const [data, setData] = useState<UsageBreakdown | null>(isTauri ? null : DEMO_BREAKDOWN)
  const [status, setStatus] = useState<"loading" | "ready" | "failed">(isTauri ? "loading" : "ready")
  const [error, setError] = useState<string | null>(null)

  const completedQuery = useRef<string | null>(null)
  useEffect(() => {
    let cancelled = false
    if (!isTauri || (period === "custom" && !custom)) return
    const query = JSON.stringify([platform, period, custom?.start, custom?.end, model])
    if (completedQuery.current !== query) {
      setData(null)
      setStatus("loading")
    }
    setError(null)
    api
      .usageBreakdown(platform, period, custom, model, queryEndMs)
      .then((breakdown) => {
        if (cancelled) return
        completedQuery.current = query
        setData(breakdown)
        setStatus("ready")
      })
      .catch((reason) => {
        if (cancelled) return
        setData(null)
        setStatus("failed")
        setError(String(reason))
      })
    return () => {
      cancelled = true
    }
  }, [platform, period, custom, refreshKey, queryEndMs, model])

  return { breakdown: data, status, error }
}

/**
 * 当前范围内出现过的模型名。查询失败时返回空列表——
 * 筛选器少几个选项不影响主数据，不值得把整页拦下来报错。
 */
export function useModels(
  platform: PlatformId,
  period: StatPeriod,
  custom: CustomRange | null,
  refreshKey: number,
  queryEndMs: number,
) {
  const [models, setModels] = useState<string[]>(isTauri ? [] : DEMO_MODELS)

  useEffect(() => {
    let cancelled = false
    if (!isTauri || (period === "custom" && !custom)) return
    api
      .listModels(platform, period, custom, queryEndMs)
      .then((list) => {
        if (!cancelled) setModels(list)
      })
      .catch(() => {
        if (!cancelled) setModels([])
      })
    return () => {
      cancelled = true
    }
  }, [platform, period, custom, refreshKey, queryEndMs])

  return models
}

/** 四条 Token 系列的固定顺序与配色槽位；顺序不随数据变化，颜色跟着含义走 */
export const TOKEN_SERIES = [
  { key: "fresh_input", label: "新增输入", colorVar: "--chart-fresh-input" },
  { key: "output", label: "输出", colorVar: "--chart-output" },
  { key: "cache_write", label: "缓存创建", colorVar: "--chart-cache-write" },
  { key: "cache_read", label: "缓存命中", colorVar: "--chart-cache-read" },
] as const

export interface ChartData {
  /** X 轴刻度文字，与各系列的数组下标一一对应 */
  labels: string[]
  series: Array<{ key: string; label: string; colorVar: string; values: Array<number | null> }>
  /** 估算费用，与 labels 同长；null 表示该桶无法估算，图上断线 */
  cost: Array<number | null>
  bucket: string
}

const EMPTY_CHART = (bucket: string): ChartData => ({ labels: [], series: [], cost: [], bucket })

function toChart(trend: Trend): ChartData {
  if (!trend.points.length) return EMPTY_CHART(trend.bucket)
  return {
    labels: trend.points.map((point) => point.label),
    series: TOKEN_SERIES.map((spec) => ({
      key: spec.key,
      label: spec.label,
      colorVar: spec.colorVar,
      values: trend.points.map((point) => point[spec.key]),
    })),
    cost: trend.points.map((point) => point.cost),
    bucket: trend.bucket,
  }
}

export function useTrend(
  platform: PlatformId,
  period: StatPeriod,
  custom: CustomRange | null,
  refreshKey: number,
  queryEndMs: number,
  model: string | null = null,
) {
  const [data, setData] = useState<ChartData>(isTauri ? EMPTY_CHART("读取中") : DEMO_TREND)
  const [live, setLive] = useState(false)
  const [status, setStatus] = useState<"loading" | "ready" | "empty" | "failed">(isTauri ? "loading" : "ready")
  const [error, setError] = useState<string | null>(null)

  const completedQuery = useRef<string | null>(null)
  useEffect(() => {
    let cancelled = false
    // 自定义周期必须有已生效的范围才查，否则等用户点「确定」
    if (!isTauri || (period === "custom" && !custom)) return
    const query = JSON.stringify([platform, period, custom?.start, custom?.end, model])
    if (completedQuery.current !== query) {
      setData(EMPTY_CHART("读取中"))
      setLive(false)
      setStatus("loading")
    }
    setError(null)
    api
      .usageTrend(platform, period, custom, model, queryEndMs)
      .then((t) => {
        if (cancelled) return
        completedQuery.current = query
        setData(toChart(t))
        setLive(true)
        setStatus(t.points.length ? "ready" : "empty")
      })
      .catch((reason) => {
        if (cancelled) return
        setData(EMPTY_CHART("查询失败"))
        setLive(false)
        setStatus("failed")
        setError(String(reason))
      })
    return () => {
      cancelled = true
    }
  }, [platform, period, custom, refreshKey, queryEndMs, model])

  return { chart: data, live, status, error }
}

/**
 * 上游真实响应码 → 状态标签。本地会话记录没有 HTTP 响应码，但能写入用量
 * 说明响应已成功返回（失败的调用不会产生完整 usage），故按「成功」展示；
 * 仍不标注具体状态码——伪造 200 会让真实的失败请求看起来一切正常。
 */
function statusFromCode(code: number | null): RequestLogRow["status"] {
  if (code === null) return { kind: "success", label: "成功" }
  if (code === 429) return { kind: "rate-limited", label: String(code) }
  if (code >= 200 && code < 300) return { kind: "success", label: String(code) }
  return { kind: "failed", label: String(code) }
}

function toLogRows(page: LogPage): RequestLogRow[] {
  return page.rows.map((r) => {
    // 各平台的 input_tokens 语义不同，先归一成实际输入总量再展示，
    // 否则 Claude 这种缓存命中率高的会只显示个位数的新增输入（§7.5）
    const inputTotal = totalInputTokens(r)
    const cacheSplit = r.cache_read !== null && r.cache_write !== null
    return {
    id: String(r.id),
    time: r.time,
    date: r.date,
    model: r.model,
    effort: r.effort,
    input: inputTotal === null ? null : grouped(inputTotal),
    inputHint: inputTotal === null || !cacheSplit || r.input === null
      ? null
      : `新增 ${grouped(r.input_semantics === "includes_cache" || (!r.input_semantics && r.platform === "codex") ? Math.max(0, r.input - r.cache_read!) : r.input)} · 缓存命中 ${grouped(r.cache_read!)} · 缓存创建 ${grouped(r.cache_write!)}`,
    output: r.output === null ? null : grouped(r.output),
    // 本地记录没有账单金额，这里是按价目表推算的**估算值**，
    // 界面已在表头与提示中标注「估算」；价目表未覆盖的模型仍显示「—」（§2.4）
    cost: r.cost_estimate === null
      ? null
      : r.cost_estimate > 0 && r.cost_estimate < 0.0001
        ? "<$0.0001"
        : `$${r.cost_estimate.toFixed(4)}`,
    // 本地记录推算出的费用一律标「估算」，不得当成账单金额（§2.4）
    costEstimated: r.cost_estimate !== null,
    // 耗时与首字只有经本地代理转发时才测得到；会话记录里没有，保持 null 显示「—」
    durationMs: r.duration_ms,
    firstTokenMs: r.first_token_ms,
    // 有上游真实响应码按码判定；没有则按「成功」展示（能写入用量即响应已返回），不补造 200
    status: r.native_outcome === "failed" ? { kind: "failed", label: "失败" } : r.native_outcome === "success" ? { kind: "success", label: "成功" } : r.platform !== "claude" && r.platform !== "codex" && r.status_code === null ? { kind: "unknown", label: "未上报状态" } : statusFromCode(r.status_code),
    }
  })
}

export function useRequestLog(
  platform: PlatformId,
  period: StatPeriod,
  page: number,
  custom?: CustomRange | null,
  refreshKey = 0,
  queryEndMs = 0,
  model: string | null = null,
) {
  const [rows, setRows] = useState<RequestLogRow[]>(isTauri ? [] : LOG_ROWS)
  const [totalCount, setTotalCount] = useState<number | null>(isTauri ? 0 : LOG_TOTAL_COUNT)
  const [pageCount, setPageCount] = useState(isTauri ? 1 : LOG_PAGE_COUNT)
  const [live, setLive] = useState(false)
  const [status, setStatus] = useState<"loading" | "ready" | "empty" | "failed">(isTauri ? "loading" : "ready")
  const [error, setError] = useState<string | null>(null)

  const completedQuery = useRef<string | null>(null)
  useEffect(() => {
    let cancelled = false
    if (!isTauri || (period === "custom" && !custom)) return
    const query = JSON.stringify([platform, period, page, custom?.start, custom?.end, model])
    if (completedQuery.current !== query) {
      setRows([])
      setTotalCount(0)
      setPageCount(1)
      setLive(false)
      setStatus("loading")
    }
    setError(null)
    api
      .requestLog(platform, period, page, custom ?? null, model, queryEndMs)
      .then((p) => {
        if (cancelled) return
        completedQuery.current = query
        setRows(toLogRows(p))
        setTotalCount(p.total_count)
        setPageCount(p.page_count)
        setLive(true)
        setStatus(p.rows.length ? "ready" : "empty")
      })
      .catch((reason) => {
        if (cancelled) return
        setRows([])
        setTotalCount(0)
        setPageCount(1)
        setLive(false)
        setStatus("failed")
        setError(String(reason))
      })
    return () => {
      cancelled = true
    }
  }, [platform, period, page, custom, refreshKey, queryEndMs, model])

  return { rows, totalCount, pageCount, live, status, error }
}
