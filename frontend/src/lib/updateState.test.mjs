import assert from "node:assert/strict"
import test from "node:test"
import {
  DISMISSED_KEY,
  clearDismissedVersion,
  formatBytes,
  isUpdateRelevant,
  progressPercent,
  readDismissedVersion,
  writeDismissedVersion,
} from "./updateState.ts"

/** 最小 localStorage 桩：只实现更新状态用到的三个方法 */
function stubStorage() {
  const map = new Map()
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
  }
}

test("isUpdateRelevant：没有可用更新时绿灯不亮", () => {
  assert.equal(isUpdateRelevant(null, null), false)
})

test("isUpdateRelevant：有新版本且未忽略时亮灯", () => {
  assert.equal(isUpdateRelevant("0.2.0", null), true)
})

test("isUpdateRelevant：忽略当前版本后同版本不再亮灯", () => {
  assert.equal(isUpdateRelevant("0.2.0", "0.2.0"), false)
})

test("isUpdateRelevant：忽略的是旧版本时，更新的新版本仍然亮灯", () => {
  assert.equal(isUpdateRelevant("0.3.0", "0.2.0"), true)
})

test("忽略版本写入后可读出，清除后为 null", () => {
  const storage = stubStorage()
  assert.equal(readDismissedVersion(storage), null, "初始无记录")
  writeDismissedVersion("0.2.0", storage)
  assert.equal(readDismissedVersion(storage), "0.2.0")
  assert.equal(storage.getItem(DISMISSED_KEY), "0.2.0", "落在约定键上")
  clearDismissedVersion(storage)
  assert.equal(readDismissedVersion(storage), null)
})

test("存储异常时读写静默降级，不抛错", () => {
  const broken = {
    getItem: () => { throw new Error("quota") },
    setItem: () => { throw new Error("quota") },
    removeItem: () => {},
  }
  assert.equal(readDismissedVersion(broken), null)
  assert.doesNotThrow(() => writeDismissedVersion("0.2.0", broken))
})

test("progressPercent：按已下载/总量取整为 0–100", () => {
  assert.equal(progressPercent(0, 18.2 * 1024 * 1024), 0)
  assert.equal(progressPercent(18.2 * 1024 * 1024, 18.2 * 1024 * 1024), 100)
  assert.equal(progressPercent(12.4 * 1024 * 1024, 18.2 * 1024 * 1024), 68)
})

test("progressPercent：总量未知返回 null；超出按 100 封顶", () => {
  assert.equal(progressPercent(1024, null), null)
  assert.equal(progressPercent(99, 50), 100)
})

test("formatBytes：按量级选单位，保留一位小数", () => {
  assert.equal(formatBytes(0), "0 B")
  assert.equal(formatBytes(512), "512 B")
  assert.equal(formatBytes(1024), "1.0 KB")
  assert.equal(formatBytes(12.4 * 1024 * 1024), "12.4 MB")
  assert.equal(formatBytes(2.5 * 1024 * 1024 * 1024), "2.5 GB")
})
