import { useEffect, useState } from "react"
import { api, isTauri, type CustomRange, type SourceInfoDto } from "@/lib/api"
import { platformConfig, platformSupports } from "@/lib/platforms"
import { useSourceMetrics } from "@/lib/useSourceMetrics"
import type { PlatformId } from "@/types"

export function NativeSourcePanel({ platform, period, custom, model, queryEndMs }: {
  platform: PlatformId; period: string; custom: CustomRange | null; model: string | null; queryEndMs: number
}) {
  const [result, setResult] = useState<{ platform: string; sources: SourceInfoDto[]; error?: string } | null>(null)
  const metrics = useSourceMetrics(platform, period, custom, model, queryEndMs)
  useEffect(() => {
    if (!isTauri) return
    let cancelled = false
    void api.listSources(platform)
      .then(sources => { if (!cancelled) setResult({ platform, sources }) })
      .catch(error => { if (!cancelled) setResult({ platform, sources: [], error: String(error) }) })
    return () => { cancelled = true }
  }, [platform, queryEndMs])
  const config = platformConfig(platform)
  const current = result?.platform === platform ? result : null
  return <section className="flex flex-col gap-3 rounded-card border bg-surface p-4" aria-label="本机来源状态">
    <p className="text-sm font-semibold">{config.name} · 本机记录</p>
    <p className="text-xs text-text-secondary">{config.limitation}。统计覆盖本机留存记录，按平台合并，不能代表账号账单或剩余额度。</p>
    {current?.sources.map(source => <p key={source.id} className="text-xs text-text-secondary">
      {source.name}：{source.available ? "已发现来源" : source.reason ?? "来源不可用"}
    </p>)}
    {isTauri && !current && <p className="text-xs text-text-tertiary">正在检查本机来源…</p>}
    {(current?.error || metrics.error) && <p role="alert" className="text-xs text-danger">{current?.error ?? metrics.error}</p>}
    {platformSupports(platform, "credits") && <div className="flex flex-wrap gap-6 border-t pt-3">
      <div><p className="text-xs text-text-secondary">区间上报积分</p><p className="tnum text-xl font-semibold">{metrics.data?.credits?.toLocaleString("zh-CN", { maximumFractionDigits: 4 }) ?? "—"}</p><p className="text-[11px] text-text-tertiary">{metrics.data?.credit_requests ?? "—"} 条含积分记录；不等于实际扣款或余额</p></div>
      <div><p className="text-xs text-text-secondary">最近上下文占用</p><p className="tnum text-xl font-semibold">{metrics.data?.context_ratio == null ? "—" : `${(metrics.data.context_ratio * 100).toFixed(1)}%`}</p><p className="text-[11px] text-text-tertiary">{metrics.data?.context_at_ms ? new Date(metrics.data.context_at_ms).toLocaleString("zh-CN") : "该范围暂无上下文快照"} · 非套餐额度</p></div>
    </div>}
  </section>
}
