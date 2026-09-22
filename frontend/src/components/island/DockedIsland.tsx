import { cn } from "@/lib/utils"
import { SHIMMER_CYCLE_MS, SHIMMER_MIN_CYCLES, useCycleExit } from "@/lib/motion"
import { quotaBarClass } from "@/lib/quota"
import type { DockEdge, QuotaWindow } from "@/types"

/**
 * DockedIsland —— 贴边停靠条（§2.1.3 / 画布 17）
 *
 * 上/下 120×14，左/右 14×120；轨道 3px、间距 2px、沿边内边距 8px。
 * 贴屏幕那一侧为直角，露出侧为 7px 圆角。
 * 只保留水位颜色一个信号：不显示平台名、百分比、倒计时与 Token 增量文字。
 * 上下横条水位自左向右生长，左右竖条自下而上生长。
 *
 * 三种形态（dockQuotaView 判定）：
 * - 真实窗口按水位画；
 * - `unlimited`：API Key 无套餐，两条满格蓝色轨道 = 不设套餐上限（2026-09-19 鼠鼠定版）；
 * - 未连接 / 查询失败只留空轨道，不着色不画满格，与真实耗尽（红满格）区分。
 */
const LONG = 120
const SHORT = 14
const INNER = 104
const BAR = 3

const RADIUS: Record<DockEdge, string> = {
  top: "rounded-b-[7px]",
  bottom: "rounded-t-[7px]",
  left: "rounded-r-[7px]",
  right: "rounded-l-[7px]",
}

export function DockedIsland({
  edge,
  quotas,
  /** API Key 无套餐：忽略 quotas，画两条满格蓝色轨道，表示无套餐上限 */
  unlimited = false,
  /** 未连接 / 查询失败 / 来源不可用：只保留空轨道，不着色也不画成满格 */
  unavailable,
  pulse,
  pulseKey = 0,
  refreshing = false,
  refreshKey = 0,
  className,
}: {
  edge: DockEdge
  quotas: QuotaWindow[]
  unlimited?: boolean
  unavailable?: boolean
  /** 收到真实新增 Token 时短暂提高水位亮度。 */
  pulse?: boolean
  pulseKey?: number
  /** 刷新进行中：整条亮度持续脉冲，刷新结束补齐当前轮再回位。 */
  refreshing?: boolean
  /** 刷新序号：变化即起跑，不依赖 `refreshing` 是否被观察到为真 */
  refreshKey?: number
  className?: string
}) {
  const horizontal = edge === "top" || edge === "bottom"
  // 与卡片同一道柔光、同一套收尾规则：轮次边界处柔光正好在条外，摘除毫无痕迹。
  // 原先的整条提亮在「无额度数据」时几乎不可见——白底加浅灰空轨没有提亮空间。
  const shimmering = useCycleExit(refreshKey, refreshing, SHIMMER_CYCLE_MS, SHIMMER_MIN_CYCLES)

  return (
    <div
      className={cn(
        "relative flex select-none items-center justify-center border bg-surface shadow-island",
        horizontal ? "flex-col" : "flex-row",
        RADIUS[edge],
        className,
      )}
      style={{
        width: horizontal ? LONG : SHORT,
        height: horizontal ? SHORT : LONG,
        gap: 2,
        padding: horizontal ? "3px 8px" : "8px 3px",
      }}
      title="双击解除停靠 · 悬停探出"
    >
      {shimmering && (
        <span aria-hidden className="island-refresh-veil">
          <span className="island-refresh-shimmer" />
        </span>
      )}
      {(unlimited ? [null, null] : quotas).map((q) => {
        // unlimited：两条满格蓝色；真实窗口按水位着色；不可用强制 0（空轨道）
        const pct = unlimited ? 100 : unavailable ? 0 : (q!.usedPercent ?? 0)
        const size = Math.round((INNER * pct) / 100)
        return (
          <div
            key={`${q?.key ?? "unlimited"}:${pulseKey}`}
            className={cn(
              "overflow-hidden rounded-[1.5px] bg-track",
              horizontal ? "flex flex-row" : "flex flex-col justify-end",
            )}
            style={{
              width: horizontal ? INNER : BAR,
              height: horizontal ? BAR : INNER,
            }}
          >
            {pct > 0 && (
              <div
                className={cn(
                  "rounded-[1.5px]",
                  unlimited ? "bg-accent-blue" : quotaBarClass(pct),
                  pulse && "dock-token-pulse",
                )}
                style={{
                  width: horizontal ? size : BAR,
                  height: horizontal ? BAR : size,
                }}
              />
            )}
          </div>
        )
      })}
    </div>
  )
}
