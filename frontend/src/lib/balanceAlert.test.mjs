import assert from 'node:assert/strict'
import test from 'node:test'
import { balanceAlert } from './balanceAlert.ts'
test('balance warning requires configured matching currency and fresh actual data', () => {
  assert.equal(balanceAlert(10, 'USD', 10, 'USD', false), true)
  assert.equal(balanceAlert(0, 'USD', 10, 'USD', false), true)
  assert.equal(balanceAlert(11, 'USD', 10, 'USD', false), false)
  assert.equal(balanceAlert(1, 'CNY', 10, 'USD', false), false)
  assert.equal(balanceAlert(null, 'USD', 10, 'USD', false), false)
  assert.equal(balanceAlert(1, 'USD', null, 'USD', false), false)
  assert.equal(balanceAlert(1, 'USD', 10, 'USD', true), false)
})
