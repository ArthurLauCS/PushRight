import assert from 'node:assert/strict'
import { test } from 'node:test'
import { calendarStats, formatTime, readTimeZone } from '../src/lib/time.ts'

const sec = (iso) => Date.parse(iso) / 1000

test('time zones handle date boundaries, fractional offsets and historical daylight saving', () => {
  const instant = sec('2026-01-01T00:15:00Z')
  assert.equal(formatTime(instant, 'UTC'), '2026-01-01 00:15')
  assert.equal(formatTime(instant, 'Asia/Shanghai'), '2026-01-01 08:15')
  assert.equal(formatTime(instant, 'America/Los_Angeles'), '2025-12-31 16:15')
  assert.equal(formatTime(instant, 'Asia/Kathmandu'), '2026-01-01 06:00')
  assert.equal(formatTime(sec('2026-03-08T09:59:00Z'), 'America/Los_Angeles'), '2026-03-08 01:59')
  assert.equal(formatTime(sec('2026-03-08T10:00:00Z'), 'America/Los_Angeles'), '2026-03-08 03:00')
  assert.equal(formatTime(sec('2026-07-01T00:15:00Z'), 'America/Los_Angeles'), '2026-06-30 17:15')
  assert.equal(formatTime(instant, ''), formatTime(instant, Intl.DateTimeFormat().resolvedOptions().timeZone))
})

test('missing or invalid stored time zones follow the system and aliases are canonicalized', () => {
  for (const value of [null, '', 'not/a-zone']) assert.equal(readTimeZone(value), '')
  assert.equal(readTimeZone('Asia/Shanghai'), 'Asia/Shanghai')
  assert.equal(readTimeZone('Etc/UTC'), 'UTC')
})

test('statistics use the selected time zone for date bounds and months, including empty months', () => {
  const times = ['2026-04-01T00:15:00Z', '2026-01-01T00:15:00Z', '2026-01-20T12:00:00Z'].map(sec)
  assert.deepEqual(calendarStats(times, 'Asia/Shanghai'), {
    first: '2026-01-01', last: '2026-04-01',
    months: [{ name: '2026-01', count: 2 }, { name: '2026-02', count: 0 }, { name: '2026-03', count: 0 }, { name: '2026-04', count: 1 }],
  })
  assert.deepEqual(calendarStats(times, 'America/Los_Angeles'), {
    first: '2025-12-31', last: '2026-03-31',
    months: [{ name: '2025-12', count: 1 }, { name: '2026-01', count: 1 }, { name: '2026-02', count: 0 }, { name: '2026-03', count: 1 }],
  })
  assert.deepEqual(calendarStats([], 'UTC'), { first: '', last: '', months: [] })
})
