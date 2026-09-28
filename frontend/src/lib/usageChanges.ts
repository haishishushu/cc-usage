import { listenEvent, type LiveUsageDto } from "./api"
import { isUsageChange, throttleTrailing, USAGE_REFRESH_THROTTLE_MS } from "./usageRefresh"

/**
 * 订阅某平台统计数据的变化，按 [`USAGE_REFRESH_THROTTLE_MS`] 合并连续变化。
 * 返回的取消函数同时清掉尚未执行的补刷新。
 */
export function onUsageChanged(platform: string, refresh: () => void, wait = USAGE_REFRESH_THROTTLE_MS): () => void {
  const throttled = throttleTrailing(refresh, wait)
  let stopped = false
  let off: (() => void) | null = null
  void listenEvent<LiveUsageDto>("live-usage", (usage) => {
    if (!stopped && isUsageChange(usage, platform)) throttled()
  }).then((unlisten) => {
    if (stopped) unlisten()
    else off = unlisten
  }).catch(() => {})
  return () => {
    stopped = true
    throttled.cancel()
    off?.()
  }
}
