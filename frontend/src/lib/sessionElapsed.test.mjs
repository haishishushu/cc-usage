import test from "node:test"
import assert from "node:assert/strict"
import { sessionElapsed } from "./sessionElapsed.ts"

test("本轮耗时按起点计算，跨小时且新一轮重新计时", () => {
  assert.equal(sessionElapsed(1000, 62000), "1分01秒")
  assert.equal(sessionElapsed(1000, 3662000), "1时01分01秒")
  assert.equal(sessionElapsed(62000, 62000), "0分00秒")
})

test("未知起点不编造耗时，时钟回退不显示负数", () => {
  assert.equal(sessionElapsed(undefined, 62000), "耗时未知")
  assert.equal(sessionElapsed(63000, 62000), "0分00秒")
})
