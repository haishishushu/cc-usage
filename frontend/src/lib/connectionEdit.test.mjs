import test from "node:test"
import assert from "node:assert/strict"
import { planConnectionEdit } from "./connectionEdit.ts"

test("改名与换 Key 组合成保存计划，无变化时为空计划", () => {
  // 名称与 Key 都填：两个动作都要做，名称与 Key 去首尾空白
  assert.deepEqual(
    planConnectionEdit({ originalName: "测试", name: " 公司号 ", secret: " sk-new-1234 " }),
    { rename: "公司号", replaceKey: "sk-new-1234" },
  )
  // 只改名：不产生换 Key 动作
  assert.deepEqual(
    planConnectionEdit({ originalName: "测试", name: "新名", secret: "" }),
    { rename: "新名" },
  )
  // 只换 Key：名称未变不产生改名动作
  assert.deepEqual(
    planConnectionEdit({ originalName: "测试", name: "测试", secret: "sk-x-1" }),
    { replaceKey: "sk-x-1" },
  )
  // 什么都没改：空计划，界面据此直接关弹窗
  assert.deepEqual(planConnectionEdit({ originalName: "测试", name: "测试", secret: "" }), {})
})

test("空名称或超长名称拒绝保存，与后端同一口径", () => {
  assert.deepEqual(
    planConnectionEdit({ originalName: "测试", name: "   ", secret: "" }),
    { error: "连接名称应为 1 到 80 个字符" },
  )
  assert.deepEqual(
    planConnectionEdit({ originalName: "测试", name: "长".repeat(81), secret: "" }),
    { error: "连接名称应为 1 到 80 个字符" },
  )
  // 80 个字符恰好允许
  assert.deepEqual(
    planConnectionEdit({ originalName: "测试", name: "名".repeat(80), secret: "" }),
    { rename: "名".repeat(80) },
  )
})
