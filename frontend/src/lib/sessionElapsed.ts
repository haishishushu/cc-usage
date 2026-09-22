/** 仅用本轮任务开始时间计算耗时，不用日志更新时间代替。 */
export function sessionElapsed(startedAtMs: number | undefined, now: number): string {
  if (startedAtMs == null || !Number.isFinite(startedAtMs)) return "耗时未知"
  const seconds = Math.max(0, Math.floor((now - startedAtMs) / 1000))
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor(seconds % 3600 / 60)
  const remainder = seconds % 60
  return hours > 0
    ? `${hours}时${String(minutes).padStart(2, "0")}分${String(remainder).padStart(2, "0")}秒`
    : `${minutes}分${String(remainder).padStart(2, "0")}秒`
}
