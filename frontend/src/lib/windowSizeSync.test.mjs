import test from "node:test"
import assert from "node:assert/strict"
import { createWindowSizeSync } from "./windowSizeSync.ts"

test("原生窗口被独立收窄后，相同内容尺寸仍能强制恢复", async () => {
  const calls = []
  const sync = createWindowSizeSync(async size => { calls.push(size) })
  const size = { width: 428, height: 129 }
  sync.request(size)
  await new Promise(resolve => setImmediate(resolve))
  sync.request(size, true)
  await new Promise(resolve => setImmediate(resolve))
  assert.equal(calls.length, 2)
})

test("慢请求期间反复探出收回，最终仅应用最新停靠尺寸且不并发", async () => {
  const calls = []
  let release
  const sync = createWindowSizeSync(async size => {
    calls.push(size)
    await new Promise(resolve => { release = resolve })
  })
  sync.request({ width: 428, height: 129 })
  sync.request({ width: 14, height: 120 })
  sync.request({ width: 428, height: 300 })
  sync.request({ width: 14, height: 120 })
  assert.equal(calls.length, 1)
  release()
  await new Promise(resolve => setImmediate(resolve))
  assert.deepEqual(calls, [{ width: 428, height: 129 }, { width: 14, height: 120 }])
  release()
  await new Promise(resolve => setImmediate(resolve))
  sync.request({ width: 14, height: 120 })
  assert.equal(calls.length, 2)
})
