import { cn } from "@/lib/utils"
import { balanceTextClass } from "@/lib/quota"
import { Chip } from "@/components/ui/primitives"
import { QueryStateNotice, type QueryState } from "./QueryStateNotice"
import type { Balance } from "@/types"
import { useSettings } from "@/lib/useSettings"
import { balanceAlert } from "@/lib/balanceAlert"

/**
 * BalanceCard —— §2.5
 *
 * 五种**查询状态**（加载 / 已获取 / 不支持查询 / 权限不足 / 查询失败）
 * 与四档**水位着色**（充足绿 / 偏低琥珀 / 耗尽红 / 不可用灰）相互独立。
 *
 * 绿色使用可读性更高的 success-text（#06834A / #3ADB8B），
 * 不使用额度进度条的 #06C167，以保证 32px 大号数字在白底上的对比度。
 * 告警阈值待确认，阈值未定前不得声称某个余额「充足」。
 */
export function BalanceCard({
  balance,
  queryState,
  reason,
  stale,
  fetchedAt,
  amount = null,
}: {
  balance: Balance
  /** 非 undefined 时余额不可用：显示原因，不显示金额（§2.5） */
  queryState?: QueryState
  reason?: string
  stale?: boolean
  fetchedAt?: Date | null
  amount?: number | null
}) {
  const { settings } = useSettings()
  const warn = balanceAlert(amount, balance.currency, settings.balance_alert_threshold, settings.balance_alert_currency, !!stale || !!queryState)
  return (
    <section className="flex w-full flex-col gap-4 rounded-card border bg-surface p-5">
      <header className="flex w-full items-center justify-between">
        <div className="flex items-center gap-2.5">
          <h3 className="text-sm font-semibold text-text-primary">剩余余额</h3>
          {/* 只按接口实际返回标注，不能把共享余额说成 Key 独立余额 */}
          <Chip tone="neutral">{balance.scopeLabel}</Chip>
        </div>
        <span className="text-[11px] text-text-tertiary">{balance.sourceText}</span>
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
        {warn && <p role="status" className="rounded-lg bg-warn-soft px-3 py-2 text-xs text-warn">余额已低于或等于设置的 {settings.balance_alert_threshold} {settings.balance_alert_currency} 提醒阈值</p>}
        <div className="flex w-full items-end gap-2.5">
          <span
            className={cn("tnum font-mono text-[32px] font-semibold leading-none", balanceTextClass(balance.level))}
          >
            {balance.amountText ?? "—"}
          </span>
          <span className="text-xs text-text-secondary">{balance.currency}</span>
          <span className="flex-1" />
          <span className="text-[11px] text-text-tertiary">
            余额范围：{balance.scopeLabel}（接口未提供当前 Key 独立余额）
          </span>
        </div>
        {queryState && stale && (
          <QueryStateNotice state={queryState} reason={reason} stale fetchedAt={fetchedAt} />
        )}
      </>
      )}

    </section>
  )
}
