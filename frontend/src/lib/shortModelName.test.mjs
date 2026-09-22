import assert from "node:assert/strict"
import test from "node:test"
import { shortModelName } from "./modelName.ts"

test("去掉结尾的发布日期，保留真正用于区分模型的部分", () => {
  assert.equal(shortModelName("claude-sonnet-4-5-20250929"), "claude-sonnet-4-5")
  assert.equal(shortModelName("claude-opus-4-1-20250805"), "claude-opus-4-1")
})

test("没有日期后缀的名字原样返回", () => {
  for (const name of ["claude-opus-5", "gpt-6-astra", "glm-5.3-flash", "zai-org/GLM-5.3", "gpt-5.6"]) {
    assert.equal(shortModelName(name), name)
  }
})

test("只裁结尾的八位日期，名字中间的数字段不受影响", () => {
  // 版本号里的数字、以及非八位的尾段都必须原样保留
  assert.equal(shortModelName("claude-opus-4-8"), "claude-opus-4-8")
  assert.equal(shortModelName("model-2025"), "model-2025")
  assert.equal(shortModelName("model-202509291"), "model-202509291")
  // 日期在中间而不在结尾时不裁
  assert.equal(shortModelName("claude-20250929-preview"), "claude-20250929-preview")
})
