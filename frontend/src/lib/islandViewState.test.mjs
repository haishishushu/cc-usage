import assert from "node:assert/strict"
import test from "node:test"
import { collapseExpandedView, snapIndicatorVisible } from "./islandViewState.ts"

test("未拖动的停靠灵动岛收起后保留探出态，等待鼠标移开自动缩回", () => {
  assert.deepEqual(collapseExpandedView("right"), {
    mode: "docked",
    peek: true,
    edge: "right",
  })
})

test("自由灵动岛收起后回到自由收缩态", () => {
  assert.deepEqual(collapseExpandedView(null), {
    mode: "collapsed",
    peek: false,
    edge: null,
  })
})

test("自由态和停靠探出态拖动都显示吸附预览，松手或离开吸附范围后隐藏", () => {
  assert.equal(snapIndicatorVisible(true, "collapsed", true), true)
  assert.equal(snapIndicatorVisible(true, "expanded", true), true)
  assert.equal(snapIndicatorVisible(true, "docked", true), true)
  assert.equal(snapIndicatorVisible(false, "docked", true), false)
  assert.equal(snapIndicatorVisible(true, "docked", false), false)
  assert.equal(snapIndicatorVisible(false, "collapsed", true), false)
  assert.equal(snapIndicatorVisible(true, "collapsed", false), false)
})
