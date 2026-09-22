import test from "node:test"
import assert from "node:assert/strict"
import { connectionToggleAction } from "./connectionActions.ts"

test("unified connection action is mutually exclusive", () => {
  assert.deepEqual(connectionToggleAction("connected", false), { label: "启用", action: "select", disabled: false })
  assert.deepEqual(connectionToggleAction("connected", true), { label: "断开", action: "clear", disabled: false })
  assert.deepEqual(connectionToggleAction("paused", false), { label: "启用", action: "reconnect", disabled: false })
  assert.deepEqual(connectionToggleAction("expired", false), { label: "启用", action: "unavailable", disabled: true })
})
