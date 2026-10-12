import assert from 'node:assert/strict'
import { test } from 'node:test'
import { treeRows } from '../src/lib/file-tree.ts'

const files = (...paths) => paths.map((path) => ({ path }))
const brief = (rows) => rows.map((r) => `${'  '.repeat(r.depth)}${r.name}${r.items ? '/' : ''}`)

test('folders come first, single-child chains collapse, files keep their order', () => {
  const rows = treeRows(files('README.md', 'src/lib/locales/zh.ts', 'src/lib/locales/en.ts', 'src/app.css', 'crates/engine/src/diff.rs'))
  assert.deepEqual(brief(rows), [
    'crates/engine/src/', '  diff.rs',
    'src/', '  lib/locales/', '    zh.ts', '    en.ts', '  app.css',
    'README.md',
  ])
  assert.deepEqual(rows[2].items.map((f) => f.path), ['src/lib/locales/zh.ts', 'src/lib/locales/en.ts', 'src/app.css'])
  assert.equal(rows[3].key, 'src/lib/locales/')
})

test('closed folders hide their children but still report every file below', () => {
  const rows = treeRows(files('a/b/one.txt', 'a/two.txt', 'three.txt'), new Set(['a/']))
  assert.deepEqual(brief(rows), ['a/', 'three.txt'])
  assert.equal(rows[0].open, false)
  assert.equal(rows[0].items.length, 2)
})

test('an untracked nested repository listed with a trailing slash stays one file row', () => {
  const rows = treeRows(files('vendor/tool/'))
  assert.deepEqual(brief(rows), ['vendor/', '  tool'])
  assert.equal(rows[1].item.path, 'vendor/tool/')
})
