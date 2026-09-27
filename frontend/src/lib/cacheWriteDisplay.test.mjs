import test from "node:test"
import assert from "node:assert/strict"
import { cacheWriteDisplayValue } from "./cacheWriteDisplay.ts"

test("Codex Auth 有请求时不把来源的零值当成已知缓存创建量", () => {
  assert.equal(cacheWriteDisplayValue("codex", "auth", 0, true), null)
  assert.equal(cacheWriteDisplayValue("codex", "auth", 1024, true), 1024)
  assert.equal(cacheWriteDisplayValue("codex", "auth", 0, false), 0)
})

test("API Key 与其他平台保留来源的真实零值", () => {
  assert.equal(cacheWriteDisplayValue("codex", "api", 0, true), 0)
  assert.equal(cacheWriteDisplayValue("claude", "auth", 0, true), 0)
})
