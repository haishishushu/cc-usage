import assert from 'node:assert/strict'
import test from 'node:test'
import { LiveTokenCounter } from './liveTokenCounter.ts'

test('current turn snapshot restores on mount without replay or double counting', () => {
  const counter = new LiveTokenCounter()
  counter.setTarget('total', 12000, 0, true)
  assert.equal(counter.sample(0).values.total, 12000)
  counter.setTarget('total', 12000, 100)
  assert.equal(counter.sample(100).values.total, 12000)
  counter.setTarget('total', 12500, 200)
  assert.equal(counter.sample(1100).values.total, 12500)
  counter.setTarget('total', 0, 1200)
  assert.equal(counter.sample(1200).values.total, 0)
})

test('continuous updates advance before the stream stops, without overshoot or replay', () => {
  const counter = new LiveTokenCounter()
  let previous = 0
  for (let i = 0; i < 30; i++) {
    counter.add('total', 100, i * 30)
    counter.add('session', 100, i * 30)
    const { values } = counter.sample(i * 30 + 16)
    assert.ok(values.total > previous)
    assert.ok(values.total <= (i + 1) * 100)
    assert.equal(values.total, values.session)
    previous = values.total
  }
  assert.deepEqual({ ...counter.sample(2000).values }, { total: 3000, session: 3000 })
  assert.equal(counter.sample(2000).settled, true)
})

test('reduced motion settles immediately; a new segment starts from zero', () => {
  const counter = new LiveTokenCounter()
  counter.add('total', 10000, 0)
  assert.equal(counter.sample(1, true).values.total, 10000)
  assert.equal(counter.sample(2).settled, true)
  counter.clear()
  counter.add('total', 10, 20)
  assert.equal(counter.sample(20).values.total, 0)
  assert.equal(counter.sample(920).values.total, 10)
})
