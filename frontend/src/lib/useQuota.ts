import { useSettings } from "./useSettings"
import { refreshDelay } from "./displayPreferences"
import { useCallback, useEffect, useRef, useState } from "react"
import {
  api,
  listenEvent,
  localCodexQuota,
  localGrokQuota,
  isTauri,
  type BalanceStateDto,
  type ApiUsageStateDto,
  type CostEstimateDto,
  type CustomRange,
  type QuotaStateDto,
  type LiveUsageDto,
} from "./api"

/**
 * 额度与余额查询（§2.5 / §7.4）
 *
 * 查询失败**绝不渲染为 0% 或满格**：后端返回带状态标签的联合类型，
 * 界面按状态显示「—」与具体原因。
 *
 * 刷新失败但有旧值时保留旧值并标注过期与最后更新时间（§2.5）。
 */
/**
 * 结构性结论：该来源本就不提供额度（unsupported），或凭证无权限/已失效
 * （forbidden / unauthorized）。这些不是临时故障，切换连接后必须直接替换旧值，
 * 不能把上一个来源的额度当成“过期数据”继续画进度条。
 * 仅 failed / rate_limited 属于临时故障，保留旧值并标过期。
 */
function isDefinitiveQuota(r: QuotaStateDto): boolean {
  return r.state === "unsupported" || r.state === "forbidden" || r.state === "unauthorized"
}

export interface QuotaResult {
  state: QuotaStateDto | null
  failure: Exclude<QuotaStateDto, { state: "ok" }> | null
  /** 正在查询 —— 对应「加载」态 */
  loading: boolean
  /** 上次成功查询的时间；失败时用于标注「数据可能已过期」 */
  fetchedAt: Date | null
  /** 当前展示的是上次成功的旧值 */
  stale: boolean
  refresh: () => Promise<void>
}

/**
 * 本地直查模式：`true` = 本机 Codex（历史布尔签名，保持兼容），`"grok"` = 本机 Grok。
 * 两者都不依赖连接，凭证分别来自 ~/.codex 与 ~/.grok。
 */
export function useQuota(connectionId: string | null, local: boolean | "grok" = false, enabled = true): QuotaResult {
  const interval = refreshDelay(useSettings().settings.refresh_minutes)
  const requestId = useRef(0)
  const queryKey = `${connectionId}:${local}`
  const owner = useRef(queryKey)
  const [state, setState] = useState<QuotaStateDto | null>(null)
  const [loading, setLoading] = useState(false)
  const [fetchedAt, setFetchedAt] = useState<Date | null>(null)
  const [stale, setStale] = useState(false)
  const [failure, setFailure] = useState<Exclude<QuotaStateDto, { state: "ok" }> | null>(null)

  const run = useCallback(async (force = false) => {
    if (!isTauri || (!connectionId && !local)) return
    const current = ++requestId.current
    setLoading(true)
    try {
      const r = local === true
        ? await localCodexQuota(force)
        : local === "grok"
          ? await localGrokQuota(force)
          : await api.connectionQuota(connectionId!, force)
      if (current !== requestId.current) return
      if (r.state === "ok") {
        setState(r)
        setFetchedAt(new Date())
        setStale(false)
        setFailure(null)
      } else if (isDefinitiveQuota(r)) {
        // 结构性结论（该来源不支持额度 / 凭证无权限或失效）：切换到无套餐连接后
        // 必须直接替换旧值，不能把上一个来源的额度当成“过期数据”继续显示。
        setState(r)
        setFetchedAt(null)
        setStale(false)
        setFailure(null)
      } else {
        // 仅限流 / 网络失败等临时故障：有旧值就留着并标过期（§2.5）
        setState((prev) => {
          const keep = prev?.state === "ok"
          setStale(keep)
          setFailure(keep ? r : null)
          return keep ? prev : r
        })
      }
    } catch (e) {
      if (current !== requestId.current) return
      const failed = { state: "failed", reason: String(e) } as const
      setState((prev) => {
        const keep = prev?.state === "ok"
        setStale(keep)
        setFailure(keep ? failed : null)
        return keep ? prev : failed
      })
    } finally {
      if (current === requestId.current) setLoading(false)
    }
  }, [connectionId, local])

  useEffect(() => {
    owner.current = queryKey
    setState(null)
    setLoading(false)
    setFetchedAt(null)
    setStale(false)
    setFailure(null)
    return () => { ++requestId.current }
  }, [queryKey])

  useEffect(() => {
    if (!enabled || !isTauri || (!connectionId && !local)) { setLoading(false); return }
    void run(false)
    const timer = window.setInterval(() => void run(true), interval)
    return () => { ++requestId.current; window.clearInterval(timer) }
  }, [run, local, connectionId, interval, enabled])

  useEffect(() => {
    if (!enabled || state?.state !== "ok") return
    const now = Date.now()
    const nextReset = state.windows
      .map((window) => window.resets_at ? Date.parse(window.resets_at) : Number.NaN)
      .filter((time) => Number.isFinite(time) && time > now)
      .sort((a, b) => a - b)[0]
    if (!nextReset) return
    const timer = window.setTimeout(
      () => { void run(true) },
      Math.min(nextReset - now + 1_000, 2_147_000_000),
    )
    return () => window.clearTimeout(timer)
  }, [state, run, enabled])

  const refresh = useCallback(() => run(true), [run])
  if (owner.current !== queryKey) return { state: null, loading: true, fetchedAt: null, refresh, failure: null, stale: false }
  return { state, failure, loading, fetchedAt, stale, refresh }
}

/** 余额的结构性结论，语义同 [`isDefinitiveQuota`]。 */
function isDefinitiveBalance(r: BalanceStateDto): boolean {
  return r.state === "unsupported" || r.state === "forbidden" || r.state === "unauthorized"
}

export interface BalanceResult {
  state: BalanceStateDto | null
  failure: Exclude<BalanceStateDto, { state: "ok" }> | null
  loading: boolean
  fetchedAt: Date | null
  stale: boolean
  refresh: () => Promise<void>
}

export function useBalance(connectionId: string | null, enabled = true): BalanceResult {
  const interval = refreshDelay(useSettings().settings.refresh_minutes)
  const requestId = useRef(0)
  const queryKey = connectionId
  const owner = useRef(queryKey)
  const [state, setState] = useState<BalanceStateDto | null>(null)
  const [loading, setLoading] = useState(false)
  const [fetchedAt, setFetchedAt] = useState<Date | null>(null)
  const [stale, setStale] = useState(false)
  const [failure, setFailure] = useState<Exclude<BalanceStateDto, { state: "ok" }> | null>(null)

  const run = useCallback(async (force = false) => {
    if (!isTauri || !connectionId) return
    const current = ++requestId.current
    setLoading(true)
    try {
      const r = await api.connectionBalance(connectionId, force)
      if (current !== requestId.current) return
      if (r.state === "ok") {
        setState(r)
        setFetchedAt(new Date())
        setStale(false)
        setFailure(null)
      } else if (isDefinitiveBalance(r)) {
        // 结构性结论（该分组无钱包余额 / 无权限 / 凭证失效）：切换连接后直接替换旧值，
        // 不能把上一个来源的余额当成“过期数据”继续显示。
        setState(r)
        setFetchedAt(null)
        setStale(false)
        setFailure(null)
      } else {
        setState((prev) => {
          if (prev?.state === "ok") {
            setStale(true)
            setFailure(r)
            return prev
          }
          setFailure(null)
          return r
        })
      }
    } catch (e) {
      if (current !== requestId.current) return
      const failed = { state: "failed", reason: String(e) } as const
      setState((prev) => {
        const keep = prev?.state === "ok"
        setStale(keep)
        setFailure(keep ? failed : null)
        return keep ? prev : failed
      })
    } finally {
      if (current === requestId.current) setLoading(false)
    }
  }, [connectionId])

  useEffect(() => {
    owner.current = queryKey
    setState(null)
    setLoading(false)
    setFetchedAt(null)
    setStale(false)
    setFailure(null)
    return () => { ++requestId.current }
  }, [queryKey])

  useEffect(() => {
    if (!enabled || !isTauri || !connectionId) { setLoading(false); return }
    void run(false)
    const timer = window.setInterval(() => void run(true), interval)
    return () => { ++requestId.current; window.clearInterval(timer) }
  }, [run, connectionId, interval, enabled])

  const refresh = useCallback(() => run(true), [run])
  if (owner.current !== queryKey) return { state: null, loading: true, fetchedAt: null, refresh, failure: null, stale: false }
  return { state, failure, loading, fetchedAt, stale, refresh }
}

export interface ApiUsageResult {
  state: ApiUsageStateDto | null
  loading: boolean
  fetchedAt: Date | null
  refresh: () => Promise<void>
}

/** Admin Key 优先读取官方组织用量；普通 Key 的 forbidden 由调用方回退到本机统计。 */
export function useApiUsage(connectionId: string | null, enabled = true): ApiUsageResult {
  const interval = refreshDelay(useSettings().settings.refresh_minutes)
  const requestId = useRef(0)
  const queryKey = connectionId
  const owner = useRef(queryKey)
  const [state, setState] = useState<ApiUsageStateDto | null>(null)
  const [loading, setLoading] = useState(false)
  const [fetchedAt, setFetchedAt] = useState<Date | null>(null)

  const run = useCallback(async (force = false) => {
    if (!isTauri || !connectionId) return
    const current = ++requestId.current
    setLoading(true)
    try {
      const result = await api.connectionApiUsage(connectionId, force)
      if (current !== requestId.current) return
      setState(result)
      if (result.state === "ok") setFetchedAt(new Date())
    } catch (error) {
      if (current === requestId.current) setState({ state: "failed", reason: String(error) })
    } finally {
      if (current === requestId.current) setLoading(false)
    }
  }, [connectionId])

  useEffect(() => {
    owner.current = queryKey
    setState(null)
    setLoading(false)
    setFetchedAt(null)
    return () => { ++requestId.current }
  }, [queryKey])

  useEffect(() => {
    if (!enabled || !isTauri || !connectionId) { setLoading(false); return }
    void run(false)
    const timer = window.setInterval(() => void run(true), interval)
    return () => { ++requestId.current; window.clearInterval(timer) }
  }, [run, connectionId, interval, enabled])

  const refresh = useCallback(() => run(true), [run])
  if (owner.current !== queryKey) return { state: null, loading: true, fetchedAt: null, refresh }
  return { state, loading, fetchedAt, refresh }
}

/**
 * 区间估算费用（§2.4）。
 * 本地记录不含账单金额，因此这里拿到的**永远是估算值**，界面必须标注「估算」。
 */
export function useCostEstimate(
  platform: string,
  period: string,
  custom?: CustomRange | null,
  queryEndMs?: number,
  model?: string | null,
) {
  const [data, setData] = useState<CostEstimateDto | null>(null)
  const owner = useRef<string | null>(null)

  useEffect(() => {
    const query = JSON.stringify([platform, period, custom?.start, custom?.end, model])
    if (owner.current !== query) setData(null)
    owner.current = query
    if (!isTauri || (period === "custom" && !custom)) return
    let cancelled = false
    let serial = 0
    const offs: Array<() => void> = []
    const refresh = () => {
      const request = ++serial
      void api.costEstimate(platform, period, custom ?? null, model ?? null, queryEndMs ?? Date.now())
        .then(r => { if (!cancelled && serial === request) setData(r) })
        .catch(() => { if (!cancelled && serial === request) setData(null) })
    }
    const listen = (promise: Promise<() => void>) => void promise.then(off => { if (cancelled) off(); else offs.push(off) }).catch(() => {})
    refresh()
    if (queryEndMs == null) {
      listen(listenEvent<LiveUsageDto>("live-usage", usage => { if (usage.platform === platform) refresh() }))
      // 带连接 ID 的行内更新不影响费用汇总；只有全局刷新才重查。
      listen(listenEvent<string | null>("refresh-requested", connectionId => { if (connectionId == null) refresh() }))
    }
    return () => {
      cancelled = true
      offs.forEach(off => off())
    }
  }, [platform, period, custom, queryEndMs, model])

  return data
}
