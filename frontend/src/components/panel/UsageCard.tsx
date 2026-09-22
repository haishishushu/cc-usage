import { cn } from "@/lib/utils"
import { quotaBarClass, quotaLevelChipClass, quotaLevelLabel } from "@/lib/quota"
import { ConnectionKindChip, StatusDot } from "@/components/ui/primitives"
import { QueryStateNotice, type QueryState } from "./QueryStateNotice"
import type { QuotaWindow } from "@/types"

/** 额度卡：padding 20，宽进度条 1024×8 r4（附录 A.4） */
const WIDE_TRACK = 1024

const BADGE_TONE: Record<NonNullable<QuotaWindow["badgeTone"]>, string> = {
  purple: "bg-purple-soft text-purple-text",
  green: "bg-success-soft text-success-text",
  blue: "bg-accent-blue-soft text-accent-blue",
  neutral: "bg-neutral-soft text-text-secondary",
}

function WideQuotaRow({ quota }: { quota: QuotaWindow }) {
  const pct = quota.usedPercent
  return (
    <div className="flex w-full flex-col gap-2.5">
      <div className="flex w-full items-center justify-between gap-x-4 gap-y-1">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              "rounded-[6px] px-[7px] py-0.5 font-mono text-[11px] font-medium",
              BADGE_TONE[quota.badgeTone],
            )}
          >
            {quota.key}
          </span>
          <span className="text-xs text-text-secondary">{quota.windowName}</span>
        </div>
        <div className="flex items-center gap-3.5">
          {/* 主面板宽行给出等级文字，灵动岛收缩态不显示（§3） */}
          {pct !== null && (
            <span
              className={cn(
                "rounded-[6px] px-[7px] py-0.5 text-[10px] font-medium",
                quotaLevelChipClass(pct),
              )}
            >
              {quotaLevelLabel(pct)}
            </span>
          )}
          <span className="tnum font-mono text-[13px] font-semibold text-text-primary">
            {pct === null ? "—" : `${pct}%`}
          </span>
        </div>
      </div>

      <div
        className="h-2 overflow-hidden rounded-[4px] bg-track"
        style={{ width: WIDE_TRACK, maxWidth: "100%" }}
      >
        {pct !== null && (
          <div
            className={cn("h-full rounded-[4px]", quotaBarClass(pct))}
            style={{ width: `${pct}%` }}
          />
        )}
      </div>

      <div className="flex w-full flex-wrap items-center justify-between gap-x-4 gap-y-1 text-[11px] text-text-tertiary">
        <span className="min-w-0">{quota.usedText ?? "该窗口未提供已用 / 总量"}</span>
        <span className="whitespace-nowrap">
          {quota.resetCountdown ? `重置倒计时 ${quota.resetCountdown}` : "重置时间未提供"}
        </span>
      </div>
    </div>
  )
}

export function UsageCard({
  title,
  kind,
  statusLabel,
  statusTone = "success",
  updatedText,
  quotas,
  queryState,
  reason,
  stale,
  fetchedAt,
}: {
  title: string
  kind: "auth" | "api"
  statusLabel: string
  statusTone?: "success" | "warn" | "danger"
  updatedText: string
  quotas: QuotaWindow[]
  /** 非 undefined 时，额度数据不可用：显示原因而不画进度条（§2.5） */
  queryState?: QueryState
  reason?: string
  stale?: boolean
  fetchedAt?: Date | null
}) {
  return (
    <section className="flex w-full flex-col gap-5 rounded-card border bg-surface p-5">
      <header className="flex w-full flex-wrap items-center justify-between gap-x-4 gap-y-2">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h3 className="shrink-0 text-sm font-semibold text-text-primary">{title}</h3>
          <ConnectionKindChip kind={kind} />
          <span className={cn("inline-flex items-center gap-1.5 rounded-[8px] px-2 py-[3px] text-[11px] font-medium", statusTone === "success" ? "bg-success-soft text-success-text" : statusTone === "warn" ? "bg-warn-soft text-warn" : "bg-danger-soft text-danger")}>
            <StatusDot tone={statusTone} />
            {statusLabel}
          </span>
        </div>
        <span className="whitespace-nowrap text-[11px] text-text-tertiary">{updatedText}</span>
      </header>
      {queryState && !stale ? (
        <QueryStateNotice
          state={queryState}
          reason={reason}
          stale={stale}
          fetchedAt={fetchedAt}
        />
      ) : (
        <>
          {quotas.map((q) => <WideQuotaRow key={q.key} quota={q} />)}
          {queryState && stale && (
            <QueryStateNotice state={queryState} reason={reason} stale fetchedAt={fetchedAt} />
          )}
        </>
      )}
    </section>
  )
}
