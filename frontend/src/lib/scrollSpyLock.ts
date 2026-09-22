/**
 * 设置分区导航的「点击后高亮锁」。
 *
 * 导航高亮平时由滚动位置推导。改成平滑滚动后，滚动途中会连续扫过中间的每一个
 * 分区，高亮便会一路闪着追过去——比原来的瞬间跳转更难看。
 *
 * 所以点击时先把高亮钉在目标分区上，等滚动停下来再交还给推导。
 * 这里只管「现在该听谁的」这一件事；解锁的时机编排（滚动静默）留在组件里。
 */

export type SpyLock = { target: string; expiresAt: number } | null

/**
 * 锁的硬上限。
 *
 * 正常解锁靠滚动静默，但有一种情况滚动事件根本不会来：点击的分区已经停在顶部，
 * 平滑滚动无事发生。那时没有任何事件能触发解锁，高亮就会永远钉死——这个上限是
 * 兜底。取 1.2 秒：长于最远一次平滑滚动，短到用户察觉不出高亮"迟钝"。
 */
export const SPY_LOCK_MAX_MS = 1200

export function createSpyLock(target: string, now: number): NonNullable<SpyLock> {
  return { target, expiresAt: now + SPY_LOCK_MAX_MS }
}

export function spyLockExpired(lock: SpyLock, now: number): boolean {
  return lock === null || now >= lock.expiresAt
}

/** 锁还在就听锁的，否则听滚动推导的 */
export function activeSection(lock: SpyLock, derived: string, now: number): string {
  return spyLockExpired(lock, now) ? derived : lock!.target
}
