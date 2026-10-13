import assert from 'node:assert/strict'
import { test } from 'node:test'
import { fileIcon, GLYPHS } from '../src/lib/file-icons.ts'

test('file icons tell languages and formats apart and fall back to a plain file', () => {
  const same = (a, b) => JSON.stringify(fileIcon(a)) === JSON.stringify(fileIcon(b))
  assert.ok(!same('src/Weapon.luau', 'README.md'), 'Luau source and Markdown must not share an icon')
  assert.ok(!same('a.luau', 'a.lua') && !same('a.ts', 'a.js') && !same('a.json', 'a.toml') && !same('a.png', 'a.svg'))
  assert.deepEqual(fileIcon('Config/武器[1]CONFIG.LUAU'), fileIcon('x.luau'), 'extensions match case-insensitively')
  assert.equal(fileIcon('place.rbxl').glyph, 'roblox')
  assert.equal(fileIcon('dir.with.dots/.gitignore').glyph, 'git')
  assert.equal(fileIcon('Dockerfile').glyph, 'box')
  for (const path of ['noext', 'weird.zzz', 'untracked dir/']) assert.equal(fileIcon(path).glyph, 'file')
  for (const path of ['a.luau', 'a.md', 'a.zzz', 'a.lock']) assert.ok(GLYPHS[fileIcon(path).glyph] && fileIcon(path).color)
})
