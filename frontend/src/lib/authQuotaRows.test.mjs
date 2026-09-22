import test from "node:test"
import assert from "node:assert/strict"
import { authQuotaRows } from "./authQuotaRows.ts"

test("Auth 无额度时保留未知的 5h / 7d，不伪造零用量", () => {
  const rows = authQuotaRows()
  assert.deepEqual(rows.map(r => [r.key, r.usedPercent]), [["5h", null], ["7d", null]])
})
test("仅有周额度时不能当作 5 小时额度", () => {
  const weekly = { key: "7d", usedPercent: 56, resetCountdown: "还剩 3d 6h" }
  const rows = authQuotaRows([weekly])
  assert.equal(rows[0].usedPercent, null)
  assert.equal(rows[1], weekly)
})
