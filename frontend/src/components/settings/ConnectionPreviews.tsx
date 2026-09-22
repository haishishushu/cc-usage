import { useEffect, useState } from "react"
import { listenEvent } from "@/lib/api"
import { isFetchOnlyPlatform, platformSupports, platformConfig } from "@/lib/platforms"
import { useSourceMetrics } from "@/lib/useSourceMetrics"
import { Plug, Unplug } from "lucide-react"
import { PlatformLogo } from "@/components/brand/PlatformLogo"
import { useQuota, useBalance, useApiUsage, useCostEstimate } from "@/lib/useQuota"
import { useLiveUsage } from "@/lib/useLiveUsage"
import { toQuotaWindows } from "@/lib/quotaMap"
import { cn } from "@/lib/utils"
import { balanceTextClass } from "@/lib/quota"
import type { Connection } from "@/types"

export function ConnectionPreviews({ connections, selectedId }: { connections: Connection[]; selectedId: string | null }) {
  if (!connections.length) return <div className="flex items-center gap-3 rounded-xl border bg-surface-2 p-5 text-xs text-text-secondary">
    <Plug aria-hidden="true" className="size-5" />读取本机配置后显示指标预览。
  </div>
  return <div className="grid grid-cols-1 gap-4 sm:grid-cols-2" aria-label="连接指标预览">
    {connections.map(connection => <ConnectionPreview key={`${connection.id}:${connection.status}`} connection={connection} selected={connection.id === selectedId} />)}
  </div>
}

function ConnectionPreview({ connection, selected }: { connection: Connection; selected: boolean }) {
  return <article data-connection-preview={connection.id} className={cn("flex min-w-0 flex-col gap-3 rounded-xl border bg-surface p-3.5", selected && "border-accent-blue")}>
    <header className="flex flex-wrap items-center gap-2">
      <PlatformLogo platform={connection.platformId} size={16} />
      <span title={connection.name} className="min-w-0 flex-1 truncate text-xs font-semibold text-text-primary">{connection.name}</span>
      <span className={cn("rounded-md px-1.5 py-0.5 text-[10px]", connection.kind === "auth" ? "bg-success-soft text-success-text" : "bg-accent-blue-soft text-accent-blue")}>{connection.kind === "auth" ? isFetchOnlyPlatform(connection.platformId) ? "本机" : "Auth" : "API"}</span>
      {selected && <span className="rounded-md bg-neutral-soft px-1.5 py-0.5 text-[10px] text-text-tertiary">{connection.status === "connected" ? "使用中" : "当前选择"}</span>}
    </header>
    {connection.status === "paused" ? <div className="flex items-start gap-2 rounded-lg bg-surface-2 p-3 text-xs text-text-secondary">
      <Unplug aria-hidden="true" className="size-4 shrink-0 text-warn" />
      <p>已断开，暂停远程查询。配置、凭证与历史保留，可在连接管理中重新连接。</p>
    </div> : isFetchOnlyPlatform(connection.platformId) && connection.kind === "api" ? <p className="text-xs text-text-secondary">官方 Key 只读检测连接；未接入此 Key 的独立用量、额度或余额。本机记录请在总览查看。</p> : isFetchOnlyPlatform(connection.platformId) && connection.platformId !== "grok" ? <NativeConnectionMetrics connection={connection} /> : <ConnectionMetrics connection={connection} />}
  </article>
}

function ConnectionMetrics({ connection }: { connection: Connection }) {
  const gateway = !!connection.baseUrl
  // api 连接同样查额度：编程套餐（智谱/Kimi 等）按 base_url 识别后能返回额度窗口
  const quota = useQuota(connection.id)
  const balance = useBalance(connection.kind === "api" && gateway ? connection.id : null)
  const usage = useApiUsage(connection.kind === "api" && !gateway ? connection.id : null)
  const live = useLiveUsage(connection.platformId)
  const cost = useCostEstimate(connection.platformId, "today")
  const quotaOk = quota.state?.state === "ok" && quota.state.windows.length > 0
  // 主结果：auth 看额度；api+网关优先看额度窗口（编程套餐），无窗口再看余额；直连看组织用量
  const result = connection.kind === "auth"
    ? quota
    : quotaOk ? quota : gateway ? balance : usage
  const failure = connection.kind === "auth"
    ? quota.failure ?? (quota.state?.state !== "ok" ? quota.state : null)
    : quotaOk ? null
      : gateway ? balance.failure ?? (balance.state?.state !== "ok" ? balance.state : null)
        : usage.state?.state !== "ok" ? usage.state : null
  return <>
    {quota.state?.state === "ok" && quota.state.windows.length > 0 && <div className="flex flex-col gap-2">
      {toQuotaWindows(quota.state.windows, Date.now()).map(window => <div key={window.key} className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-surface-2 px-3 py-2 text-xs">
        <span className="text-text-secondary">{window.windowName}</span>
        <span className="font-mono text-text-primary">{window.usedPercent === null ? "—" : `${Math.round(window.usedPercent)}%`}{window.resetCountdown ? ` · ${window.resetCountdown}` : ""}</span>
      </div>)}
    </div>}
    {quota.state?.state === "ok" && !quota.state.windows.length && <p className="text-xs text-text-secondary">来源未提供额度窗口</p>}
    {gateway && balance.state?.state === "ok" && <p className="text-xs text-text-secondary">网关余额：<span className={cn("font-mono", balanceTextClass(balance.state.balance <= 0 ? "empty" : "healthy"))}>{balance.state.balance.toFixed(2)}</span> <span className="font-mono">{balance.state.currency}</span></p>}
    {usage.state?.state === "ok" && <p className="text-xs text-text-secondary">官方组织用量：<span className="font-mono text-text-primary">{usage.state.total_tokens.toLocaleString("en-US")} Token</span>{usage.state.cost_usd === null ? " · 费用不可用" : ` · $${usage.state.cost_usd.toFixed(2)}`}</p>}
    {result.loading && <p role="status" className="text-xs text-text-secondary">正在更新连接数据…</p>}
    {failure && "reason" in failure && <p role="status" className="break-words rounded-lg bg-warn-soft px-3 py-2 text-xs text-warn">{failure.reason}</p>}
    {!result.loading && !result.state && <p className="text-xs text-text-secondary">尚无连接查询数据</p>}
    <div className="grid grid-cols-2 gap-2 border-t pt-3">
      <div className="rounded-lg bg-surface-2 p-3"><p className="text-[11px] text-text-secondary">本机今日 Token</p><p className="mt-1 font-mono text-sm text-text-primary">{live.todayTokenText ?? "—"}</p></div>
      <div className="rounded-lg bg-surface-2 p-3"><p className="text-[11px] text-text-secondary">本机估算费用</p><p className="mt-1 font-mono text-sm text-text-primary">{cost?.complete ? `≈ $${cost.amount.toFixed(4)}` : "—"}</p></div>
    </div>
    <p className="text-[11px] leading-relaxed text-text-secondary">本机统计按平台汇总，同平台连接共享此数据，不能视为各账号独立用量。</p>
    <p className="text-[11px] text-text-tertiary">{result.fetchedAt ? `连接数据更新于 ${result.fetchedAt.toLocaleTimeString("zh-CN", { hour12: false })}` : "连接数据尚无成功更新时间"}</p>
  </>
}

function NativeConnectionMetrics({ connection }: { connection: Connection }) {
  const live = useLiveUsage(connection.platformId)
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    let stopped = false
    let off: (() => void) | undefined
    void listenEvent<{ platform: string }>("live-usage", value => { if (value.platform === connection.platformId) setNow(Date.now()) })
      .then(unlisten => { if (stopped) unlisten(); else off = unlisten })
    return () => { stopped = true; off?.() }
  }, [connection.platformId])
  const metrics = useSourceMetrics(connection.platformId, "today", null, null, now)
  return <>
    <p className="text-xs text-text-secondary">本机今日 Token：{live.todayTokenText ?? "—"}</p>
    {platformSupports(connection.platformId, "credits") && <p className="text-xs text-text-secondary">今日上报积分：{metrics.data?.credits?.toLocaleString("zh-CN", { maximumFractionDigits: 4 }) ?? "—"}（非余额）</p>}
    {metrics.error && <p role="alert" className="text-xs text-danger">{metrics.error}</p>}
    <p className="text-[11px] text-text-tertiary">{platformConfig(connection.platformId).limitation}。按平台合并本机记录，非账号独立账单。</p>
  </>
}
