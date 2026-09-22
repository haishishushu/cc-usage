import assert from "node:assert/strict"
import test from "node:test"
import { dockPulseActive } from "./dockPulse.ts"

test("停靠条只在真实新增且允许动效时脉冲", () => {
  assert.equal(dockPulseActive(120, "enter", false, false), true)
  assert.equal(dockPulseActive(120, "hold", false, false), true)
  assert.equal(dockPulseActive(0, "hold", false, false), false)
  assert.equal(dockPulseActive(120, "leave", false, false), false)
  assert.equal(dockPulseActive(120, "hold", true, false), false)
  assert.equal(dockPulseActive(120, "hold", false, true), false)
})
