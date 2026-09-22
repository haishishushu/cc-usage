import test from "node:test"
import assert from "node:assert/strict"
import { isPlanCovered, planCoverageHint } from "./planCoverage.ts"

const w = (key, windowName, amount_text = null, used_percent = null) =>
  ({ key, window_name: windowName, amount_text, used_percent, resets_at: null })

test("查到 5 小时或周额度窗口的就是套餐", () => {
  assert.equal(isPlanCovered([w("5h", "5 小时额度")]), true)
  assert.equal(isPlanCovered([w("7d", "周额度")]), true)
  assert.equal(isPlanCovered([w("5h", "5 小时额度"), w("7d", "周额度")]), true)
})

test("没有额度窗口的按 API Key 处理，照常显示估算金额", () => {
  assert.equal(isPlanCovered([]), false)
  assert.equal(isPlanCovered(null), false)
  assert.equal(isPlanCovered(undefined), false)
})

test("只有月额度或积分额度不算 5 小时 / 周套餐", () => {
  // 额度查询成功但窗口形态不同，不能一律当成套餐
  assert.equal(isPlanCovered([w("30d", "月额度")]), false)
  assert.equal(isPlanCovered([w("credits", "Grok 积分额度")]), false)
})

test("悬停提示原样透传来源给的额度文案", () => {
  const hint = planCoverageHint([
    w("5h", "5 小时额度", "12.4 / 20.0", 62),
    w("7d", "周额度", "31 / 180", 17.2),
  ])
  assert.equal(hint, "5 小时额度 12.4 / 20.0 · 周额度 31 / 180")
})

test("来源没给分母时退回百分比，两者都没有就只列窗口名", () => {
  assert.equal(planCoverageHint([w("5h", "5 小时额度", null, 62)]), "5 小时额度 已用 62%")
  assert.equal(planCoverageHint([w("5h", "5 小时额度")]), "5 小时额度")
})

test("没有窗口就没有提示", () => {
  assert.equal(planCoverageHint([]), null)
  assert.equal(planCoverageHint(null), null)
})
