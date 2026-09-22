import { Zap } from "lucide-react"
import { cn } from "@/lib/utils"
import { compactTokens, grouped, type UsageBreakdown } from "@/lib/api"
import { TOKEN_SERIES } from "@/lib/useUsage"

/**
 * UsageHero —— 当前统计范围的 Token 分项卡。
 *
 * 贯穿整个组件的一条规则：**未知与零必须可区分**。来源没提供某个字段时显示「—」
 * 并给出悬停说明，绝不显示 0——把「没说」画成 0 会让用户以为真的没有缓存写入。
 * 判空一律用 `== null`，因为真实的 0 是有效数据，不能被 falsy 判断吞掉。
 */

/** 未知值的统一呈现。`hint` 说明为什么未知，让「—」不是一个无解的符号 */
function Value({ value, hint }: { value: string | null; hint?: string }) {
  if (value === null) {
    return (
      <span className="text-text-tertiary" title={hint ?? "来源未提供该字段"}>
        —
      </span>
    )
  }
  return <>{value}</>
}

/** 分项格：左上角一个色点标明它在趋势图里对应哪条线 */
function MiniStat({
  label,
  colorVar,
  value,
  hint,
}: {
  label: string
  colorVar: string
  value: string | null
  hint?: string
}) {
  return (
    <div
      className="flex min-w-0 flex-col gap-1.5 rounded-sm2 border bg-surface-2 p-3"
      title={value === null ? hint : undefined}
    >
      <span className="flex items-center gap-1.5 text-[11px] text-text-secondary">
        <span
          aria-hidden
          className="size-2 shrink-0 rounded-full"
          style={{ background: `var(${colorVar})` }}
        />
        <span className="truncate">{label}</span>
      </span>
      <span className="tnum font-mono text-[15px] font-semibold text-text-primary">
        <Value value={value} hint={hint} />
      </span>
    </div>
  )
}

/** 顶部右侧的整体指标：请求数、平均每次、估算费用 */
function TopStat({ label, value, tone }: { label: string; value: string | null; tone?: "cost" }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 px-3.5 first:pl-0 last:pr-0">
      <span className="whitespace-nowrap text-[10px] tracking-wide text-text-secondary">{label}</span>
      <span
        className={cn(
          "tnum whitespace-nowrap font-mono text-[13px] font-semibold",
          tone === "cost" ? "text-warn" : "text-text-primary",
        )}
      >
        <Value value={value} />
      </span>
    </div>
  )
}

/**
 * 缓存写入在不同协议下的可得性。OpenAI 协议不区分缓存写入，只上报命中；
 * 这时显示「—」并说明原因，比显示 0 诚实——0 意味着「写过但是零」。
 */
function cacheWriteHint(platform: string): string {
  return platform === "codex"
    ? "当前记录未提供缓存创建数值；不根据缓存命中反推写入"
    : "来源未提供缓存写入字段"
}

export function UsageHero({
  platform,
  breakdown,
  status,
  error,
  costText,
  onRetry,
}: {
  platform: string
  breakdown: UsageBreakdown | null
  status: "loading" | "ready" | "failed"
  error?: string | null
  /** 估算费用文案，由调用方按现有费用口径格式化；无法估算时传 null */
  costText: string | null
  onRetry?: () => void
}) {
  if (status === "loading" || (status === "ready" && !breakdown)) {
    return (
      <div className="grid min-h-[136px] place-items-center rounded-card border bg-surface text-xs text-text-tertiary">
        统计读取中…
      </div>
    )
  }

  if (status === "failed" || !breakdown) {
    return (
      <div className="flex min-h-[136px] flex-col items-center justify-center gap-3 rounded-card border bg-surface px-6 text-xs text-danger">
        <span>分项统计查询失败：{error ?? "未知错误"}</span>
        {onRetry && (
          <button
            type="button"
            onClick={onRetry}
            className="rounded-sm2 border px-3 py-1.5 text-text-primary transition-colors hover:bg-hover"
          >
            重试
          </button>
        )}
      </div>
    )
  }

  const { real_total: realTotal, requests, cache_hit_rate: hitRate } = breakdown
  const values: Record<string, number | null> = {
    fresh_input: breakdown.fresh_input,
    output: breakdown.output,
    cache_write: breakdown.cache_write,
    cache_read: breakdown.cache_read,
  }
  // 百分比夹在 0–100：比率来自后端的除法，浮点误差不该让进度条溢出轨道
  const hitPercent = hitRate === null ? null : Math.min(100, Math.max(0, hitRate * 100))

  return (
    <div className="flex w-full flex-col gap-4 rounded-card border bg-surface p-5">
      {/* 上排：主数字 + 整体指标 */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-sm2 bg-success-soft">
            <Zap className="size-5 text-success-text" />
          </span>
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="text-[11px] text-text-secondary">真实消耗 Token</span>
            <span className="flex items-baseline gap-2">
              <span className="tnum font-mono text-[26px] font-semibold leading-none text-text-primary">
                <Value
                  value={realTotal === null ? null : grouped(realTotal)}
                  hint="区间内有记录缺少总量字段，合计无法确定"
                />
              </span>
              {realTotal !== null && (
                <span className="tnum rounded-sm2 bg-surface-3 px-1.5 py-0.5 font-mono text-[11px] text-text-secondary">
                  ≈ {compactTokens(realTotal)}
                </span>
              )}
            </span>
          </div>
        </div>

        <div className="flex items-center divide-x rounded-sm2 border bg-surface-2 px-3.5 py-2">
          <TopStat label="请求数" value={grouped(requests)} />
          <TopStat
            label="平均每次"
            value={
              breakdown.avg_per_request === null
                ? null
                : compactTokens(Math.round(breakdown.avg_per_request))
            }
          />
          <TopStat label="估算费用" value={costText} tone="cost" />
        </div>
      </div>

      {/* 下排：四个分项 + 命中率 */}
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        {TOKEN_SERIES.map((spec) => {
          const value = values[spec.key]
          return (
            <MiniStat
              key={spec.key}
              label={spec.label}
              colorVar={spec.colorVar}
              value={value === null ? null : compactTokens(value)}
              hint={
                spec.key === "cache_write"
                  ? cacheWriteHint(platform)
                  : undefined
              }
            />
          )
        })}

        <div className="col-span-2 flex flex-col justify-center gap-2 rounded-sm2 border bg-surface-2 p-3 lg:col-span-1">
          <span className="flex items-center justify-between gap-2 text-[11px]">
            <span className="text-text-secondary">缓存命中率</span>
            <span className="tnum font-mono font-semibold text-text-primary">
              <Value
                value={hitPercent === null ? null : `${hitPercent.toFixed(hitPercent >= 99.95 ? 0 : 1)}%`}
                hint="新增输入、缓存创建或缓存命中之一未知，比率无法计算"
              />
            </span>
          </span>
          {/* 比率未知时不画轨道：空轨道会被读成「命中率 0」 */}
          {hitPercent !== null && (
            <span className="block h-1.5 w-full overflow-hidden rounded-full bg-track">
              <span
                className="block h-full rounded-full transition-[width] duration-500"
                style={{ width: `${hitPercent}%`, background: "var(--chart-cache-read)" }}
              />
            </span>
          )}
        </div>
      </div>
    </div>
  )
}
