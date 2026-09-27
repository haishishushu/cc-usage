import test from "node:test"
import assert from "node:assert/strict"

test("编辑连接把必填 ID 和其他字段按 Tauri 命令签名传递", async () => {
  const calls = []
  globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => { calls.push({ command, args }) } } }
  try {
    const { api } = await import("./api.ts")
    await api.updateConnection({
      id: "connection-1",
      name: "Claude",
      base_url: "https://example.test/api",
      secret: "key",
      model: "model-1",
      effort: "medium",
      context_1m: true,
    })
    assert.deepEqual(calls, [{
      command: "update_connection",
      args: {
        id: "connection-1",
        name: "Claude",
        baseUrl: "https://example.test/api",
        secret: "key",
        model: "model-1",
        effort: "medium",
        context1m: true,
      },
    }])
  } finally {
    delete globalThis.window
  }
})
