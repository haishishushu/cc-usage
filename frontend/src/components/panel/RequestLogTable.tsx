import { AlertTriangle, Check, CircleSlash, Inbox, Minus } from "lucide-react"
import { cn } from "@/lib/utils"
import { shortModelName } from "@/lib/modelName"
import {
  LATENCY_TEXT_CLASSES,
  durationSeverity,
  firstTokenSeverity,
  formatLatency,
} from "@/lib/latencyHealth"
import type { LogTableState, RequestLogRow } from "@/types"

/**
 * RequestLogTable —— 八列固定，基准宽 1064（附录 A.3 / A.4）
 * 时间 128 / 计费模型 248 / 思考强度 96 / 输入 108 / 输出 108 / 总成本 128 /
 * 用时·首字 144 / 状态 104
 * 表头高 44、数据行高 64、单元格内边距 [0,16]、只用细横线分行、不画竖线。
 * 表头与数据一律水平垂直居中。
 *
 * 附录 A.5-10：窄窗口在表格内横向滚动，不隐藏列；宽屏多出的空间按各列
 * 基准宽的比例分摊（flexGrow = 基准宽），不单独放大计费模型列，
 * 避免模型列独占余量、数字列挤在右侧。
 */
export const LOG_TABLE_WIDTH = 1064

const COLUMNS = [
  { key: "time", label: "时间", width: 128 },
  { key: "model", label: "计费模型", width: 248 },
  { key: "effort", label: "思考强度", width: 96 },
  { key: "input", label: "输入", width: 108 },
  { key: "output", label: "输出", width: 108 },
  { key: "cost", label: "总成本", width: 128 },
  { key: "duration", label: "用时 / 首字", width: 144 },
  { key: "status", label: "状态", width: 104 },
]

function Cell({
  width,
  children,
  className,
}: {
  /** 基准宽，同时作为 flex 分配权重 */
  width: number
  children?: React.ReactNode
  className?: string
}) {
  return (
    <div
      className={cn("flex h-full min-w-0 flex-col items-center justify-center gap-0.5 px-4", className)}
      style={{ flexGrow: width, flexShrink: 1, flexBasis: 0 }}
    >
      <div className="min-w-0 max-w-full text-center">{children}</div>
    </div>
  )
}

function StatusCell({ status }: { status: RequestLogRow["status"] }) {
  const map = {
    success: { cls: "bg-success-soft text-success-text", Icon: Check },
    "rate-limited": { cls: "bg-warn-soft text-warn", Icon: AlertTriangle },
    failed: { cls: "bg-danger-soft text-danger", Icon: AlertTriangle },
    unknown: { cls: "bg-neutral-soft text-text-secondary", Icon: Minus },
  }[status.kind]
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-[8px] px-2 py-[3px] font-mono text-[11px] font-medium",
        map.cls,
      )}
    >
      <map.Icon className="size-3" />
      {status.label}
    </span>
  )
}

/** 缺失值统一显示「—」，不伪造为 0 */
function Missing() {
  return <span className="text-text-tertiary">—</span>
}

/**
 * 用时 / 首字：两行各按自己那套阈值分档着色（见 lib/latencyHealth）。
 * 慢的那一项自己变色，不牵连另一项——否则无法一眼看出是「开口慢」还是「生成久」。
 *
 * 两者都只有经本地代理转发时才测得到；拿不到就显示「—」，不伪造 0.0s。
 */
function LatencyCell({ durationMs, firstTokenMs }: { durationMs: number | null; firstTokenMs: number | null }) {
  const duration = formatLatency(durationMs)
  const firstToken = formatLatency(firstTokenMs)
  return (
    <>
      <div
        className={cn(
          "tnum font-mono text-[13px]",
          duration === null ? "text-text-primary" : LATENCY_TEXT_CLASSES[durationSeverity(durationMs!)],
        )}
      >
        {duration ?? <Missing />}
      </div>
      <div
        className={cn(
          "tnum font-mono text-xs",
          firstToken === null ? "text-text-secondary" : LATENCY_TEXT_CLASSES[firstTokenSeverity(firstTokenMs!)],
        )}
      >
        <span className="text-[11px]">首字</span> {firstToken ?? "—"}
      </div>
    </>
  )
}

function Row({ row, last, planCovered, planHint }: { row: RequestLogRow; last: boolean; planCovered: boolean; planHint: string | null }) {
  return (
    <div
      className={cn("flex h-16 w-full items-center hover:bg-hover", !last && "border-b")}
      style={{ minWidth: LOG_TABLE_WIDTH }}
    >
      <Cell width={128}>
        <div className="tnum font-mono text-[13px] text-text-primary">{row.time}</div>
        <div className="tnum font-mono text-xs text-text-secondary">{row.date}</div>
      </Cell>
      <Cell width={248}>
        {/* 展示短名，完整名（含日期后缀）留给悬浮提示 */}
        <div
          className="truncate font-mono text-[13px] text-text-primary"
          title={row.model ?? undefined}
        >
          {row.model ? shortModelName(row.model) : <Missing />}
        </div>
      </Cell>
      <Cell width={96}>
        <div className="truncate font-mono text-[13px] text-text-primary" title={row.effort ?? undefined}>
          {row.effort ?? <Missing />}
        </div>
      </Cell>
      <Cell width={108}>
        <div className="tnum font-mono text-[13px] text-text-primary" title={row.inputHint ?? undefined}>
          {row.input ?? <Missing />}
        </div>
      </Cell>
      <Cell width={108}>
        <div className="tnum font-mono text-[13px] text-text-primary">
          {row.output ?? <Missing />}
        </div>
      </Cell>
      <Cell width={128}>
        {/* 套餐制下单条请求不单独扣费，按价目表推算的金额不是真实支出，故只说明「套餐内」 */}
        {planCovered ? (
          <div
            className="text-[13px] text-text-primary"
            title={planHint ? `本次请求包含在套餐额度内 · ${planHint}` : "本次请求包含在套餐额度内，不单独计费"}
          >
            套餐内
          </div>
        ) : (
          <>
            <div className="tnum font-mono text-[13px] font-semibold text-text-primary">
              {row.cost ?? <Missing />}
            </div>
            {/* 成本为估算值时必须标识，不能当作实际账单金额 */}
            {row.costEstimated && (
              <div
                className="text-[11px] text-warn"
                title="按公开 API 价目表乘 Token 数推算；订阅用户的实际支出是月费，不是该金额"
              >
                估算
              </div>
            )}
          </>
        )}
      </Cell>
      <Cell width={144}>
        <LatencyCell durationMs={row.durationMs} firstTokenMs={row.firstTokenMs} />
      </Cell>
      <Cell width={104}>
        <StatusCell status={row.status} />
      </Cell>
    </div>
  )
}

function SkeletonRow({ last }: { last: boolean }) {
  return (
    <div
      className={cn("flex h-16 w-full items-center", !last && "border-b")}
      style={{ minWidth: LOG_TABLE_WIDTH }}
    >
      {COLUMNS.map((c, i) => (
        <Cell key={c.key} width={c.width}>
          <div className="flex flex-col items-center gap-1.5">
            <div
              className="h-2.5 rounded-[5px] bg-surface-3"
              style={{ width: c.key === "model" ? "82%" : "60%" }}
            />
            {/* 时间与用时两列是双行内容，骨架也给两条 */}
            {(i === 0 || c.key === "duration") && (
              <div className="h-2 rounded-[4px] bg-surface-3 opacity-70" style={{ width: "42%" }} />
            )}
          </div>
        </Cell>
      ))}
    </div>
  )
}

function EmptyBody({
  icon: Icon,
  title,
  desc,
  action,
  onAction,
}: {
  icon: React.ComponentType<{ className?: string }>
  title: string
  desc: string
  action?: string
  onAction?: () => void
}) {
  return (
    <div className="flex h-[180px] w-full flex-col items-center justify-center gap-2 p-6">
      <Icon className="size-[22px] text-text-tertiary" />
      <p className="text-[13px] font-medium text-text-primary">{title}</p>
      <p className="text-[11px] text-text-tertiary">{desc}</p>
      {action && (
        <button
          type="button"
          onClick={onAction}
          className="mt-1 rounded-[8px] border bg-surface px-3.5 py-1.5 text-xs text-text-primary"
        >
          {action}
        </button>
      )}
    </div>
  )
}

export function RequestLogTable({
  rows,
  state = "ready",
  onRetry,
  planCovered = false,
  planHint = null,
}: {
  rows: RequestLogRow[]
  state?: LogTableState
  onRetry?: () => void
  /** 当前连接是套餐制（查到 5 小时 / 周额度）时，成本列不给金额 */
  planCovered?: boolean
  /** 套餐额度的真实消耗，放在成本列的悬停提示里 */
  planHint?: string | null
}) {
  return (
    <div className="w-full overflow-hidden rounded-card border bg-surface">
      {/* 窄窗口横向滚动，不隐藏列 */}
      <div className="w-full overflow-x-auto">
        <div style={{ minWidth: LOG_TABLE_WIDTH }}>
          <div className="flex h-11 w-full items-center border-b bg-surface-2">
            {COLUMNS.map((c) => (
              <Cell key={c.key} width={c.width}>
                <span className="text-xs font-medium text-text-secondary">{c.label}</span>
              </Cell>
            ))}
          </div>

          {state === "ready" &&
            rows.map((r, i) => <Row key={r.id} row={r} last={i === rows.length - 1} planCovered={planCovered} planHint={planHint} />)}
          {state === "loading" &&
            [0, 1, 2].map((i) => <SkeletonRow key={i} last={i === 2} />)}
          {state === "empty" && (
            <EmptyBody
              icon={Inbox}
              title="当前时间范围内暂无请求"
              desc="可调整统计周期或自定义时间范围后重试"
            />
          )}
          {state === "failed" && (
            <EmptyBody
              icon={AlertTriangle}
              title="请求日志查询失败"
              desc="来源暂时不可用，请检查连接后重试"
              action="重试"
              onAction={onRetry}
            />
          )}
          {state === "unsupported" && (
            <EmptyBody
              icon={CircleSlash}
              title="此来源不提供逐条请求日志"
              desc="该来源仅提供汇总数据；Token 汇总与趋势图不受影响"
            />
          )}
        </div>
      </div>
    </div>
  )
}

export { Minus as MissingIcon }
