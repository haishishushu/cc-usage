import assert from "node:assert/strict"
import test from "node:test"
import {
  SPY_LOCK_MAX_MS,
  activeSection,
  createSpyLock,
  spyLockExpired,
} from "./scrollSpyLock.ts"

test("没有锁时，高亮完全交给滚动推导", () => {
  assert.equal(activeSection(null, "appearance", 1000), "appearance")
  assert.equal(spyLockExpired(null, 1000), true)
})

test("锁定期间高亮钉在点击的目标上，忽略滚动推导", () => {
  const lock = createSpyLock("data", 1000)
  // 平滑滚动途中会连续扫过中间分区，推导值一路在变
  assert.equal(activeSection(lock, "connections", 1050), "data")
  assert.equal(activeSection(lock, "island", 1200), "data")
  assert.equal(activeSection(lock, "appearance", 1900), "data")
})

test("超过硬上限后自动交还滚动推导，不会永久钉死", () => {
  const lock = createSpyLock("data", 1000)
  const justBefore = 1000 + SPY_LOCK_MAX_MS - 1
  assert.equal(spyLockExpired(lock, justBefore), false)
  assert.equal(activeSection(lock, "appearance", justBefore), "data")

  const atLimit = 1000 + SPY_LOCK_MAX_MS
  assert.equal(spyLockExpired(lock, atLimit), true)
  assert.equal(activeSection(lock, "appearance", atLimit), "appearance")
})

test("硬上限是兜底：滚动事件一直不来时也能自己解开", () => {
  // 点击已经停在顶部的分区时，smooth 滚动不产生任何 scroll 事件，
  // 静默解锁的计时器是主路径，这里断言上限存在且是有限值
  assert.ok(Number.isFinite(SPY_LOCK_MAX_MS))
  assert.ok(SPY_LOCK_MAX_MS > 0)
})

test("锁的目标就是点击的分区，时间戳按传入的时刻记", () => {
  const lock = createSpyLock("proxy", 5000)
  assert.equal(lock.target, "proxy")
  assert.equal(lock.expiresAt, 5000 + SPY_LOCK_MAX_MS)
})

test("重新点击会换成新的目标与新的到期时刻", () => {
  const first = createSpyLock("data", 1000)
  const second = createSpyLock("general", 1500)
  assert.equal(activeSection(second, "data", 1600), "general")
  assert.ok(second.expiresAt > first.expiresAt)
})
