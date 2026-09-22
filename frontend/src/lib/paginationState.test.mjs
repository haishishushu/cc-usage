import assert from "node:assert/strict"
import test from "node:test"
import { parsePageInput } from "./paginationState.ts"

test("页码跳转只接受有效范围内的整数", () => {
  assert.deepEqual(parsePageInput("3", 7), { page: 3, error: null })
  assert.equal(parsePageInput("0", 7).page, null)
  assert.equal(parsePageInput("8", 7).page, null)
  assert.equal(parsePageInput("1.5", 7).page, null)
  assert.equal(parsePageInput("abc", 7).page, null)
})
