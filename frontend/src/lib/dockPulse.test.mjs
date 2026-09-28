import assert from "node:assert/strict"
import test from "node:test"
import { dockPulseActive } from "./dockPulse.ts"

test("停靠条只在真实新增且允许动效时脉冲", () => {
  assert.equal(dockPulseActive(true, "enter", false, false), true)
  assert.equal(dockPulseActive(true, "hold", false, false), true)
  assert.equal(dockPulseActive(false, "hold", false, false), false)
  assert.equal(dockPulseActive(true, "leave", false, false), false)
  assert.equal(dockPulseActive(true, "hold", true, false), false)
  assert.equal(dockPulseActive(true, "hold", false, true), false)
})
