import assert from 'node:assert/strict'
import { test } from 'node:test'
import { orderBranches } from '../src/lib/branch-order.ts'

test('sidebar branches: current first, then pinned in pin order, the rest untouched', () => {
  const names = (list) => list.map((r) => r.name)
  const refs = ['a', 'b', 'c', 'd', 'e'].map((name) => ({ name }))
  assert.deepEqual(names(orderBranches(refs, 'd', ['e', 'b'])), ['d', 'e', 'b', 'a', 'c'])
  assert.deepEqual(names(orderBranches(refs, 'b', ['b', 'gone'])), ['b', 'a', 'c', 'd', 'e'], 'a pinned current branch stays first; deleted pins are ignored')
  assert.deepEqual(names(orderBranches(refs, null, [])), ['a', 'b', 'c', 'd', 'e'])
  assert.deepEqual(names(refs), ['a', 'b', 'c', 'd', 'e'], 'the input is not reordered in place')
})
