import assert from 'node:assert/strict'
import { test } from 'node:test'
import { splitRows, whitespaceMarks } from '../src/lib/diff-rows.ts'

const lines = (kinds) => [...kinds].map((kind) => ({ kind }))
const brief = (kinds) => splitRows(lines(kinds)).map((r) => `${r.left?.index ?? '.'}|${r.right?.index ?? '.'}`)

test('deletions pair with the additions that follow, extra lines leave the other side empty', () => {
  assert.deepEqual(brief(' --+ '), ['0|0', '1|3', '2|.', '4|4'])
  assert.deepEqual(brief('-+++'), ['0|1', '.|2', '.|3'])
})

test('pure additions, pure deletions and back-to-back changes stay on their own side', () => {
  assert.deepEqual(brief('++'), ['.|0', '.|1'])
  assert.deepEqual(brief('--'), ['0|.', '1|.'])
  // 新增之后又出现删除，是另一处修改，不能和前面的新增配对
  assert.deepEqual(brief('-+-+'), ['0|1', '2|3'])
  assert.deepEqual(brief('+-'), ['.|0', '1|.'])
})

test('the no-newline marker follows the line before it', () => {
  assert.deepEqual(brief('-\\+\\'), ['0|2', '1|3'])
  assert.deepEqual(brief(' \\'), ['0|0', '1|1'])
})

test('whitespace marks line up with the text and expand tabs to the next stop', () => {
  const cells = (s) => s.replaceAll('\u200b', '')
  assert.equal(cells(whitespaceMarks('a  b ')), '\u00a0··\u00a0·')
  // 制表符占到下一个 4 的倍数列：行首占 4 格，第 3 列之后只占 1 格
  assert.equal(cells(whitespaceMarks('\tx')), '→\u00a0\u00a0\u00a0\u00a0')
  assert.equal(cells(whitespaceMarks('abc\tx')), '\u00a0\u00a0\u00a0→\u00a0')
  assert.equal(cells(whitespaceMarks('中 a')), '\u3000·\u00a0')
  assert.equal(whitespaceMarks(''), '')
})
