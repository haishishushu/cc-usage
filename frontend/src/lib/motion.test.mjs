import assert from "node:assert/strict"
import test from "node:test"
import { cycleExitDelay, SHIMMER_CYCLE_MS, SHIMMER_MIN_CYCLES } from "./motion.ts"

test("刷新柔光补齐到轮次边界，秒回时也能完整掠一轮", () => {
  // 起跑即要停（缓存命中）：补满整轮，反馈不会一闪而过
  assert.equal(cycleExitDelay(0, 1100), 1100)
  // 轮内途中停：只补到本轮末尾，不重新起一轮
  assert.equal(cycleExitDelay(500, 1100), 600)
  // 跨轮后停：按当前所处的那一轮补齐，不累计历史轮次
  assert.equal(cycleExitDelay(1500, 1100), 700)
  assert.equal(cycleExitDelay(3000, 1100), 300)
})

test("最少轮数是下限：不足则等满，超过则按边界收", () => {
  // 秒回：等满 2 轮，而不是掠一轮就收
  assert.equal(cycleExitDelay(0, 1100, 2), 2200)
  // 第 1 轮途中查完：仍要等满 2 轮
  assert.equal(cycleExitDelay(500, 1100, 2), 1700)
  // 第 2 轮途中查完：补到第 2 轮末尾，正好满足下限
  assert.equal(cycleExitDelay(1500, 1100, 2), 700)
  // 已超过下限：只补到当前轮边界，不再多掠
  assert.equal(cycleExitDelay(2500, 1100, 2), 800)
  assert.equal(cycleExitDelay(9000, 1100, 2), 900)
})

test("收尾时刻必然落在轮次边界上，柔光才不会被拦腰摘除", () => {
  for (const elapsed of [0, 1, 250, 900, 1099, 1100, 2500, 5000, 12345]) {
    for (const minCycles of [1, 2, 3]) {
      const stopAt = elapsed + cycleExitDelay(elapsed, SHIMMER_CYCLE_MS, minCycles)
      const offset = stopAt % SHIMMER_CYCLE_MS
      assert.ok(
        offset < 1e-9 || Math.abs(offset - SHIMMER_CYCLE_MS) < 1e-9,
        `elapsed=${elapsed} min=${minCycles} stopAt=${stopAt} offset=${offset}`,
      )
      assert.ok(stopAt >= SHIMMER_CYCLE_MS * minCycles - 1e-9, `elapsed=${elapsed} min=${minCycles}`)
    }
  }
})

test("任何输入都给出正的等待时长，不会立刻摘除或永不摘除", () => {
  for (const elapsed of [0, 1, 250, 1099, 1100, 5000, 12345]) {
    for (const cycle of [SHIMMER_CYCLE_MS, 1600]) {
      const delay = cycleExitDelay(elapsed, cycle, SHIMMER_MIN_CYCLES)
      assert.ok(delay > 0, `elapsed=${elapsed} cycle=${cycle} delay=${delay}`)
      assert.ok(delay <= cycle * SHIMMER_MIN_CYCLES, `elapsed=${elapsed} cycle=${cycle} delay=${delay}`)
    }
  }
})

test("柔光节奏与 index.css 的 island-refresh-shimmer 保持同一组数值", () => {
  // 卡片与停靠条共用同一周期，两种形态节奏一致
  assert.equal(SHIMMER_CYCLE_MS, 1100)
  assert.equal(SHIMMER_MIN_CYCLES, 2)
})
