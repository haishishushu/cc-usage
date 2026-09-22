import test from "node:test"
import assert from "node:assert/strict"
import { totalInputTokens } from "./requestInput.ts"

test("Claude 的 input_tokens 不含缓存，总输入要把缓存读写加回来", () => {
  // 取自本机真实记录：input 只有 2，缓存命中 198100，缓存创建 380
  assert.equal(totalInputTokens({ platform: "claude", input: 2, cache_read: 198100, cache_write: 380 }), 198482)
})

test("Codex 的 input_tokens 已含 cached，直接采用，不能再加一次", () => {
  // 取自本机真实记录：input 113035 本身已经包含 cache_read 110336
  assert.equal(totalInputTokens({ platform: "codex", input: 113035, cache_read: 110336, cache_write: 0 }), 113035)
})

test("输入未知时不编造，返回 null", () => {
  assert.equal(totalInputTokens({ platform: "claude", input: null, cache_read: 100, cache_write: 0 }), null)
  assert.equal(totalInputTokens({ platform: "codex", input: null, cache_read: 100, cache_write: 0 }), null)
})

test("Claude 缺任一缓存字段时总输入不可知，与 total_tokens 的口径保持一致", () => {
  assert.equal(totalInputTokens({ platform: "claude", input: 2, cache_read: null, cache_write: 380 }), null)
  assert.equal(totalInputTokens({ platform: "claude", input: 2, cache_read: 198100, cache_write: null }), null)
})

test("Codex 不依赖缓存字段，缺失也能给出总输入", () => {
  assert.equal(totalInputTokens({ platform: "codex", input: 113035, cache_read: null, cache_write: null }), 113035)
})

test("输入口径未知时不把原始计数冒充总输入", () => {
  assert.equal(totalInputTokens({ platform: "grok", input: 500, cache_read: 100, cache_write: 0 }), null)
})

test("原生来源按明确的输入口径显示，真实零值保留", () => {
  assert.equal(totalInputTokens({ platform: "workbuddy", input_semantics: "includes_cache", input: 500, cache_read: 100, cache_write: null }), 500)
  assert.equal(totalInputTokens({ platform: "qoder", input_semantics: "excludes_cache", input: 5, cache_read: 100, cache_write: 20 }), 125)
  assert.equal(totalInputTokens({ platform: "gemini", input_semantics: "includes_cache", input: 0, cache_read: 0, cache_write: null }), 0)
})
