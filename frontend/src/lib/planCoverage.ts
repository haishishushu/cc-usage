/**
 * 判断当前连接是不是套餐制。
 *
 * 口径：额度接口返回了 5 小时或周窗口的，就是套餐（官方订阅、sub2api 网关、
 * 各家 Coding Plan 都走这个形态）；查不到这两个窗口的按 API Key 按量计费处理。
 *
 * 套餐制下单条请求不单独扣费，本地按公开价目表乘 Token 推算出的金额不是真实支出，
 * 因此请求日志的成本列显示「套餐内」，把真实的额度消耗放在悬停提示里，不编造金额。
 */
export type PlanQuotaWindow = {
  key: string
  window_name: string
  /** 已用 / 总量的文字描述，来源没给分母时为 null */
  amount_text: string | null
  used_percent: number | null
}

/** 只有 5h 与 7d 代表「5 小时 / 周额度」；月额度、积分额度是别的形态，不算 */
const PLAN_WINDOW_KEYS = ["5h", "7d"]

/** 参数放宽为只读 key：额度窗口在 DTO、后端映射、界面类型里字段名不同，套餐判定只认 key */
export function isPlanCovered(windows: readonly { key: string }[] | null | undefined): boolean {
  return Boolean(windows?.some((window) => PLAN_WINDOW_KEYS.includes(window.key)))
}

/** 悬停提示：原样透传来源给的额度文案，没有分母就退回百分比，都没有就只列窗口名 */
export function planCoverageHint(windows: readonly PlanQuotaWindow[] | null | undefined): string | null {
  const parts = (windows ?? [])
    .filter((window) => PLAN_WINDOW_KEYS.includes(window.key))
    .map((window) => {
      if (window.amount_text) return `${window.window_name} ${window.amount_text}`
      if (window.used_percent !== null) return `${window.window_name} 已用 ${window.used_percent}%`
      return window.window_name
    })
  return parts.length ? parts.join(" · ") : null
}
