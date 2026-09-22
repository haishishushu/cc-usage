import { useContentMotion, useExitPresence } from "@/lib/motion"
import { SelectField } from "@/components/ui/SelectField"
import { useEffect, useMemo, useRef, useState } from "react"
import { cn } from "@/lib/utils"
import { Button, ConnectionKindChip, Dropdown, Hint, SectionTitle, Segmented } from "@/components/ui/primitives"
import { UsageCard } from "@/components/panel/UsageCard"
import { UsageAreaChart } from "@/components/panel/UsageAreaChart"
import { UsageHero } from "@/components/panel/UsageHero"
import { RequestLogTable } from "@/components/panel/RequestLogTable"
import { Pagination } from "@/components/panel/Pagination"
import { BalanceCard } from "@/components/panel/BalanceCard"
import { useToast } from "@/components/ui/Toast"
import { DateRangePicker } from "@/components/panel/DateRangePicker"
import { grouped, isTauri, listenEvent, type BalanceStateDto, type CustomRange, type LiveUsageDto, type QuotaStateDto } from "@/lib/api"
import { useConnections } from "@/lib/useConnections"
import { shouldRefreshConnection } from "@/lib/refreshScope"
import { useApiUsage, useBalance, useCostEstimate, useQuota } from "@/lib/useQuota"
import { toQuotaWindows } from "@/lib/quotaMap"
import { isPlanCovered, planCoverageHint } from "@/lib/planCoverage.ts"
import type { QueryState } from "@/components/panel/QueryStateNotice"
import { BALANCE, CLAUDE_QUOTAS } from "@/mock/data"
import { useModels, useRequestLog, useTokenSummaries, useTrend, useUsageBreakdown } from "@/lib/useUsage"
import { shortModelName } from "@/lib/modelName"
import type { PlatformId, StatPeriod } from "@/types"
import { platformConfig, isFetchOnlyPlatform, platformSupports } from "@/lib/platforms"
import { NativeSourcePanel } from "@/components/panel/NativeSourcePanel"
import { useCollectionStatus } from "@/lib/useCollectionStatus"

/** C/TokenCard：w200 padding16 gap6 r14；标签 fs12 → 数值 fs22/600 → 来源提示 fs11 */
function TokenCard({ label, value, hint }: { label: string; value: string; hint: string }) {
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1.5 rounded-card border bg-surface p-4">
      <span className="text-xs text-text-secondary">{label}</span>
      <span className="tnum font-mono text-[22px] font-semibold text-text-primary">{value}</span>
      <span className="text-[11px] text-text-tertiary">{hint}</span>
    </div>
  )
}

/**
 * 把查询状态翻成 UsageCard / BalanceCard 的提示 props。
 * 返回空对象表示「有真实数据，正常渲染」；
 * 桌面端没有连接时也返回不可用态，避免把设计示例当成真实额度（§6.3 空状态）。
 */
function notice(
  s: { state: string } | null,
  failure: { state: string; reason?: string } | null,
  loading: boolean,
  stale: boolean,
  fetchedAt: Date | null,
  hasConnection: boolean,
) {
  if (!isTauri) return {}
  if (!hasConnection) {
    return {
      queryState: "unsupported" as QueryState,
      reason: "尚未读取连接。请在 CC Switch 配置后，到「设置 → 连接管理 → 添加连接」选择平台和类型后读取",
    }
  }
  if (loading && !s) return { queryState: "loading" as QueryState }
  if (stale && failure) {
    return {
      queryState: failure.state as QueryState,
      reason: failure.reason,
      stale: true,
      fetchedAt,
    }
  }
  if (!s || s.state === "ok") return {}
  return {
    queryState: s.state as QueryState,
    reason: (s as { reason?: string }).reason,
    stale,
    fetchedAt,
  }
}

function quotaNotice(
  q: { state: QuotaStateDto | null; failure: Exclude<QuotaStateDto, { state: "ok" }> | null; loading: boolean; stale: boolean; fetchedAt: Date | null },
  hasConnection: boolean,
) {
  return notice(q.state, q.failure, q.loading, q.stale, q.fetchedAt, hasConnection)
}

function balanceNotice(
  b: { state: BalanceStateDto | null; failure: Exclude<BalanceStateDto, { state: "ok" }> | null; loading: boolean; stale: boolean; fetchedAt: Date | null },
  hasConnection: boolean,
) {
  return notice(b.state, b.failure, b.loading, b.stale, b.fetchedAt, hasConnection)
}

/** 额度卡标题：平台名已经足够表达身份，不重复显示通用的 pro 档位。 */
function quotaTitle(platformName: string, plan: string | null | undefined): string {
  const normalized = plan?.trim()
  return !normalized || /^pro(?:\s*\([^)]*\))?$/i.test(normalized)
    ? platformName
    : `${platformName} ${normalized}`
}

/** 自定义范围的可读摘要，供分桶说明使用 */
function rangeText(r: CustomRange): string {
  const f = (ms: number) => {
    const d = new Date(ms)
    const p = (n: number) => String(n).padStart(2, "0")
    return `${d.getFullYear()}/${p(d.getMonth() + 1)}/${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
  }
  return r.end === null ? `${f(r.start)} 起至当前时刻` : `${f(r.start)} — ${f(r.end)}`
}

export function OverviewView({
  platform,
  variant = "auth",
  preferredConnectionId = null,
  onAddConnection,
  active: panelActive = true,
}: {
  platform: PlatformId
  /** auth：官方账号双额度；api：顶部余额区域 */
  variant?: "auth" | "api"
  preferredConnectionId?: string | null
  onAddConnection?: () => void
  active?: boolean
}) {
  const [period, setPeriod] = useState<StatPeriod>("today")
  const [page, setPage] = useState(1)
  // null 表示不按模型筛选；空字符串会被当成「模型名为空」，两者不能混用
  const [model, setModel] = useState<string | null>(null)
  // 0 表示不轮询，只靠文件监听的 live-usage 事件驱动（默认，见 §采集与刷新）
  const [pollMs, setPollMs] = useState(0)
  // 已生效的自定义范围；打开面板不立即改变筛选，确定后才写入（§2.3）
  const [custom, setCustom] = useState<CustomRange | null>(null)
  const [pickerOpen, setPickerOpen] = useState(false)
  const [selectedConnectionId, setSelectedConnectionId] = useState<string | null>(null)
  const [refreshKey, setRefreshKey] = useState(0)
  const [quotaNow, setQuotaNow] = useState(() => Date.now())
  const collection = useCollectionStatus()
  const toast = useToast()
  const pickerPresence = useExitPresence(pickerOpen ? true : null, 120)
  /** 自定义时间面板是浮层，点面板与周期条以外的地方、或按 Esc 都收起 */
  const pickerAnchorRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!pickerOpen) return
    const onPointerDown = (event: PointerEvent) => {
      if (!pickerAnchorRef.current?.contains(event.target as Node)) setPickerOpen(false)
    }
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setPickerOpen(false) }
    document.addEventListener("pointerdown", onPointerDown)
    document.addEventListener("keydown", onKeyDown)
    return () => {
      document.removeEventListener("pointerdown", onPointerDown)
      document.removeEventListener("keydown", onKeyDown)
    }
  }, [pickerOpen])
  const connectionMotion = useContentMotion(`${platform}:${selectedConnectionId}`, 160, 0)
  const wasActive = useRef(panelActive)
  useEffect(() => {
    if (panelActive && !wasActive.current) setRefreshKey((value) => value + 1)
    wasActive.current = panelActive
  }, [panelActive])

  useEffect(() => {
    if (!panelActive) return
    const timer = window.setInterval(() => setQuotaNow(Date.now()), 30_000)
    return () => window.clearInterval(timer)
  }, [panelActive])

  // 当前平台下的连接：额度与余额都按连接查，不按平台
  const { connections } = useConnections()
  const platformConnections = connections.filter((connection) => connection.platformId === platform)
  const active = platformConnections.find((connection) => connection.id === selectedConnectionId) ?? null
  const effectiveVariant = active?.kind ?? variant
  const quota = useQuota(active?.status === "paused" ? null : active?.id ?? null, false, panelActive)
  const balance = useBalance(active?.status === "paused" ? null : active?.id ?? null, panelActive)
  // Grok 不走连接：额度直接来自本机 grok CLI 凭证（~/.grok/auth.json）
  const grokQuota = useQuota(null, "grok", panelActive && platform === "grok" && effectiveVariant === "auth")
  /**
   * 套餐判定按实际查到的额度窗口走：有 5 小时 / 周额度的就是套餐，其余按 API Key 计费。
   * Grok 走独立的积分额度接口，key 是 credits，不落入这两个窗口，因此照常显示估算金额。
   */
  const planWindows = platform === "grok"
    ? (grokQuota.state?.state === "ok" ? grokQuota.state.windows : null)
    : (quota.state?.state === "ok" ? quota.state.windows : null)
  const planCovered = isPlanCovered(planWindows)
  const planHint = planCoverageHint(planWindows)
  const officialApiUsage = useApiUsage(active?.status !== "paused" && active?.kind === "api" && !active.baseUrl ? active.id : null, panelActive)
  const queryEndMs = useMemo(
    () => Date.now(),
    [platform, period, custom?.start, custom?.end, refreshKey, model],
  )
  const fetchedCost = useCostEstimate(platform, period, custom, queryEndMs, model)
  const cost = platformSupports(platform, "cost_estimate") ? fetchedCost : null
  const platformName = platformConfig(platform).name

  // 真实数据；浏览器预览下回落到设计示例
  const { summaries, status: summaryStatus, error: summaryError } = useTokenSummaries(platform, custom, refreshKey, queryEndMs, model)
  const { breakdown, status: breakdownStatus, error: breakdownError } = useUsageBreakdown(platform, period, custom, refreshKey, queryEndMs, model)
  const { chart, status: trendStatus, error: trendError } = useTrend(platform, period, custom, refreshKey, queryEndMs, model)
  const { rows: logRows, totalCount, pageCount, status: logStatus, error: logError } = useRequestLog(platform, period, page, custom, refreshKey, queryEndMs, model)
  const models = useModels(platform, period, custom, refreshKey, queryEndMs)
  const trendMotion = useContentMotion(`${platform}:${period}:${custom?.start}:${custom?.end}:${model}`, 120, 0)
  const logMotion = useContentMotion(`${platform}:${period}:${page}:${custom?.start}:${custom?.end}:${model}`, 120, 0)

  // 切换平台、周期或模型后回到第一页（§2.4）；自定义范围变化同理
  useEffect(() => setPage(1), [platform, period, custom, model])

  // 换平台后旧平台的模型名不再适用，留着会筛出空结果
  useEffect(() => setModel(null), [platform])

  // 选中的模型在新范围里不存在时自动取消筛选，避免页面停在一片空白上
  useEffect(() => {
    if (model !== null && models.length > 0 && !models.includes(model)) setModel(null)
  }, [models, model])

  // 固定间隔刷新是兜底档：默认 0（不轮询），实时更新仍由 live-usage 文件事件驱动
  useEffect(() => {
    if (!panelActive || pollMs <= 0) return
    const timer = window.setInterval(() => setRefreshKey((value) => value + 1), pollMs)
    return () => window.clearInterval(timer)
  }, [panelActive, pollMs])

  useEffect(() => {
    if (!panelActive) return
    let stopped = false
    const offs: Array<() => void> = []
    void listenEvent<string | null>("refresh-requested", (connectionId) => {
      if (shouldRefreshConnection(connectionId, selectedConnectionId)) setRefreshKey((value) => value + 1)
    })
      .then((off) => stopped ? off() : offs.push(off))
    void listenEvent<LiveUsageDto>("live-usage", (usage) => {
      if (usage.platform === platform) setRefreshKey((value) => value + 1)
    }).then((off) => stopped ? off() : offs.push(off))
    return () => { stopped = true; offs.forEach((off) => off()) }
  }, [platform, panelActive, selectedConnectionId])

  useEffect(() => {
    const key = `overview-connection:${platform}`
    const saved = window.localStorage.getItem(key)
    const next = platformConnections.some((connection) => connection.id === preferredConnectionId)
      ? preferredConnectionId
      : platformConnections.some((connection) => connection.id === saved)
      ? saved
      : saved === null && platformConnections.length === 1
        ? platformConnections[0].id
        : null
    setSelectedConnectionId(next)
  }, [platform, connections.length, preferredConnectionId])

  const chooseConnection = (id: string) => {
    const next = id || null
    setSelectedConnectionId(next)
    if (next) window.localStorage.setItem(`overview-connection:${platform}`, next)
    else window.localStorage.removeItem(`overview-connection:${platform}`)
  }

  const realBalance = balance.state?.state === "ok"
    ? {
        queryState: "loaded" as const,
        amountText: balance.state.balance.toFixed(2),
        currency: balance.state.currency,
        scopeLabel: "范围未由接口说明",
        sourceText: balance.fetchedAt
          ? `网关余额接口 · ${balance.fetchedAt.toLocaleTimeString("zh-CN", { hour12: false })}`
          : "网关余额接口",
        level: balance.state.balance <= 0 ? "empty" as const : "healthy" as const,
      }
    : BALANCE

  return (
    <div className="flex w-full flex-col gap-7 p-8">
      {collection.status && !collection.status.ok && (
        <div className="flex items-center justify-between gap-4 rounded-card bg-danger-soft px-4 py-3">
          <div className="min-w-0 text-[11px] text-danger">
            <p className="font-semibold">本地会话采集异常</p>
            <p className="truncate" title={collection.status.errors.join("；")}>{collection.status.errors.join("；")}</p>
            <p className="text-text-tertiary">上次成功：{collection.status.last_success_ms
              ? new Date(collection.status.last_success_ms).toLocaleString("zh-CN", { hour12: false })
              : "尚无成功记录"}</p>
          </div>
          <Button onClick={() => {
            void collection.retry()
              .then(() => toast.info("已重新采集本地会话", "采集状态随下一轮刷新更新"))
              .catch((reason) => toast.danger("重新采集失败", String(reason)))
          }}>重试采集</Button>
        </div>
      )}
      {/* 当前平台 / 当前连接 —— 套餐名原样透传，不由额度百分比推断 */}
      <div className="flex w-full items-center gap-6">
        <span className="flex items-center gap-1.5">
          <span className="text-xs text-text-secondary">当前平台：</span>
          <span className="text-[13px] font-semibold text-text-primary">{platformName}</span>
        </span>
        <span className="flex items-center gap-1.5">
          <span className="text-xs text-text-secondary">当前连接：</span>
          <span className="text-[13px] font-semibold text-text-primary">
            {active?.name ?? "未选择连接"}
          </span>
          <ConnectionKindChip kind={effectiveVariant} local={isFetchOnlyPlatform(platform) && effectiveVariant === "auth" && platform !== "grok"} />
        </span>
        <span className="flex-1" />
        <SelectField
          label="当前连接"
          className="w-[240px] max-w-[280px]"
          value={active?.id ?? ""}
          onValueChange={chooseConnection}
          options={[
            { value: "", label: platformConnections.length ? "请选择具体连接" : "该平台暂无连接" },
            ...platformConnections.map((connection) => ({ value: connection.id, label: `${connection.name} · ${connection.kind === "auth" ? "Auth" : "API Key"}` })),
          ]}
        />
        {!active && (
          <Button variant="primary" onClick={onAddConnection}>
            添加连接
          </Button>
        )}
      </div>

      <div ref={connectionMotion}>
      {platform === "grok" && effectiveVariant === "auth" ? (
        <UsageCard
          title="Grok · SuperGrok"
          kind="auth"
          statusLabel="本机凭证"
          statusTone="success"
          updatedText="额度来源：grok.com 计费接口 · 凭证来自本机 grok CLI（~/.grok/auth.json）"
          quotas={grokQuota.state?.state === "ok" ? toQuotaWindows(grokQuota.state.windows, quotaNow) : []}
          {...quotaNotice(grokQuota, true)}
        />
      ) : isFetchOnlyPlatform(platform) ? (
        <NativeSourcePanel platform={platform} period={period} custom={custom} model={model} queryEndMs={queryEndMs} />
      ) : effectiveVariant === "auth" ? (
        <UsageCard
          title={
            quota.state?.state === "ok" && quota.state.plan
              ? quotaTitle(platformName, quota.state.plan)
              : `${platformName} 官方订阅`
          }
          kind={active?.kind ?? "auth"}
          statusLabel={!active ? "尚未添加连接" : { connected: "已连接", expired: "凭证已过期", invalid: "凭证无效", offline: "离线", paused: "已断开" }[active.status]}
          statusTone={!active || active.status === "paused" || active.status === "offline" || active.status === "expired" ? "warn" : active.status === "invalid" ? "danger" : "success"}
          updatedText={
            quota.fetchedAt
              ? `额度来源：账号额度接口 · 更新于 ${quota.fetchedAt.toLocaleTimeString("zh-CN", { hour12: false })}`
              : "额度来源：账号额度接口"
          }
          quotas={
            quota.state?.state === "ok" ? toQuotaWindows(quota.state.windows, quotaNow) : CLAUDE_QUOTAS
          }
          {...(active?.status === "paused" ? { queryState: "unsupported" as QueryState, reason: "连接已断开，远程查询已暂停；可在设置中重新连接。本机历史统计继续保留。" } : quotaNotice(quota, !!active))}
        />
      ) : active?.baseUrl ? (
        <div className="flex w-full flex-col gap-4">
          {/* 编程套餐（智谱/Kimi 等）的 api 连接有额度窗口：额度卡为主，余额不支持时不再显示失败卡 */}
          {quota.state?.state === "ok" && quota.state.windows.length > 0 && (
            <UsageCard
              title={
                quota.state.plan
                  ? quotaTitle(platformName, quota.state.plan)
                  : `${platformName} 套餐额度`
              }
              kind={active.kind}
              statusLabel={{ connected: "已连接", expired: "凭证已过期", invalid: "凭证无效", offline: "离线", paused: "已断开" }[active.status]}
              statusTone={active.status === "paused" || active.status === "offline" || active.status === "expired" ? "warn" : active.status === "invalid" ? "danger" : "success"}
              updatedText={
                quota.fetchedAt
                  ? `额度来源：套餐额度接口 · 更新于 ${quota.fetchedAt.toLocaleTimeString("zh-CN", { hour12: false })}`
                  : "额度来源：套餐额度接口"
              }
              quotas={toQuotaWindows(quota.state.windows, quotaNow)}
              {...(active.status === "paused"
                ? { queryState: "unsupported" as QueryState, reason: "连接已断开，远程查询已暂停；可在设置中重新连接。" }
                : quotaNotice(quota, true))}
            />
          )}
          {balance.state?.state !== "unsupported" && (
            <BalanceCard amount={balance.state?.state === "ok" ? balance.state.balance : null} balance={realBalance} {...(active.status === "paused" ? { queryState: "unsupported" as QueryState, reason: "连接已断开，余额查询已暂停；可在设置中重新连接。" } : balanceNotice(balance, !!active))} />
          )}
        </div>
      ) : (
        <section className="flex w-full flex-col gap-3 rounded-card border bg-surface p-5">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-text-primary">今日 API 用量</h3>
            <span className="text-[11px] text-text-tertiary">
              {officialApiUsage.state?.state === "ok" ? officialApiUsage.state.source : "本机同平台会话记录"}
            </span>
          </div>
          <div className="flex gap-4">
            <TokenCard
              label="今日 Token"
              value={officialApiUsage.state?.state === "ok"
                ? grouped(officialApiUsage.state.total_tokens)
                : summaries.find((item) => item.period === "today")?.value ?? "—"}
              hint={officialApiUsage.state?.state === "ok" ? "官方组织聚合" : "本机记录，无法按 Key 区分"}
            />
            <TokenCard
              label="今日费用"
              value={officialApiUsage.state?.state === "ok" && officialApiUsage.state.cost_usd !== null
                ? `$${officialApiUsage.state.cost_usd.toFixed(2)}`
                : cost?.complete ? `≈ $${cost.amount.toFixed(2)}` : "—"}
              hint={officialApiUsage.state?.state === "ok" && officialApiUsage.state.cost_usd !== null
                ? "官方组织费用"
                : "按本地 Token 估算"}
            />
          </div>
          {officialApiUsage.loading && <Hint>正在检查该 Key 是否具有组织用量权限…</Hint>}
          {officialApiUsage.state?.state === "ok" && officialApiUsage.state.cost_reason && (
            <Hint>{officialApiUsage.state.cost_reason}；费用已回退为本机 Token 估算。</Hint>
          )}
          {officialApiUsage.state && officialApiUsage.state.state !== "ok" && (
            <Hint>{officialApiUsage.state.reason}；已回退到本机同平台记录。</Hint>
          )}
        </section>
      )}
      </div>

      {/* Token 统计：周期与模型筛选在这里统一控制汇总、趋势与日志三块内容 */}
      <section className="flex w-full flex-col gap-3.5">
        <header className="flex w-full flex-wrap items-center justify-between gap-3">
          <SectionTitle>
            {platformName} 本地 Token 统计
          </SectionTitle>
          <div className="flex flex-wrap items-center gap-2.5">
            <span className="text-xs text-text-secondary">统计来源</span>
            <Dropdown label="本地会话记录" />
            <SelectField
              label="模型筛选"
              className="w-[190px]"
              value={model ?? ""}
              onValueChange={(value) => setModel(value || null)}
              options={[
                { value: "", label: models.length ? "全部模型" : "暂无模型记录" },
                ...models.map((name) => ({ value: name, label: shortModelName(name) })),
              ]}
            />
            <SelectField
              label="刷新方式"
              className="w-[150px]"
              value={String(pollMs)}
              onValueChange={(value) => setPollMs(Number(value))}
              options={[
                { value: "0", label: "事件驱动" },
                { value: "5000", label: "每 5 秒" },
                { value: "10000", label: "每 10 秒" },
                { value: "30000", label: "每 30 秒" },
                { value: "60000", label: "每 60 秒" },
              ]}
            />
          </div>
        </header>

        {/* 周期条是浮层的锚点：面板绝对定位挂在它下方，不占文档流、不推挤下方内容 */}
        <div ref={pickerAnchorRef} className="relative w-full">
          <Segmented
            items={[
              { value: "today", label: "今日" },
              { value: "week", label: "本周" },
              { value: "month", label: "本月" },
              { value: "total", label: "累计" },
              { value: "custom", label: "自定义时间" },
            ]}
            value={period}
            onChange={(v) => {
              // 点「自定义时间」只弹面板，已生效的筛选不动（§2.3）
              if (v === "custom") setPickerOpen(true)
              else {
                setPeriod(v)
                setCustom(null)
                setPickerOpen(false)
              }
            }}
          />

          {pickerPresence.rendered && (
            // 外层只管定位，动画留给内层：motion-popover 用 transform 做位移，两者放一起会互相覆盖
            <div className="pointer-events-none absolute inset-x-0 top-full z-30 flex justify-center pt-2">
              <div inert={pickerPresence.exiting} data-state={pickerPresence.exiting ? "closed" : "open"} className="motion-popover pointer-events-auto">
                <DateRangePicker
                  initial={custom}
                  onCancel={() => setPickerOpen(false)}
                  onApply={(r) => {
                    setCustom(r)
                    setPeriod("custom")
                    setPickerOpen(false)
                  }}
                  onQuick={(q) => {
                    setPeriod(q)
                    setCustom(null)
                    setPickerOpen(false)
                  }}
                />
              </div>
            </div>
          )}
        </div>

        <UsageHero
          platform={platform}
          breakdown={breakdown}
          status={breakdownStatus}
          error={breakdownError}
          costText={cost?.complete ? `≈ $${cost.amount.toFixed(2)}` : null}
          onRetry={() => setRefreshKey((value) => value + 1)}
        />

        {/* 四个自然周期压成一行摘要：主视觉让给上面跟随当前范围的分项卡 */}
        <div className="flex w-full flex-wrap items-center gap-x-4 gap-y-1 rounded-card border bg-surface px-4 py-3 text-[11px] text-text-secondary">
          {summaryStatus === "loading" && <span className="text-text-tertiary">周期汇总读取中…</span>}
          {summaryStatus === "failed" && (
            <span className="flex flex-1 items-center justify-between gap-3 text-danger">
              <span>Token 汇总查询失败：{summaryError ?? "未知错误"}</span>
              <Button onClick={() => setRefreshKey((value) => value + 1)}>重试</Button>
            </span>
          )}
          {summaryStatus === "empty" && <span className="text-text-tertiary">当前筛选下暂无 Token 记录</span>}
          {summaryStatus === "ready" && summaries.map((t) => (
            <span key={t.period} className="flex items-baseline gap-1.5">
              <span>{t.label}</span>
              <span className="tnum font-mono font-semibold text-text-primary">{t.value}</span>
              {t.period === "total" && <span className="text-text-tertiary">（{t.hint}）</span>}
            </span>
          ))}
        </div>

        {cost && (
          <p className="text-[11px] leading-[1.6] text-warn">
            {cost.complete
              ? `该区间按 API 价目折算 $${cost.amount.toFixed(2)}（估算）。`
              : "该区间有记录缺少计价所需 Token 字段，费用保持未知。"}
            {cost.complete && (effectiveVariant === "auth"
              ? "官方订阅按月费计价，这个数字是「同等用量走 API 要花多少」，不是你的实际支出。"
              : "按公开价目表乘 Token 数推算，不含套餐折扣与批处理折扣，与实际账单会有差异。")}
            {cost.complete && cost.uncovered_tokens > 0 &&
              ` 另有 ${grouped(cost.uncovered_tokens)} Token 的模型不在价目表内，未计入。`}
          </p>
        )}
        <Hint>
          当前统计来自本平台本机会话记录，无法按具体账号或 API Key 区分；「真实消耗」含缓存重读（每次请求都会重复计入上下文），
          因此大于灵动岛实时数——灵动岛与 Claude Code 终端一致，只计新鲜 Token（输入+输出）。
          「新增输入」已扣除缓存重读，是与灵动岛口径一致的那部分。
          「缓存创建」与「缓存命中」按会话记录中的对应字段统计：记录为 0 时显示 0，字段缺失时显示「—」。
          仅凭记录中的 0 无法判断服务端是否实际创建了缓存，也无法推断缓存创建是否计入其他字段。
        </Hint>
      </section>

      {/* 趋势图 */}
      <section className="flex w-full flex-col gap-3.5">
        <header className="flex w-full items-center justify-between">
          <SectionTitle>{platformName} Token 趋势图</SectionTitle>
          <span className="text-[11px] text-text-tertiary">复用上方统计周期与模型筛选</span>
        </header>

        <div ref={trendMotion}>
        {trendStatus === "loading" ? (
          <div className="grid h-[300px] place-items-center rounded-card border bg-surface text-xs text-text-tertiary">趋势读取中…</div>
        ) : trendStatus === "failed" ? (
          <div className="flex h-[300px] flex-col items-center justify-center gap-3 rounded-card border bg-surface px-6 text-xs text-danger">
            <span>趋势查询失败：{trendError ?? "未知错误"}</span>
            <Button onClick={() => setRefreshKey((value) => value + 1)}>重试</Button>
          </div>
        ) : trendStatus === "empty" ? (
          <div className="grid h-[300px] place-items-center rounded-card border bg-surface text-xs text-text-tertiary">当前范围暂无趋势数据</div>
        ) : (
          <UsageAreaChart labels={chart.labels} series={chart.series} cost={chart.cost} />
        )}
        </div>
        <Hint>
          上格纵轴为 Token 用量，下格为按价目表估算的费用，两格共用同一根时间轴。
          成本单独成格而不是叠在同一绘图区上：两种量纲共用一个绘图区时，线条的交叉点由各自的缩放比例决定，不代表任何真实关系。
          当前分桶粒度：{chart.bucket}
          {period === "custom" && custom ? ` · 范围 ${rangeText(custom)}` : ""}；
          切换周期后趋势图与请求日志同步更新，旧请求返回不得覆盖新选择的数据。
        </Hint>
      </section>

      {/* 请求日志 */}
      <section className="flex w-full flex-col gap-3.5">
        <header className="flex w-full items-center justify-between">
          <SectionTitle>{platformName} 请求日志</SectionTitle>
          <span className="text-[11px] text-text-tertiary">
            复用上方统计周期、自定义时间范围与模型筛选 · 来源：
            本地会话记录
            {!isTauri && " · 设计示例"}
          </span>
        </header>
        <div ref={logMotion}>
        <RequestLogTable
          rows={logRows}
          state={logStatus}
          onRetry={() => setRefreshKey((value) => value + 1)}
          planCovered={planCovered}
          planHint={planHint}
        />
        </div>
        {logStatus === "failed" && logError && <Hint>失败原因：{logError}</Hint>}
        <Pagination
          current={page}
          pageCount={pageCount}
          totalCount={totalCount}
          onChange={setPage}
        />
      </section>
    </div>
  )
}

export { cn }
