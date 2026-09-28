import assert from 'node:assert/strict'
import test from 'node:test'
import { isUsageChange, throttleTrailing } from './usageRefresh.ts'

function fakeTimers() {
  let now = 0
  let queue = []
  return {
    now: () => now,
    set: (fn, ms) => { const timer = { fn, at: now + ms }; queue.push(timer); return timer },
    clear: (timer) => { queue = queue.filter((item) => item !== timer) },
    advance(ms) {
      now += ms
      const due = queue.filter((item) => item.at <= now)
      queue = queue.filter((item) => item.at > now)
      due.forEach((item) => item.fn())
    },
  }
}

test('first change refreshes immediately and bursts collapse into one trailing refresh', () => {
  const timers = fakeTimers()
  let runs = 0
  const refresh = throttleTrailing(() => { runs += 1 }, 1500, timers)
  refresh()
  assert.equal(runs, 1)
  for (let index = 0; index < 5; index += 1) { timers.advance(100); refresh() }
  assert.equal(runs, 1, '窗口内的连续变化不逐条重查')
  timers.advance(1500)
  assert.equal(runs, 2, '窗口结束补一次，最后的变化不会丢')
  timers.advance(5000)
  assert.equal(runs, 2)
  refresh()
  assert.equal(runs, 3, '静默足够久后的新变化立即刷新')
})

test('cancel drops a pending trailing refresh', () => {
  const timers = fakeTimers()
  let runs = 0
  const refresh = throttleTrailing(() => { runs += 1 }, 1000, timers)
  refresh()
  refresh()
  refresh.cancel()
  timers.advance(2000)
  assert.equal(runs, 1)
})

test('heartbeat renewals and other platforms are not statistics changes', () => {
  const usage = (platform, data_changed) => ({ platform, data_changed })
  assert.equal(isUsageChange(usage('codex', true), 'codex'), true)
  assert.equal(isUsageChange(usage('codex', false), 'codex'), false)
  assert.equal(isUsageChange(usage('claude', true), 'codex'), false)
  assert.equal(isUsageChange(usage('codex', undefined), 'codex'), true, '旧版后端没有该字段时按有变化处理')
})
