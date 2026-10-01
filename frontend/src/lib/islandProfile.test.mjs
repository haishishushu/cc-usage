import assert from "node:assert/strict"
import test from "node:test"
import { MAX_ISLANDS, cloneActions, isOwnIslandEvent, islandCount, islandIdFromSearch, islandProfile } from "./islandProfile.ts"

const base = {
  island_platform: "claude",
  island_kind: "auth",
  island_connection_id: "c1",
  island_connection_name: "官方订阅",
  island_source_id: "local:claude",
  dock: { edge: "top", offset: 400, monitor: null },
  island_clones: [
    { id: "a1", platform: "codex", kind: "auth", connection_id: "c2", connection_name: "工作账号", source_id: "local:codex", dock: { edge: null, offset: 0, monitor: null }, position: [100, 200] },
  ],
}

test("本体没有 clone 参数，分身从 URL 读取 id", () => {
  assert.equal(islandIdFromSearch("?window=island"), null)
  assert.equal(islandIdFromSearch("?window=island&clone="), null)
  assert.equal(islandIdFromSearch("?window=island&clone=a1"), "a1")
  assert.equal(islandIdFromSearch("?window=menu&source=island&clone=a1"), "a1")
})

test("本体与分身各自取到自己的连接与停靠", () => {
  const primary = islandProfile(base, null)
  assert.equal(primary.connection_id, "c1")
  assert.equal(primary.dock.edge, "top")
  const clone = islandProfile(base, "a1")
  assert.equal(clone.platform, "codex")
  assert.equal(clone.connection_id, "c2")
  assert.equal(clone.dock.edge, null)
})

test("分身记录丢失时回退本体配置，旧设置缺少 island_clones 也能工作", () => {
  assert.equal(islandProfile(base, "missing").connection_id, "c1")
  const legacy = { ...base }
  delete legacy.island_clones
  assert.equal(islandProfile(legacy, "a1").connection_id, "c1")
  assert.equal(islandCount(legacy), 1)
  assert.equal(islandCount(base), 2)
})

test("停靠事件只被目标岛处理", () => {
  assert.equal(isOwnIslandEvent({ island: null }, null), true)
  assert.equal(isOwnIslandEvent({}, null), true)
  assert.equal(isOwnIslandEvent({ island: "a1" }, "a1"), true)
  assert.equal(isOwnIslandEvent({ island: "a1" }, null), false)
  assert.equal(isOwnIslandEvent({ island: null }, "a1"), false)
})

test("至少保留一个、最多 MAX_ISLANDS 个", () => {
  assert.deepEqual(cloneActions(1), { canCreate: true, canDestroy: false })
  assert.deepEqual(cloneActions(3), { canCreate: true, canDestroy: true })
  assert.deepEqual(cloneActions(MAX_ISLANDS), { canCreate: false, canDestroy: true })
})
