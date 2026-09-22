import { useId, useState } from "react"
import { IslandConnectionSwitcher } from "./IslandConnectionSwitcher"
import { cn } from "@/lib/utils"
import { SHIMMER_CYCLE_MS, SHIMMER_MIN_CYCLES, useCycleExit } from "@/lib/motion"
import { authQuotaRows } from "@/lib/authQuotaRows"
import { PlatformLogo } from "@/components/brand/PlatformLogo"
import { ConnectionKindChip, Divider, StatusDot } from "@/components/ui/primitives"
import { UsageRow } from "./UsageRow"
import { AnimatedTokens, TokenDelta, type DeltaPhase } from "./TokenDelta"
import { SessionList } from "./SessionList"
import type { PlatformId, QuotaWindow, SessionActivity } from "@/types"

/**
 * UsageIsland —— 宽 400、padding [12,16]、gap 10、圆角 18（附录 A.4）
 *
 * 收缩态不显示「打开主面板」按钮；双击进入展开态，
 * 展开态右上角提供连接切换入口。
 */
export const ISLAND_WIDTH = 400

export interface IslandData {
  platform: PlatformId
  platformName: string
  /** 接入方式只有官方订阅（Auth，绿）与 API Key（API，蓝）两种。决定徽标颜色与文字 */
  kind: "auth" | "api"
  localMetrics?: { token: string | null; credits: string | null; message: string }
  /**
   * 额度数据不可用（未连接 / 查询失败 / 该来源不提供额度）。
   * Auth 仍保留额度布局与未知值，不回退到 API 指标。
   */
  quotaUnavailable?: boolean
  quotaMessage?: string
  /** Auth：额度窗口；API 模式不使用 */
  quotas?: QuotaWindow[]
  /** API：今日 Token 与今日费用 */
  apiToday?: { token: string | null; cost: string | null; costEstimated?: boolean; tokenLabel?: string }
  balanceText?: string
  /** 网关余额水位（§2.5）：充足绿 / 耗尽红；缺省按充足处理 */
  balanceLevel?: "healthy" | "empty"
  /** 网关余额的获取时刻（快照）。余额不是实时值，标注时刻避免与网关面板即时值对不上时无从判断 */
  balanceTimeText?: string
  apiMessage?: string
  connectionLabel?: string
  sessions?: SessionActivity[]
  sessionCountText?: string | null
  deltaText?: string | null
  /** 实时累计增量数值；有值时数字带补间动画，优先于 deltaText */
  deltaTokens?: number | null
  deltaPhase?: DeltaPhase
  todayTokenText?: string | null
  sourceText?: string
  status?: { tone: "success" | "warn" | "danger"; label: string }
  /** 会话区标题。来源无可信运行状态时用「最近活跃会话」，不得称为运行中（§2.1.2） */
  sessionSectionTitle?: string
  /** 采集源中断时，把当前会话显式标为状态未知。 */
  sessionStatusUnknown?: boolean
  /** 未连接 / 连接过期时的说明，设置后不画进度条 */
  unavailable?: { title: string; hint: string; tone: "warn" | "danger" }
}

function Shell({
  className,
  children,
  onDoubleClick,
  minHeight,
  refreshKey,
  refreshing,
}: {
  className?: string
  children: React.ReactNode
  onDoubleClick?: () => void
  minHeight?: number
  /** 刷新序号：变化即起跑一轮整卡柔光，不依赖 `refreshing` 是否被观察到为真 */
  refreshKey?: number
  /** 刷新进行中：柔光循环掠过；结束后补齐当前这一轮再收 */
  refreshing?: boolean
}) {
  const shimmering = useCycleExit(refreshKey ?? 0, Boolean(refreshing), SHIMMER_CYCLE_MS, SHIMMER_MIN_CYCLES)

  return (
    <div
      onDoubleClick={onDoubleClick}
      // 拖动由 IslandWindow 统一处理，避免与系统默认拖拽重复启动。
      className={cn(
        "island-shell relative flex shrink-0 select-none flex-col gap-2.5 rounded-island border bg-surface px-4 py-3 shadow-island",
        className,
      )}
      style={{ width: `var(--island-shell-width, ${ISLAND_WIDTH}px)`, minHeight }}
    >
      {shimmering && (
        /* 不带 key：一旦重挂动画就从起点重来，正是「卡一下」的来源。
           整个生命周期只挂载一次，由 useCycleExit 在轮次边界摘除。 */
        <span aria-hidden className="island-refresh-veil">
          <span className="island-refresh-shimmer" />
        </span>
      )}
      {children}
    </div>
  )
}

function PlatformHead({ data }: { data: IslandData }) {
  return (
    <div className="flex items-center gap-1.5">
      <PlatformLogo platform={data.platform} />
      <span className="text-[13px] font-semibold text-text-primary">{data.platformName}</span>
      <ConnectionKindChip kind={data.kind} local={!!data.localMetrics} />
    </div>
  )
}

/** 收缩态：Auth 双额度 / API 今日 Token 与费用 / 未连接 */
export function IslandCollapsed({
  data,
  onExpand,
  refreshKey,
  refreshing,
}: {
  data: IslandData
  onExpand?: () => void
  refreshKey?: number
  refreshing?: boolean
}) {
  // Auth 与 API 布局由接入类型决定，额度缺失保留空态。
  // 编程套餐（api 连接）现在也能返回额度窗口，有窗口就按额度布局显示。
  const showQuotas = data.kind === "auth" || (data.quotas?.length ?? 0) > 0
  const quotaRows = data.platform === "grok" ? data.quotas ?? [] : authQuotaRows(data.quotas)
  return (
    <Shell onDoubleClick={onExpand} refreshKey={refreshKey} refreshing={refreshing}>
      <div className="flex w-full items-center justify-between">
        <PlatformHead data={data} />
        {/* 只在来源提供可信运行状态时显示，否则留白 */}
        {data.sessionCountText && (
          <span className="text-[11px] text-text-secondary">{data.sessionCountText}</span>
        )}
      </div>

      <div className="flex w-full items-center gap-4">
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          {data.unavailable ? (
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-1.5">
                <StatusDot tone={data.unavailable.tone} />
                <span
                  className={cn(
                    "text-xs font-medium",
                    data.unavailable.tone === "warn" ? "text-warn" : "text-danger",
                  )}
                >
                  {data.unavailable.title}
                </span>
              </div>
              <p className="text-[11px] text-text-tertiary">{data.unavailable.hint}</p>
            </div>
          ) : data.localMetrics ? (
            <><MetricRow label="本机今日 Token" value={data.localMetrics.token} /><MetricRow label="今日上报积分" value={data.localMetrics.credits} /></>
          ) : showQuotas ? (
            quotaRows.slice(0, 2).map((q) => <UsageRow key={q.key} quota={q} />)
          ) : (
            <>
              <MetricRow label={data.apiToday?.tokenLabel ?? "今日 Token"} value={data.apiToday?.token} />
              <MetricRow
                label={data.apiToday?.costEstimated ? "本机费用（估算）" : "今日费用"}
                value={data.apiToday?.cost}
                tone={data.apiToday?.costEstimated ? "warn" : "default"}
              />
            </>
          )}
        </div>
        <TokenDelta
          text={data.unavailable ? "—" : (data.deltaText ?? null)}
          tokens={data.unavailable ? null : data.deltaTokens}
          phase={data.deltaPhase ?? "hold"}
        />
      </div>
      {showQuotas && !data.localMetrics && data.quotaMessage && (
        <p className="w-full text-[10px] leading-relaxed text-text-tertiary">{data.quotaMessage}</p>
      )}
      {!showQuotas && data.apiMessage && <p className="text-[10px] leading-relaxed text-text-tertiary">{data.apiMessage}</p>}
    </Shell>
  )
}

function MetricRow({
  label,
  value,
  hint,
  tone = "default",
}: {
  label: string
  value?: string | null
  /** 值后的小字补充，如余额快照的获取时刻；仅在有意义值时显示 */
  hint?: string | null
  tone?: "default" | "warn" | "success" | "empty"
}) {
  return (
    <div className="flex w-full items-center justify-between gap-3">
      <span className="shrink-0 text-xs text-text-secondary">{label}</span>
      {/* 连接名可以很长（CC Switch 的名称 + 掩码），不截断会把整行挤成两行、顶开岛体高度 */}
      <span className="flex min-w-0 items-baseline gap-1.5">
        <span
          title={value ?? undefined}
          className={cn(
            "tnum min-w-0 truncate font-mono text-[13px] font-semibold",
            value == null
              ? "text-text-tertiary"
              : tone === "warn"
                ? "text-warn"
                : tone === "success"
                  ? "text-success-text"
                  : tone === "empty"
                    ? "text-danger"
                    : "text-text-primary",
          )}
        >
          {value ?? "—"}
        </span>
        {hint && value != null && <span className="shrink-0 text-[10px] font-normal text-text-tertiary">{hint}</span>}
      </span>
    </div>
  )
}

/**
 * 展开态：右上角提供连接切换入口。
 * 双击岛体收回收缩态（§2.1 的双击规则在两个方向上都成立）；
 * 连接切换不触发拖动或收缩。
 */
export function IslandExpanded({
  data,
  connectionSwitcher,
  connectionMenuOpen = false,
  onCollapse,
  refreshKey,
  refreshing,
}: {
  data: IslandData
  connectionSwitcher?: React.ReactNode
  connectionMenuOpen?: boolean
  onCollapse?: () => void
  refreshKey?: number
  refreshing?: boolean
}) {
  const sessionTotal = data.sessions?.length ?? 0
  // 展开态与收缩态使用同一套额度规则：有额度窗口（auth 或编程套餐）即按额度布局。
  const showQuotas = data.kind === "auth" || (data.quotas?.length ?? 0) > 0
  const quotaRows = data.platform === "grok" ? data.quotas ?? [] : authQuotaRows(data.quotas)
  return (
    <Shell onDoubleClick={onCollapse} minHeight={connectionMenuOpen ? 240 : undefined} refreshKey={refreshKey} refreshing={refreshing}>
      <div className="flex w-full items-center justify-between">
        <PlatformHead data={data} />
        {connectionSwitcher ?? <IslandConnectionSwitcher />}
      </div>

      <Divider />

      {showQuotas && !data.localMetrics && (
        <div className="flex w-full items-center justify-between">
          <span className="text-[11px] text-text-secondary">{data.kind === "auth" ? "官方订阅额度" : "套餐额度"}</span>
          {data.status && (
            <span className={cn("inline-flex items-center gap-[5px] whitespace-nowrap rounded-[8px] px-2 py-[3px] text-[11px] font-medium",
              data.status.tone === "success" ? "bg-success-soft text-success-text" : data.status.tone === "warn" ? "bg-warn-soft text-warn" : "bg-danger-soft text-danger")}>
              <StatusDot tone={data.status.tone} />
              {data.status.label}
            </span>
          )}
        </div>
      )}

      {data.localMetrics ? (
        <div className="flex flex-col gap-2"><MetricRow label="本机今日 Token" value={data.localMetrics.token} /><MetricRow label="今日上报积分" value={data.localMetrics.credits} /><p className="text-[10px] text-text-tertiary">{data.localMetrics.message}</p></div>
      ) : showQuotas ? (
        <div className="flex w-full flex-col gap-2">
          {quotaRows.slice(0, 2).map((q) => <UsageRow key={q.key} quota={q} />)}
          {(data.quotas?.length ?? 0) > 2 && (
            <p className="text-[10px] text-text-tertiary">另有 {(data.quotas?.length ?? 0) - 2} 个额度窗口，请在主面板查看</p>
          )}
          {data.balanceText && <MetricRow label="网关余额" value={data.balanceText} hint={data.balanceTimeText} tone={data.balanceLevel === "empty" ? "empty" : "success"} />}
          {data.quotaMessage && <p className="text-[10px] text-text-tertiary">{data.quotaMessage}</p>}
        </div>
      ) : (
        <div className="flex w-full flex-col gap-1.5">
          <MetricRow label={data.apiToday?.tokenLabel ?? "今日 Token"} value={data.apiToday?.token} />
          <MetricRow
            label={data.apiToday?.costEstimated ? "本机费用（估算）" : "今日费用"}
            value={data.apiToday?.cost}
            tone={data.apiToday?.costEstimated ? "warn" : "default"}
          />
          {data.connectionLabel && <MetricRow label="连接" value={data.connectionLabel} />}
          {data.balanceText && <MetricRow label="网关余额" value={data.balanceText} hint={data.balanceTimeText} tone={data.balanceLevel === "empty" ? "empty" : "success"} />}
          {data.apiMessage && <p className="text-[10px] leading-relaxed text-text-tertiary">{data.apiMessage}</p>}
        </div>
      )}

      {(sessionTotal > 0 || data.deltaTokens != null) && (
        <>
          <Divider />
          <div className="flex w-full items-center justify-between">
            <span className="text-xs font-medium text-text-primary">
              {sessionTotal > 0 ? (data.sessionSectionTitle ?? `运行中的会话 · ${sessionTotal}`) : "本轮 Token"}
            </span>
            <AnimatedTokens tokens={data.deltaTokens} text={data.deltaText} />
          </div>
          {sessionTotal > 0 && <SessionList sessions={data.sessions!} showUnknownTag={data.sessionStatusUnknown} />}
        </>
      )}

      {data.todayTokenText && (
        <>
          <Divider />
          <MetricRow label="本地今日 Token" value={data.todayTokenText} />
        </>
      )}

      {data.sourceText && <SourceFooter text={data.sourceText} />}
    </Shell>
  )
}

function SourceFooter({ text }: { text: string }) {
  const id = useId()
  const [open, setOpen] = useState(false)
  return (
    <div className="relative w-full border-t pt-2 text-[10px] leading-relaxed text-text-tertiary"
      onMouseEnter={() => setOpen(true)} onMouseLeave={() => setOpen(false)}>
      <button type="button" className="block w-full truncate text-left" aria-label="查看完整统计来源说明"
        aria-describedby={open ? id : undefined} onFocus={() => setOpen(true)} onBlur={() => setOpen(false)}
        onClick={() => setOpen(true)} onDoubleClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => { if (event.key === "Escape") { setOpen(false); event.stopPropagation() } }}>
        {text}
      </button>
      {open && <div id={id} role="tooltip" className="motion-popover absolute inset-x-0 bottom-full z-30 max-h-[calc(100dvh-48px)] overflow-auto rounded-[8px] border bg-surface p-3 text-text-secondary shadow-popover">{text}</div>}
    </div>
  )
}
