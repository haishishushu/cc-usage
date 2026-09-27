import test from "node:test"
import assert from "node:assert/strict"
import { remainingDisplay } from "./remainingDisplay.ts"

test("Grok 将真实已用百分比换算为剩余，保留重置时间", () => {
  const view = remainingDisplay("grok", "auth", {
    state: "ok", plan: null, windows: [{ key: "7d", window_name: "周额度", used_percent: 37.5, amount_text: null, resets_at: "2026-09-30T00:00:00Z" }],
  })
  assert.equal(view.rows[0].value, "62.5%")
  assert.equal(view.rows[0].label, "SuperGrok 周剩余")
  assert.equal(view.rows[0].resetsAt, "2026-09-30T00:00:00Z")
})

test("Grok 缺失百分比不会变成 100% 剩余", () => {
  const view = remainingDisplay("grok", "auth", {
    state: "ok", plan: null, windows: [{ key: "credits", window_name: "积分", used_percent: null, amount_text: null, resets_at: null }],
  })
  assert.equal(view.rows[0].value, "—")
  assert.match(view.reason, /未提供/)
})

test("其他平台缺少远程授权时不把本机消耗冒充余额", () => {
  for (const platform of ["gemini", "trae", "qoder", "workbuddy"]) {
    const view = remainingDisplay(platform, "auth", null)
    assert.equal(view.rows.length, 0)
    assert.equal(view.state, "unsupported")
    assert.ok(view.reason.length > 0)
  }
})

test("Grok API Key 与 SuperGrok 订阅额度严格分开", () => {
  const view = remainingDisplay("grok", "api", null)
  assert.equal(view.rows.length, 0)
  assert.match(view.reason, /Management Key/)
})

test("ZCode 显示 BigModel API Key 返回的精确剩余积分", () => {
  const view = remainingDisplay("zcode", "auth", {
    state: "ok", plan: "pro", windows: [
      { key: "5h", window_name: "5 小时额度", used_percent: 25, amount_text: "3000 / 12000 credits", remaining_text: "8999 Credits", resets_at: null },
      { key: "7d", window_name: "周额度", used_percent: 0, amount_text: "0 / 60000 credits", remaining_text: "60000 Credits", resets_at: null },
    ],
  })
  assert.equal(view.rows[0].value, "8999 Credits")
  assert.equal(view.rows[1].value, "60000 Credits")
  assert.match(view.source, /API Key/)
})
