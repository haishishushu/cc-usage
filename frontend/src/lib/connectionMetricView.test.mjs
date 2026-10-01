import test from "node:test"
import assert from "node:assert/strict"
import { connectionMetricView } from "./connectionMetricView.ts"

const window = (key, used_percent) => ({ key, window_name: key, used_percent, amount_text: null, resets_at: null })

test("Auth 固定显示 5 小时和周额度，缺失窗口保持未知", () => {
  const view = connectionMetricView("auth", { state: "ok", plan: null, windows: [window("7d", 40)] }, null, null)
  assert.equal(view.mode, "plan")
  assert.deepEqual(view.items.map((item) => item.value), ["—", "剩余 60%"])
})

test("API Key 查到套餐窗口时显示双额度", () => {
  const view = connectionMetricView("api", { state: "ok", plan: null, windows: [window("5h", 25), window("7d", 70)] }, null, 1200)
  assert.equal(view.mode, "plan")
  assert.deepEqual(view.items.map((item) => item.label), ["5 小时", "周额度"])
})

test("纯 API Key 显示本机今日 Token 和真实网关余额", () => {
  const view = connectionMetricView("api", { state: "unsupported", reason: "无套餐" }, { state: "ok", balance: 12.5, currency: "USD", used: null }, 1234)
  assert.equal(view.mode, "metered")
  assert.deepEqual(view.items.map((item) => item.value), ["1,234", "12.50 USD"])
})

test("额度查询失败时不猜测 API Key 是否为套餐", () => {
  const view = connectionMetricView("api", { state: "failed", reason: "网络错误" }, null, 1234)
  assert.equal(view.mode, "unknown")
  assert.equal(view.items[0].value, "—")
})

test("ZCode 本机套餐 Key 显示真实剩余额度", () => {
  const quota = { state: "ok", plan: null, windows: [
    { ...window("5h", 20), remaining_text: "80 Credits" },
    { ...window("7d", 40), remaining_text: "600 Credits" },
  ] }
  const view = connectionMetricView("auth", quota, null, null, { platform: "zcode", totalTokens: 1234, credits: null })
  assert.deepEqual(view.items.map((item) => [item.label, item.value]), [
    ["5 小时剩余", "80 Credits"], ["周剩余", "600 Credits"],
  ])
})

test("ZCode 额度不可用时仍展示已采集的本机 Token 和原因", () => {
  const view = connectionMetricView("auth", { state: "unsupported", reason: "未配置套餐 Key" }, null, null, { platform: "zcode", totalTokens: 1234, credits: null })
  assert.equal(view.items[0].value, "1,234")
  assert.equal(view.reason, "未配置套餐 Key")
})

test("WorkBuddy 展示累计 Token 和已上报积分，不伪装成剩余积分", () => {
  const view = connectionMetricView("auth", { state: "unsupported", reason: "无在线余额接口" }, null, null, { platform: "workbuddy", totalTokens: 25045365, credits: 560.23 })
  assert.deepEqual(view.items.map((item) => [item.label, item.value]), [
    ["本机累计 Token", "25,045,365"], ["累计上报积分", "560.23"],
  ])
  assert.match(view.items[1].title, /不是账号剩余积分/)
})
