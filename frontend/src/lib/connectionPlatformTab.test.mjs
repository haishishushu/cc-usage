import test from "node:test"
import assert from "node:assert/strict"
import { canAddConnection, resolveActivePlatform } from "./connectionPlatformTab.ts"

const conn = (id, platformId) => ({ id, platformId })

test("只有声明了连接能力的平台才开放添加入口", () => {
  assert.equal(canAddConnection("claude"), true)
  assert.equal(canAddConnection("codex"), true)
  for (const platform of ["gemini", "grok", "zcode", "trae", "qoder", "workbuddy"]) {
    assert.equal(canAddConnection(platform), true, platform)
  }
})

test("默认停在灵动岛使用中的连接所属平台", () => {
  const platform = resolveActivePlatform({ connections: [conn("a", "claude"), conn("b", "codex")], selectedId: "b" })
  assert.equal(platform, "codex")
})

test("没有使用中的连接时回落到 Claude", () => {
  assert.equal(resolveActivePlatform({ connections: [], selectedId: null }), "claude")
  assert.equal(resolveActivePlatform({ connections: [conn("a", "codex")], selectedId: "zzz" }), "claude")
})
