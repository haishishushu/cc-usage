import { CircleAlert, Clock, Loader, ShieldOff, TriangleAlert } from "lucide-react"
import { cn } from "@/lib/utils"

/**
 * 额度 / 余额的查询状态提示 —— §2.5
 *
 * 五种状态：加载、不支持查询、权限不足、凭证失效、查询失败（含频率限制）。
 * **不可用时展示原因，绝不画 0% 或满格进度条**，与「真实已用 100%」区分。
 */
export type QueryState =
  | "loading"
  | "unsupported"
  | "forbidden"
  | "unauthorized"
  | "rate_limited"
  | "failed"

const MAP: Record<
  Exclude<QueryState, "loading">,
  { title: string; tone: "warn" | "danger" | "neutral"; icon: typeof CircleAlert }
> = {
  // 「不支持」是能力边界，不是错误，用中性色不用警告色
  unsupported: { title: "不支持查询", tone: "neutral", icon: ShieldOff },
  forbidden: { title: "权限不足", tone: "warn", icon: ShieldOff },
  unauthorized: { title: "凭证失效", tone: "danger", icon: TriangleAlert },
  rate_limited: { title: "已触发频率限制", tone: "warn", icon: Clock },
  failed: { title: "查询失败", tone: "danger", icon: CircleAlert },
}

const TONE: Record<"warn" | "danger" | "neutral", { bg: string; fg: string }> = {
  warn: { bg: "bg-warn-soft", fg: "text-warn" },
  danger: { bg: "bg-danger-soft", fg: "text-danger" },
  neutral: { bg: "bg-neutral-soft", fg: "text-text-secondary" },
}

export function QueryStateNotice({
  state,
  reason,
  stale,
  fetchedAt,
  className,
}: {
  state: QueryState
  reason?: string
  /** 展示的是上次成功的旧值：需标注过期与最后更新时间（§2.5） */
  stale?: boolean
  fetchedAt?: Date | null
  className?: string
}) {
  if (state === "loading") {
    return (
      <div
        className={cn(
          "flex w-full items-center gap-2 rounded-[10px] bg-surface-2 px-3 py-2.5",
          className,
        )}
      >
        <Loader className="size-3.5 shrink-0 animate-spin text-accent-blue" />
        <span className="text-[11px] text-text-secondary">正在查询…</span>
      </div>
    )
  }

  const m = MAP[state]
  const t = TONE[m.tone]
  const Icon = m.icon
  return (
    <div className={cn("flex w-full items-start gap-2 rounded-[10px] px-3 py-2.5", t.bg, className)}>
      <Icon className={cn("mt-0.5 size-3.5 shrink-0", t.fg)} />
      <div className="flex min-w-0 flex-1 flex-col gap-[3px]">
        <span className={cn("text-xs font-medium", t.fg)}>{m.title}</span>
        {reason && (
          <span className={cn("text-[11px] leading-[1.5]", t.fg)}>{reason}</span>
        )}
        {stale && fetchedAt && (
          <span className="text-[11px] leading-[1.5] text-text-tertiary">
            当前显示的是 {fetchedAt.toLocaleTimeString("zh-CN", { hour12: false })} 的数据，可能已过期
          </span>
        )}
      </div>
    </div>
  )
}
