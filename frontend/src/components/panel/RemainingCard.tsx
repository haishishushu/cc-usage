import { QueryStateNotice } from "./QueryStateNotice"
import { formatQuotaCountdown } from "@/lib/quotaMap"
import type { RemainingDisplay } from "@/lib/remainingDisplay"

export function RemainingCard({ display, fetchedAt, stale, loading }: {
  display: RemainingDisplay
  fetchedAt?: Date | null
  stale?: boolean
  loading?: boolean
}) {
  return <section className="flex w-full flex-col gap-4 rounded-card border bg-surface p-5" aria-label={display.title}>
    <header className="flex flex-wrap items-start justify-between gap-2">
      <div>
        <h3 className="text-sm font-semibold text-text-primary">{display.title}</h3>
        <p className="mt-1 text-[11px] text-text-tertiary">额度来源：{display.source}</p>
      </div>
      {fetchedAt && <span className="text-[11px] text-text-tertiary">更新于 {fetchedAt.toLocaleTimeString("zh-CN", { hour12: false })}</span>}
    </header>
    {display.rows.length > 0 ? <div className="flex flex-col gap-3">
      {display.rows.map((row, index) => <div key={`${row.label}:${index}`} className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 border-t pt-3 first:border-0 first:pt-0">
        <span className="text-xs text-text-secondary">{row.label}</span>
        <span className="tnum font-mono text-xl font-semibold text-text-primary">{row.value}</span>
        {(row.usedText || row.resetsAt) && <div className="flex w-full flex-wrap justify-between gap-2 text-[11px] text-text-tertiary">
          <span>{row.usedText}</span>
          <span>{row.resetsAt ? `重置倒计时 ${formatQuotaCountdown(row.resetsAt) ?? "待更新"}` : ""}</span>
        </div>}
      </div>)}
    </div> : <div className="flex items-baseline gap-3"><span className="text-xs text-text-secondary">剩余额度</span><span className="tnum font-mono text-xl font-semibold text-text-tertiary">—</span></div>}
    {display.state !== "ok" && <QueryStateNotice state={display.state} reason={display.reason} />}
    {stale && fetchedAt && <QueryStateNotice state="failed" reason="本次刷新失败，显示上次取得的额度。" stale fetchedAt={fetchedAt} />}
    {loading && display.state === "ok" && <p className="text-[11px] text-text-tertiary">正在更新额度…</p>}
  </section>
}
