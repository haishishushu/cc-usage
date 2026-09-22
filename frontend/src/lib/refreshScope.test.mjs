import test from "node:test"
import assert from "node:assert/strict"
import { shouldRefreshConnection } from "./refreshScope.ts"

test("targeted refresh only matches the active connection", () => {
  assert.equal(shouldRefreshConnection("claude-1", "claude-1"), true)
  assert.equal(shouldRefreshConnection("claude-1", "codex-1"), false)
  assert.equal(shouldRefreshConnection(null, "codex-1"), true)
})
