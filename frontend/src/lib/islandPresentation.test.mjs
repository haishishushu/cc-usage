import test from "node:test"
import assert from "node:assert/strict"
import { balanceSnapshotTime, islandActiveConnection, islandConnectionStatus, islandCost, connectionQueries } from "./islandPresentation.ts"

const conn = (id, platformId, status) =>
  ({ id, platformId, kind: "api", name: id, baseName: id, label: "API Key", masked: null, status, lastSyncText: "", baseUrl: null })

test("未选择连接时自动启用当前平台第一个连接成功的（2026-09-19 鼠鼠定版）", () => {
  const list = [
    conn("a", "claude", "connected"),
    conn("b", "claude", "connected"),
    conn("c", "codex", "connected"),
    conn("d", "claude", "paused"),
  ]
  assert.equal(islandActiveConnection(null, list, "claude")?.id, "a")
  // 其他平台连接成功也不顶上
  assert.equal(islandActiveConnection(null, [conn("c", "codex", "connected")], "claude"), null)
  // 没有连接成功的不编造
  assert.equal(islandActiveConnection(null, [conn("d", "claude", "paused")], "claude"), null)
  assert.equal(islandActiveConnection(null, [], "claude"), null)
})

test("明确选择的连接优先，已暂停也不被自动顶掉", () => {
  const list = [conn("a", "claude", "connected"), conn("b", "claude", "paused")]
  assert.equal(islandActiveConnection("b", list, "claude")?.id, "b")
  // 所选连接被删除：照旧「所选连接不可用」，不自动换号
  assert.equal(islandActiveConnection("missing", list, "claude"), null)
})

test("四种接入方式按能力查询，断开和未选择不偷用本机账号", () => {
  for (const platformId of ["claude", "codex"]) {
    const base = { id: platformId, platformId, status: "connected" };
    assert.deepEqual(connectionQueries({ ...base, kind: "auth" }), { quota: platformId, balance: null, usage: null });
    assert.deepEqual(connectionQueries({ ...base, kind: "api" }), { quota: null, balance: null, usage: platformId });
    assert.deepEqual(connectionQueries({ ...base, kind: "api", baseUrl: "https://gateway.test" }), { quota: platformId, balance: platformId, usage: null });
    assert.deepEqual(connectionQueries({ ...base, kind: "auth", status: "paused" }), { quota: null, balance: null, usage: null });
  }
  assert.deepEqual(connectionQueries(null), { quota: null, balance: null, usage: null });
});

test("无效与离线连接不能显示已连接，删除的选择不自动换号", () => {
  assert.deepEqual(islandConnectionStatus("invalid", false, true), { tone: "danger", label: "凭证无效" })
  assert.deepEqual(islandConnectionStatus("offline", false, true), { tone: "warn", label: "离线" })
  assert.deepEqual(islandConnectionStatus("paused", false, true), { tone: "warn", label: "已断开" })
  assert.equal(islandConnectionStatus("expired", false, true).label, "凭证已过期")
  assert.equal(islandConnectionStatus(null, false, true).label, "所选连接不可用")
  assert.equal(islandConnectionStatus(null, false, false).label, "未选择连接")
  assert.equal(islandConnectionStatus(null, true, false).label, "已连接")
})

test("实际费用优先于估算，真实零费用不会被估算覆盖", () => {
  assert.deepEqual(islandCost(0, { amount: 4, complete: true }), { cost: "$0.00", costEstimated: false })
  assert.deepEqual(islandCost(1.25, { amount: 4, complete: true }), { cost: "$1.25", costEstimated: false })
})

test("非零微小费用保留有效数值，不显示成零", () => {
  assert.equal(islandCost(0.000001, null).cost, "<$0.0001")
  assert.equal(islandCost(0.003, null).cost, "$0.0030")
  assert.deepEqual(islandCost(null, { amount: 0.000001, complete: true }), { cost: "≈ <$0.0001", costEstimated: true })
})

test("未知或不完整估算不编造金额", () => {
  assert.deepEqual(islandCost(null, { amount: 2, complete: false }), { cost: null, costEstimated: false })
  assert.equal(islandCost(Number.NaN, null).cost, null)
  assert.equal(islandCost(null, { amount: Number.POSITIVE_INFINITY, complete: true }).cost, null)
})

test("余额快照标注获取时刻，没有时刻不编造", () => {
  assert.equal(balanceSnapshotTime(null), null)
  assert.equal(balanceSnapshotTime(undefined), null)
  const text = balanceSnapshotTime(new Date(2026, 8, 18, 14, 32, 5))
  assert.ok(text.startsWith("· "), text)
  assert.ok(text.endsWith(" 获取"), text)
  assert.match(text, /\d{1,2}:\d{2}:\d{2}/)
})
