import { useId, useRef, useState } from "react"
import { cn } from "@/lib/utils"
import { compactTokens, grouped } from "@/lib/api"
import {
  areaPath,
  axisTicks,
  hitIndex,
  labelEvery,
  linePath,
  niceCeil,
  seriesMax,
  xAt,
  type Series,
} from "@/lib/chartGeometry"

/**
 * UsageAreaChart —— 多系列面积图，纯 SVG 手写，不引入图表库。
 *
 * 布局分两格、共享同一根 X 轴：上格是四条 Token 面积线，下格是估算成本。
 * 刻意**不做双轴**——把两种量纲叠在同一绘图区里，两条线的交叉点由各自的缩放比例
 * 决定，看起来像是"成本在这里超过了缓存"，实际上不表示任何真实关系。
 * 拆成两格后，各自的纵轴含义明确，横向仍然对齐可比。
 *
 * 几何计算全在 lib/chartGeometry.ts（纯函数、带单测），这里只负责渲染与交互。
 */

const TOKEN_PLOT_HEIGHT = 176
const COST_PLOT_HEIGHT = 56
const Y_AXIS_WIDTH = 52
/** 两格之间的空档：量纲不同，刻度贴太近会被读成同一根轴上的连续刻度 */
const COST_GAP = 22

/** 费用文案：极小额显示为 <$0.0001，避免四舍五入成 $0.0000 让人以为免费 */
function costText(value: number): string {
  if (value > 0 && value < 0.0001) return "<$0.0001"
  return `$${value.toFixed(value >= 1 ? 2 : 4)}`
}

export function UsageAreaChart({
  labels,
  series,
  cost,
  costLabel = "估算成本",
}: {
  labels: string[]
  series: Series[]
  cost: Array<number | null>
  costLabel?: string
}) {
  const gradientId = useId()
  const plotRef = useRef<HTMLDivElement>(null)
  const [hover, setHover] = useState<number | null>(null)
  // 被隐藏的系列。缓存命中常比其余三条大一个数量级，全开时那三条会压在底边成一条直线；
  // 关掉主导系列后纵轴按剩下的重新缩放，小量级的变化才看得出来。
  const [hidden, setHidden] = useState<Set<string>>(() => new Set())
  // 绘图区宽度靠布局决定，SVG 用 viewBox 缩放；这里的 1000 只是坐标系单位
  const VIEW_WIDTH = 1000

  const toggle = (key: string) =>
    setHidden((current) => {
      const next = new Set(current)
      // 留最后一条：全部关掉只剩一个空画布，不是有用的状态
      if (next.has(key)) next.delete(key)
      else if (next.size < series.length - 1) next.add(key)
      return next
    })

  const visible = series.filter((item) => !hidden.has(item.key))
  const tokenTop = niceCeil(seriesMax(visible))
  const tokenTicks = axisTicks(tokenTop)
  const costValues = cost.filter((value): value is number => value !== null)
  const costTop = niceCeil(costValues.length ? Math.max(...costValues) : 0)
  const every = labelEvery(labels.length)

  const tokenPlot = { width: VIEW_WIDTH, height: TOKEN_PLOT_HEIGHT }
  const costPlot = { width: VIEW_WIDTH, height: COST_PLOT_HEIGHT }

  const track = (event: React.MouseEvent<HTMLDivElement>) => {
    const box = plotRef.current?.getBoundingClientRect()
    if (!box || box.width === 0) return
    setHover(hitIndex(event.clientX - box.left, labels.length, box.width))
  }

  const hovered = hover !== null && hover >= 0 && hover < labels.length ? hover : null
  const hoverX = hovered === null ? 0 : xAt(hovered, labels.length, VIEW_WIDTH)
  // tooltip 贴边时翻转到光标另一侧，避免被卡片裁掉
  const hoverRatio = hovered === null ? 0 : hoverX / VIEW_WIDTH

  return (
    <div className="flex w-full flex-col gap-3 rounded-card border bg-surface p-5">
      {/* 图例：颜色不是唯一通道，每条系列都有文字标签；点击可暂时隐藏该条 */}
      <div className="flex flex-wrap items-center gap-x-1 gap-y-1">
        {series.map((item) => {
          const off = hidden.has(item.key)
          return (
            <button
              key={item.key}
              type="button"
              onClick={() => toggle(item.key)}
              aria-pressed={!off}
              title={off ? `显示${item.label}` : `隐藏${item.label}，纵轴会按剩余系列重新缩放`}
              className={cn(
                "flex items-center gap-1.5 rounded-sm2 px-2 py-1 text-[11px] transition-colors hover:bg-hover",
                off ? "text-text-tertiary" : "text-text-secondary",
              )}
            >
              <span
                aria-hidden
                className="size-2 rounded-full transition-opacity"
                style={{ background: `var(${item.colorVar})`, opacity: off ? 0.25 : 1 }}
              />
              <span className={off ? "line-through" : undefined}>{item.label}</span>
            </button>
          )
        })}
        <span className="ml-1 flex items-center gap-1.5 px-2 py-1 text-[11px] text-text-secondary">
          <span
            aria-hidden
            className="h-0 w-4 border-t-2 border-dashed"
            style={{ borderColor: "var(--chart-cost)" }}
          />
          {costLabel}
          <span className="text-text-tertiary">（下格）</span>
        </span>
      </div>

      <div
        className="relative flex w-full gap-3"
        onMouseLeave={() => setHover(null)}
      >
        {/* 纵轴刻度列：两格各自一套，靠 flex 高度对齐绘图区，不手工算像素位置 */}
        <div className="flex shrink-0 flex-col" style={{ width: Y_AXIS_WIDTH }}>
          <div
            className="flex flex-col items-end justify-between"
            style={{ height: TOKEN_PLOT_HEIGHT }}
          >
            {tokenTicks.map((tick) => (
              <span key={tick} className="tnum font-mono text-[10px] leading-none text-text-tertiary">
                {compactTokens(Math.round(tick))}
              </span>
            ))}
          </div>
          {/* 与上格之间留出明显的空档：两格量纲不同，刻度贴在一起会被读成一列连续刻度 */}
          <div
            className="flex flex-col items-end justify-between"
            style={{ height: COST_PLOT_HEIGHT, marginTop: COST_GAP }}
          >
            <span className="tnum font-mono text-[10px] leading-none text-text-tertiary">
              {costTop > 0 ? costText(costTop) : "$0"}
            </span>
            <span className="tnum font-mono text-[10px] leading-none text-text-tertiary">$0</span>
          </div>
        </div>

        <div className="min-w-0 flex-1">
          <div ref={plotRef} className="relative w-full" onMouseMove={track}>
            {/* 上格：Token 四系列 */}
            <svg
              viewBox={`0 0 ${VIEW_WIDTH} ${TOKEN_PLOT_HEIGHT}`}
              preserveAspectRatio="none"
              className="block w-full"
              style={{ height: TOKEN_PLOT_HEIGHT }}
              role="img"
              aria-label={`Token 趋势：${visible.map((s) => s.label).join("、")}`}
            >
              <defs>
                {visible.map((item) => (
                  <linearGradient
                    key={item.key}
                    id={`${gradientId}-${item.key}`}
                    x1="0" y1="0" x2="0" y2="1"
                  >
                    <stop offset="0%" stopColor={`var(${item.colorVar})`} stopOpacity={0.18} />
                    <stop offset="100%" stopColor={`var(${item.colorVar})`} stopOpacity={0} />
                  </linearGradient>
                ))}
              </defs>

              {/* 网格线退让在数据之后绘制会盖住面积，所以先画 */}
              {tokenTicks.map((tick) => {
                const y = tokenTop <= 0 ? TOKEN_PLOT_HEIGHT : TOKEN_PLOT_HEIGHT - (tick / tokenTop) * TOKEN_PLOT_HEIGHT
                return (
                  <line
                    key={tick}
                    x1={0} x2={VIEW_WIDTH} y1={y} y2={y}
                    stroke="var(--chart-grid)"
                    strokeWidth={1}
                    vectorEffect="non-scaling-stroke"
                  />
                )
              })}

              {visible.map((item) => (
                <path
                  key={`${item.key}-area`}
                  d={areaPath(item.values, tokenTop, tokenPlot)}
                  fill={`url(#${gradientId}-${item.key})`}
                />
              ))}
              {visible.map((item) => (
                <path
                  key={`${item.key}-line`}
                  d={linePath(item.values, tokenTop, tokenPlot)}
                  fill="none"
                  stroke={`var(${item.colorVar})`}
                  strokeWidth={2}
                  strokeLinejoin="round"
                  strokeLinecap="round"
                  // 横向拉伸时描边不跟着变粗变细
                  vectorEffect="non-scaling-stroke"
                />
              ))}

              {hovered !== null && (
                <line
                  x1={hoverX} x2={hoverX} y1={0} y2={TOKEN_PLOT_HEIGHT}
                  stroke="var(--border-strong)"
                  strokeWidth={1}
                  vectorEffect="non-scaling-stroke"
                />
              )}
            </svg>

            {/* 下格：估算成本。独立纵轴，横轴与上格逐点对齐 */}
            <div style={{ height: COST_GAP }} />
            <svg
              viewBox={`0 0 ${VIEW_WIDTH} ${COST_PLOT_HEIGHT}`}
              preserveAspectRatio="none"
              className="block w-full"
              style={{ height: COST_PLOT_HEIGHT }}
              role="img"
              aria-label="按时间桶估算的费用趋势"
            >
              <line
                x1={0} x2={VIEW_WIDTH} y1={COST_PLOT_HEIGHT} y2={COST_PLOT_HEIGHT}
                stroke="var(--chart-grid)"
                strokeWidth={1}
                vectorEffect="non-scaling-stroke"
              />
              <path
                d={areaPath(cost, costTop, costPlot)}
                fill="var(--chart-cost)"
                fillOpacity={0.1}
              />
              <path
                d={linePath(cost, costTop, costPlot)}
                fill="none"
                stroke="var(--chart-cost)"
                strokeWidth={2}
                strokeDasharray="4 4"
                strokeLinejoin="round"
                vectorEffect="non-scaling-stroke"
              />
              {hovered !== null && (
                <line
                  x1={hoverX} x2={hoverX} y1={0} y2={COST_PLOT_HEIGHT}
                  stroke="var(--border-strong)"
                  strokeWidth={1}
                  vectorEffect="non-scaling-stroke"
                />
              )}
            </svg>

            {/* X 轴刻度：两格共享一根时间轴，所以放在最底下而不是夹在中间 */}
            <div className="flex pt-1.5">
              {labels.map((label, index) => (
                <span
                  key={`${label}-${index}`}
                  className="tnum min-w-0 flex-1 text-center font-mono text-[9px] leading-none text-text-tertiary"
                >
                  {index % every === 0 ? label : ""}
                </span>
              ))}
            </div>

            {hovered !== null && (
              <div
                className="pointer-events-none absolute top-2 z-10 min-w-[168px] rounded-sm2 border bg-surface p-2.5 shadow-[0_6px_20px_var(--shadow-color)]"
                style={
                  hoverRatio > 0.6
                    ? { right: `${(1 - hoverRatio) * 100 + 1.5}%` }
                    : { left: `${hoverRatio * 100 + 1.5}%` }
                }
              >
                <p className="mb-1.5 text-[11px] font-semibold text-text-primary">{labels[hovered]}</p>
                {visible.map((item) => {
                  const value = item.values[hovered]
                  return (
                    <p key={item.key} className="flex items-center gap-1.5 text-[11px] leading-5">
                      <span
                        aria-hidden
                        className="size-2 shrink-0 rounded-full"
                        style={{ background: `var(${item.colorVar})` }}
                      />
                      <span className="text-text-secondary">{item.label}</span>
                      <span className="tnum ml-auto font-mono text-text-primary">
                        {value === null ? "—" : grouped(value)}
                      </span>
                    </p>
                  )
                })}
                <p className="mt-1 flex items-center gap-1.5 border-t pt-1 text-[11px] leading-5">
                  <span
                    aria-hidden
                    className="h-0 w-3 shrink-0 border-t-2 border-dashed"
                    style={{ borderColor: "var(--chart-cost)" }}
                  />
                  <span className="text-text-secondary">{costLabel}</span>
                  <span className="tnum ml-auto font-mono text-warn">
                    {cost[hovered] === null ? "—" : costText(cost[hovered] as number)}
                  </span>
                </p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
