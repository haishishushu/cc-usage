import test from "node:test"
import assert from "node:assert/strict"
import { dockQuotaView } from "./dockQuotaView.ts"

const win = (key, usedPercent = 40) =>
  ({ key, windowName: key, usedPercent, usedText: null, resetCountdown: null, badgeTone: "purple" })

const base = {
  kind: "api",
  connected: true,
  quotaQueried: true,
  quotaLoading: false,
  quotaState: "ok",
  windows: null,
}

test("查到套餐窗口（5h/7d）就按真实水位画，API Key 也一样", () => {
  assert.deepEqual(dockQuotaView({ ...base, windows: [win("5h")] }), { type: "windows", windows: [win("5h")] })
  assert.deepEqual(dockQuotaView({ ...base, kind: "auth", windows: [win("7d")] }), { type: "windows", windows: [win("7d")] })
  // 订阅型分组返回 1d/7d/30d，含 7d 即视为有套餐
  assert.deepEqual(
    dockQuotaView({ ...base, windows: [win("1d"), win("7d"), win("30d")] }),
    { type: "windows", windows: [win("1d"), win("7d"), win("30d")] },
  )
})

test("API Key 查询成功但没有额度窗口 = 无套餐，画满格蓝（2026-09-19 鼠鼠定版）", () => {
  assert.deepEqual(dockQuotaView({ ...base, windows: [] }), { type: "unlimited" })
})

test("只返回总配额、月消费这类非套餐窗口，仍按无套餐画满格蓝（2026-09-19 放宽）", () => {
  assert.deepEqual(dockQuotaView({ ...base, windows: [win("quota")] }), { type: "unlimited" })
  assert.deepEqual(dockQuotaView({ ...base, windows: [win("30d"), win("1d")] }), { type: "unlimited" })
})

test("网关明确不提供额度窗口（unsupported）= 无套餐，画满格蓝", () => {
  assert.deepEqual(dockQuotaView({ ...base, quotaState: "unsupported" }), { type: "unlimited" })
})

test("官方 API Key 本就不发额度查询 = 无套餐，画满格蓝", () => {
  assert.deepEqual(dockQuotaView({ ...base, quotaQueried: false, quotaState: null }), { type: "unlimited" })
})

test("查询中、还没结果、临时失败、凭证失效都不得画满格，保持空轨道", () => {
  assert.deepEqual(dockQuotaView({ ...base, quotaLoading: true }), { type: "unknown" })
  assert.deepEqual(dockQuotaView({ ...base, quotaState: null }), { type: "unknown" })
  assert.deepEqual(dockQuotaView({ ...base, quotaState: "failed" }), { type: "unknown" })
  assert.deepEqual(dockQuotaView({ ...base, quotaState: "rate_limited" }), { type: "unknown" })
  assert.deepEqual(dockQuotaView({ ...base, quotaState: "unauthorized" }), { type: "unknown" })
  assert.deepEqual(dockQuotaView({ ...base, quotaState: "forbidden" }), { type: "unknown" })
})

test("官方订阅缺数据是「查不到」，保持空轨道，不伪装成不限量", () => {
  assert.deepEqual(dockQuotaView({ ...base, kind: "auth", quotaState: "unsupported" }), { type: "unknown" })
  assert.deepEqual(dockQuotaView({ ...base, kind: "auth", windows: [] }), { type: "unknown" })
})

test("未连接 / 已暂停不画满格", () => {
  assert.deepEqual(dockQuotaView({ ...base, connected: false }), { type: "unknown" })
})
