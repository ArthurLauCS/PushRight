import assert from 'node:assert/strict'
import { test } from 'node:test'
import { highlightHunk, languageOf, splitHtml } from '../src/lib/highlight.ts'

const plain = (html) => html.replace(/<[^>]+>/g, '').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#x27;/g, "'").replace(/&amp;/g, '&')

test('diff highlighting picks the language by extension and keeps every line aligned', () => {
  assert.equal(languageOf('src/Weapon.luau'), 'luau')
  assert.equal(languageOf('a/b/App.svelte'), 'xml')
  assert.equal(languageOf('Cargo.toml'), 'ini')
  assert.equal(languageOf('x.TS'), 'ts')
  for (const path of ['notes.unknownext', 'LICENSE', 'dir/']) assert.equal(languageOf(path), null)

  // 跨行的 span 每行各自闭合，拼回去内容不变
  assert.deepEqual(splitHtml('<span class="a">x\n<span class="b">y</span>\nz</span>!'), ['<span class="a">x</span>', '<span class="a"><span class="b">y</span></span>', '<span class="a">z</span>!'])

  const lines = [
    { kind: ' ', text: 'local a = 1 --[[ open' },
    { kind: '-', text: 'still comment ]] local old = "<b>"' },
    { kind: '+', text: 'still comment ]] continue' },
    { kind: '\\', text: ' No newline at end of file' },
    { kind: ' ', text: 'return a & 1' },
  ]
  const html = highlightHunk(lines, 'luau')
  assert.equal(html.length, lines.length)
  lines.forEach((l, i) => l.kind !== '\\' && assert.equal(plain(html[i]), l.text, `line ${i}`))
  assert.match(html[0], /hljs-keyword">local/)
  assert.match(html[1], /^<span class="hljs-comment">still comment \]\]<\/span>/, 'the old side continues the multi-line comment')
  assert.match(html[2], /hljs-keyword">continue/, 'Luau keywords are recognised')
  assert.doesNotMatch(html[1], /<b>/, 'file content is escaped')
  for (const line of html) assert.equal((line.match(/<span/g) ?? []).length, (line.match(/<\/span>/g) ?? []).length)

  assert.equal(highlightHunk(lines, null), null)
  assert.equal(highlightHunk([{ kind: '+', text: 'x'.repeat(2001) }], 'luau'), null, 'minified lines are left plain')
})
