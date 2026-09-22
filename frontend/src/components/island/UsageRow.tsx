import { cn } from "@/lib/utils"
import { quotaBarClass } from "@/lib/quota"
import type { QuotaWindow } from "@/types"

/**
 * 原型额度行：周期、84px 短轨道、百分比、倒计时。
 *
 * 固定列宽保持两行对齐，收缩态也为右侧实时 Token 留出独立空间。
 * 标签配色表示「窗口身份」，不随水位变化；只有进度条填充按阈值换色。
 */
const BADGE_TONE: Record<NonNullable<QuotaWindow["badgeTone"]>, string> = {
  purple: "bg-purple-soft text-purple-text",
  green: "bg-success-soft text-success-text",
  blue: "bg-accent-blue-soft text-accent-blue",
  neutral: "bg-neutral-soft text-text-secondary",
}

export function UsageRow({ quota, showReset = true }: { quota: QuotaWindow; showReset?: boolean }) {
  const pct = quota.usedPercent
  // 无总量分母时不生成百分比进度条（§7.4）
  const hasBar = pct !== null

  return (
    <div
      className="grid w-fit max-w-full items-center gap-2"
      style={{ gridTemplateColumns: showReset ? "26px 84px 34px 56px" : "26px 84px 34px" }}
      data-quota-row={quota.key}
    >
      <span
        className={cn(
          "justify-self-start whitespace-nowrap rounded-[6px] px-1.5 py-0.5 font-mono text-[11px] font-medium leading-normal",
          BADGE_TONE[quota.badgeTone],
        )}
      >
        {quota.key}
      </span>

      <div
        className="h-1.5 min-w-0 overflow-hidden rounded-[3px] bg-track"
      >
        {hasBar && (
          <div
            className={cn("motion-quota-bar h-full rounded-[3px]", quotaBarClass(pct))}
            style={{ width: `${Math.max(0, Math.min(100, pct))}%` }}
          />
        )}
      </div>

      <span className="tnum whitespace-nowrap text-right font-mono text-xs text-text-primary">
        {pct === null ? "—" : `${pct}%`}
      </span>
      {showReset && (
        <span
          data-quota-reset
          className="tnum whitespace-nowrap text-right font-mono text-xs leading-5 text-text-secondary"
          title={quota.resetCountdown ?? "未提供重置时间"}
          aria-label={quota.resetCountdown ?? "未提供重置时间"}
        >
          {quota.resetCountdown?.replace(/^还剩\s*/, "") ?? "—"}
        </span>
      )}
    </div>
  )
}
