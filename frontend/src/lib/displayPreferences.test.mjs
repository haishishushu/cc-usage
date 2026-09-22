import assert from 'node:assert/strict'
import test from 'node:test'
import { resolveDarkTheme, refreshDelay } from './displayPreferences.ts'

test('system theme follows system changes while explicit themes stay fixed', () => {
  assert.equal(resolveDarkTheme('system', true), true)
  assert.equal(resolveDarkTheme('system', false), false)
  assert.equal(resolveDarkTheme('light', true), false)
  assert.equal(resolveDarkTheme('dark', false), true)
})

test('refresh interval accepts supported values and rejects corrupt settings', () => {
  assert.equal(refreshDelay(1), 60_000)
  assert.equal(refreshDelay(15), 900_000)
  for (const value of [0, -1, NaN, 2, undefined]) assert.equal(refreshDelay(value), 300_000)
})
