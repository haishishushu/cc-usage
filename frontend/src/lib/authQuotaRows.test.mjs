import test from "node:test"
import assert from "node:assert/strict"
import { authQuotaRows } from "./authQuotaRows.ts"

test("Auth 无额度时保留未知的 5h / 7d，不伪造零用量", () => {
  const rows = authQuotaRows()
  assert.deepEqual(rows.map(r => [r.key, r.usedPercent]), [["5h", null], ["7d", null]])
})
test("仅有周额度时不能当作 5 小时额度", () => {
  const weekly = { key: "7d", usedPercent: 56, resetCountdown: "还剩 3d 6h" }
  const rows = authQuotaRows([weekly], "账号额度接口未返回该窗口")
  assert.equal(rows[0].usedPercent, null)
  assert.equal(rows[0].usedText, "账号额度接口未返回该窗口")
  assert.equal(rows[1], weekly)
})
test("Codex 有其他窗口时仍保留缺失的 5 小时提示", () => {
  const weekly = { key: "7d", usedPercent: 1 }
  const rows = authQuotaRows([{ key: "1d", usedPercent: 20 }, weekly], "账号额度接口未返回该窗口")
  assert.deepEqual(rows.map(row => row.key), ["5h", "7d"])
  assert.equal(rows[0].usedPercent, null)
  assert.equal(rows[1], weekly)
})
