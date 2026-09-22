import test from "node:test"
import assert from "node:assert/strict"
import { formatQuotaCountdown } from "./quotaMap.ts"

test("额度倒计时使用紧凑时长并在到零后等待刷新", () => {
  const now = Date.parse("2026-09-16T12:00:00Z")
  assert.equal(formatQuotaCountdown("2026-09-19T17:00:00Z", now), "还剩 3d 5h")
  assert.equal(formatQuotaCountdown("2026-09-16T14:09:00Z", now), "还剩 2h 9m")
  assert.equal(formatQuotaCountdown("2026-09-16T12:00:00Z", now), "等待更新")
  assert.equal(formatQuotaCountdown(null, now), null)
})
