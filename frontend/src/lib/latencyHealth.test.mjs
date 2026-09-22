import assert from "node:assert/strict"
import test from "node:test"
import {
  DURATION_THRESHOLDS_MS,
  FIRST_TOKEN_THRESHOLDS_MS,
  LATENCY_TEXT_CLASSES,
  durationSeverity,
  firstTokenSeverity,
  formatLatency,
} from "./latencyHealth.ts"

test("阈值边界按「达到即进档」判定，不因差 1 毫秒错档", () => {
  assert.equal(firstTokenSeverity(9_999), "good")
  assert.equal(firstTokenSeverity(10_000), "warn")
  assert.equal(firstTokenSeverity(29_999), "warn")
  assert.equal(firstTokenSeverity(30_000), "slow")
  assert.equal(firstTokenSeverity(59_999), "slow")
  assert.equal(firstTokenSeverity(60_000), "critical")
})

test("总耗时用更宽的尺子，否则含多轮工具调用的请求会长期泛红", () => {
  // 30 秒对首字已是 slow，对整条请求仍算正常
  assert.equal(firstTokenSeverity(30_000), "slow")
  assert.equal(durationSeverity(30_000), "good")
  assert.equal(durationSeverity(60_000), "warn")
  assert.equal(durationSeverity(180_000), "slow")
  assert.equal(durationSeverity(300_000), "critical")
})

test("异常输入退回 good，不让脏数据把整列染红", () => {
  for (const bad of [-1, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.equal(firstTokenSeverity(bad), "good", String(bad))
    assert.equal(durationSeverity(bad), "good", String(bad))
  }
  assert.equal(firstTokenSeverity(0), "good")
})

test("四档都有配色，且深浅主题各给一个值", () => {
  for (const severity of ["good", "warn", "slow", "critical"]) {
    const cls = LATENCY_TEXT_CLASSES[severity]
    assert.ok(cls, severity)
    assert.ok(cls.includes("dark:"), `${severity} 缺少深色主题配色`)
  }
})

test("时长格式化：秒以内给毫秒，避免 0.0s 这种看不出差别的写法", () => {
  assert.equal(formatLatency(0), "0ms")
  assert.equal(formatLatency(340), "340ms")
  assert.equal(formatLatency(999), "999ms")
  assert.equal(formatLatency(1_000), "1.0s")
  assert.equal(formatLatency(2_500), "2.5s")
  assert.equal(formatLatency(59_900), "59.9s")
  assert.equal(formatLatency(60_000), "1m")
  assert.equal(formatLatency(95_000), "1m35s")
})

test("没有数据时返回 null，交给界面显示「—」而不是伪造 0", () => {
  assert.equal(formatLatency(null), null)
  assert.equal(formatLatency(-1), null)
  assert.equal(formatLatency(Number.NaN), null)
})

test("阈值与 sub2api 的 latencyHealth.ts 保持同一组数值", () => {
  assert.deepEqual({ ...FIRST_TOKEN_THRESHOLDS_MS }, { warn: 10_000, slow: 30_000, critical: 60_000 })
  assert.deepEqual({ ...DURATION_THRESHOLDS_MS }, { warn: 60_000, slow: 180_000, critical: 300_000 })
})
