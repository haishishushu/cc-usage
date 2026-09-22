import assert from "node:assert/strict"
import test from "node:test"
import {
  TOAST_DURATION,
  TOAST_LIMIT,
  markToastExiting,
  pushToast,
  removeToast,
  shiftToasts,
  toastRemaining,
  visibleToasts,
} from "./toastQueue.ts"

const push = (list, input, id, now) => pushToast(list, input, id, now)

test("入队补齐 id、时间戳与该 tone 的默认时长", () => {
  const list = push([], { tone: "success", title: "已切换到 kjqy-claude" }, 1, 1000)
  assert.equal(list.length, 1)
  assert.deepEqual(list[0], {
    id: 1,
    tone: "success",
    title: "已切换到 kjqy-claude",
    detail: undefined,
    duration: TOAST_DURATION.success,
    createdAt: 1000,
    exiting: false,
  })
})

test("显式 duration 覆盖默认时长，0 表示不自动消失", () => {
  const list = push([], { tone: "danger", title: "保存失败", duration: 0 }, 1, 0)
  assert.equal(list[0].duration, 0)
  assert.equal(toastRemaining(list[0], 999999), Infinity)
})

test("失败比成功停留更久，留得住读完的时间", () => {
  assert.ok(TOAST_DURATION.danger > TOAST_DURATION.warn)
  assert.ok(TOAST_DURATION.warn > TOAST_DURATION.success)
})

test("同内容不堆叠，只重置计时且不改变 id 与位置", () => {
  const first = push([], { tone: "info", title: "已是最新" }, 1, 1000)
  const second = push(first, { tone: "success", title: "已更新" }, 2, 1200)
  const again = push(second, { tone: "info", title: "已是最新" }, 3, 5000)
  assert.equal(again.length, 2)
  assert.equal(again[0].id, 1, "沿用原 id，不新增条目")
  assert.equal(again[0].createdAt, 5000, "计时被重置")
  assert.equal(again[1].id, 2, "其余条目位置不动")
})

test("标题相同但详情不同视为两条", () => {
  const list = push(
    push([], { tone: "danger", title: "更新失败", detail: "连接 A" }, 1, 0),
    { tone: "danger", title: "更新失败", detail: "连接 B" },
    2,
    10,
  )
  assert.equal(list.length, 2)
})

test("已在退场的同内容条目不被复用，重新入队为新条", () => {
  const list = markToastExiting(push([], { tone: "success", title: "已移除" }, 1, 0), 1)
  const next = push(list, { tone: "success", title: "已移除" }, 2, 100)
  assert.equal(next.length, 2)
  assert.equal(next[1].id, 2)
  assert.equal(next[1].exiting, false)
})

test("超过上限时最旧的一条转入退场，而不是被直接删除", () => {
  let list = []
  for (let i = 1; i <= TOAST_LIMIT; i += 1) {
    list = push(list, { tone: "success", title: `第 ${i} 条` }, i, i)
  }
  assert.equal(visibleToasts(list).length, TOAST_LIMIT)

  list = push(list, { tone: "success", title: "新来的" }, 99, 100)
  assert.equal(list.length, TOAST_LIMIT + 1, "退场动画期间仍留在列表里")
  assert.equal(list[0].exiting, true, "最旧的一条被标记退场")
  assert.equal(visibleToasts(list).length, TOAST_LIMIT)
  assert.equal(visibleToasts(list).at(-1).title, "新来的")
})

test("计数只看未退场的条目，退场中的不占名额", () => {
  let list = []
  for (let i = 1; i <= TOAST_LIMIT; i += 1) {
    list = push(list, { tone: "warn", title: `第 ${i} 条` }, i, i)
  }
  list = markToastExiting(list, 1)
  list = push(list, { tone: "warn", title: "补位" }, 4, 50)
  assert.equal(visibleToasts(list).length, TOAST_LIMIT)
  assert.equal(list.filter((item) => item.exiting).length, 1, "不会误伤第二条")
})

test("标记退场幂等，移除按 id 且对不存在的 id 无副作用", () => {
  const list = push([], { tone: "success", title: "已保存" }, 1, 0)
  const exiting = markToastExiting(markToastExiting(list, 1), 1)
  assert.equal(exiting[0].exiting, true)
  assert.equal(removeToast(exiting, 1).length, 0)
  assert.equal(removeToast(exiting, 404).length, 1)
})

test("剩余时长按已过去的时间递减，到点为 0 不为负", () => {
  const item = push([], { tone: "warn", title: "未找到本机配置" }, 1, 1000)[0]
  assert.equal(toastRemaining(item, 1000), TOAST_DURATION.warn)
  assert.equal(toastRemaining(item, 1600), TOAST_DURATION.warn - 600)
  assert.equal(toastRemaining(item, 1000 + TOAST_DURATION.warn), 0)
  assert.equal(toastRemaining(item, 999999), 0)
})

test("暂停恢复后按暂停时长整体平移，剩余时间不被悬停吃掉", () => {
  const list = push([], { tone: "danger", title: "导入失败" }, 1, 1000)
  // 1500 悬停，3500 移开：暂停了 2000ms
  const resumed = shiftToasts(list, 2000)
  assert.equal(resumed[0].createdAt, 3000)
  assert.equal(toastRemaining(resumed[0], 3500), TOAST_DURATION.danger - 500)
})

test("平移不影响不自动消失的条目", () => {
  const list = push([], { tone: "danger", title: "长期错误", duration: 0 }, 1, 1000)
  assert.equal(toastRemaining(shiftToasts(list, 5000)[0], 99999), Infinity)
})
