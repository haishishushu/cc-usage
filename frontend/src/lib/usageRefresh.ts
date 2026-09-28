import type { LiveUsageDto } from "./api"

/**
 * 流式输出期间每秒可能有好几条 live-usage 推送；统计视图一次重查要跑多条区间聚合，
 * 这里把连续变化合并：首个变化立即刷新，窗口内的后续变化在窗口结束时补一次。
 */
export const USAGE_REFRESH_THROTTLE_MS = 1500

export interface Throttled {
  (): void
  cancel: () => void
}

/** 首次立即执行；窗口内的再次调用只在窗口结束时补执行一次，保证最后一次变化不会丢。 */
export function throttleTrailing(
  run: () => void,
  wait: number,
  timers: { now: () => number; set: (fn: () => void, ms: number) => unknown; clear: (id: unknown) => void } = {
    now: () => Date.now(),
    set: (fn, ms) => window.setTimeout(fn, ms),
    clear: (id) => window.clearTimeout(id as number),
  },
): Throttled {
  let last = Number.NEGATIVE_INFINITY
  let pending: unknown = null
  const fire = () => {
    pending = null
    last = timers.now()
    run()
  }
  const call = (() => {
    if (pending !== null) return
    const left = last + wait - timers.now()
    if (left <= 0) fire()
    else pending = timers.set(fire, left)
  }) as Throttled
  call.cancel = () => {
    if (pending !== null) timers.clear(pending)
    pending = null
  }
  return call
}

/** 仅为续期运行中会话的心跳推送不代表统计有变化；旧版后端没有该字段时按有变化处理。 */
export function isUsageChange(usage: LiveUsageDto, platform: string): boolean {
  return usage.platform === platform && usage.data_changed !== false
}
