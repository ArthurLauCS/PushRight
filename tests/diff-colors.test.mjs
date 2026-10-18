import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import ts from 'typescript'
import { compileModule } from 'svelte/compiler'
import * as palettes from '../src/lib/diff-colors.ts'

function host() {
  const state = new Map(), web = new Map(), posted = [], commands = new Map()
  let current = {}, fail = false
  const context = { subscriptions: [], globalState: { get: (key) => state.get(key), update: async (key, value) => state.set(key, value) } }
  const vscode = {
    ConfigurationTarget: { Global: 1 }, commands: { registerCommand: (key, fn) => commands.set(key, fn) }, window: {},
    workspace: { getConfiguration: () => ({ inspect: () => ({ globalValue: current }), update: async (key, value, target) => {
      assert.equal(key, 'colorCustomizations'); assert.equal(target, 1)
      if (fail) throw new Error('read-only settings')
      current = value ?? {}
    } }) },
  }
  const core = { store: async (key, value) => web.set(key, value), stored: () => Object.fromEntries(web), t: { diffColors: { invalid: 'Invalid HEX' } } }
  const deps = { vscode, './core': core, '../../src/lib/diff-colors': palettes }
  const source = readFileSync(new URL('../extension/src/diff-colors.ts', import.meta.url), 'utf8')
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2023 } }).outputText
  const exports = {}
  new Function('require', 'exports', js)((id) => { assert.ok(deps[id], id); return deps[id] }, exports)
  return { ...exports, context, state, web, posted, commands, vscode, get colors() { return current }, set colors(value) { current = value }, set fail(value) { fail = value } }
}

test('diff palettes validate persisted and incoming colors', () => {
  for (const preset of Object.values(palettes.DIFF_PRESETS)) assert.deepEqual(palettes.readDiffColors(JSON.stringify(preset)), preset)
  for (const invalid of [null, '', '{broken', 'null', '[]', '{"added":"red","deleted":"#123456"}', '{"added":"#123456"}']) assert.equal(palettes.readDiffColors(invalid), null)
  assert.equal(palettes.validDiffColors({ added: '#AbC012', deleted: '#123456' }), true)
})

test('native diff presets restore original root and theme colors across repeated changes without clobbering manual edits', async () => {
  const h = host(), key = 'diffEditor.insertedLineBackground', deleted = 'diffEditor.removedLineBackground'
  const original = { 'editor.background': '#101010', [key]: '#12345622', '[Dark][Light]': { [deleted]: '#65432133', 'editor.foreground': '#eeeeee' } }
  h.colors = structuredClone(original)
  await h.applyDiffColors(h.context, palettes.DIFF_PRESETS.blueOrange)
  assert.equal(h.colors[key], '#0072b238')
  assert.equal(h.colors['[Dark][Light]'][deleted], '#e69f0038')
  await h.applyDiffColors(h.context, palettes.DIFF_PRESETS.purpleGold)
  await h.applyDiffColors(h.context, null)
  assert.deepEqual(h.colors, original)
  assert.equal(h.state.get('diffColorBackup'), undefined)
  assert.equal(h.web.get('diffColors'), 'null')
  await h.applyDiffColors(h.context, palettes.DIFF_PRESETS.cyanRose)
  h.colors[key] = '#ffffff22'
  h.colors['[Dark][Light]'][deleted] = '#ababab33'
  await h.applyDiffColors(h.context, palettes.DIFF_PRESETS.blueOrange)
  await h.applyDiffColors(h.context, null)
  assert.equal(h.colors[key], '#ffffff22', 'manual edits become the new restoration baseline')
  assert.equal(h.colors['[Dark][Light]'][deleted], '#ababab33')
  await h.applyDiffColors(h.context, palettes.DIFF_PRESETS.blueOrange)
  h.colors[key] = '#abcdef11'
  await h.applyDiffColors(h.context, null)
  assert.equal(h.colors[key], '#abcdef11', 'reset preserves a later manual edit')
})

test('failed writes retain the original backup and do not publish a successful color change', async () => {
  const h = host()
  await assert.rejects(h.applyDiffColors(h.context, { added: 'red', deleted: '#123456' }), /Invalid HEX/)
  assert.equal(h.state.size, 0)
  await h.applyDiffColors(h.context, palettes.DIFF_PRESETS.blueOrange)
  const backup = structuredClone(h.state.get('diffColorBackup')), current = structuredClone(h.colors), count = h.posted.length
  h.fail = true
  for (const colors of [palettes.DIFF_PRESETS.purpleGold, null]) {
    await assert.rejects(h.applyDiffColors(h.context, colors), /read-only settings/)
    assert.deepEqual(h.state.get('diffColorBackup'), backup)
    assert.deepEqual(h.colors, current)
    assert.equal(h.posted.length, count)
  }
  h.fail = false
  await h.applyDiffColors(h.context, null)
  assert.deepEqual(h.colors, {})
})

test('native command supports presets and custom colors, and cancellation leaves settings alone', async () => {
  const h = host()
  h.registerDiffColors(h.context)
  const command = h.commands.get('pushright.diffColors')
  h.vscode.window.showQuickPick = async (items) => items[0]
  await command()
  assert.deepEqual(JSON.parse(h.web.get('diffColors')), palettes.DIFF_PRESETS.blueOrange)
  h.vscode.window.showQuickPick = async (items) => items.find((i) => i.key === 'custom')
  let inputs = ['#123456', undefined]
  h.vscode.window.showInputBox = async (options) => { assert.ok(options.validateInput('red')); assert.equal(options.validateInput('#abcdef'), null); return inputs.shift() }
  const count = h.posted.length
  await command()
  assert.equal(h.posted.length, count)
  inputs = ['#123456', '#abcdef']
  await command()
  assert.deepEqual(JSON.parse(h.web.get('diffColors')), { added: '#123456', deleted: '#abcdef' })
})

test('shared diff preferences survive reloads, synchronize storage events and fall back on malformed data', async () => {
  const storage = new Map([['prefs', '{"authorStyles":{}}']]), styles = new Map(), listeners = []
  globalThis.localStorage = { getItem: (key) => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) }
  globalThis.document = { documentElement: { style: { setProperty: (key, value) => styles.set(key, value), removeProperty: (key) => styles.delete(key) } } }
  globalThis.addEventListener = (_, fn) => listeners.push(fn)
  const source = readFileSync(new URL('../src/lib/diff-colors.svelte.ts', import.meta.url), 'utf8')
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ESNext } }).outputText
  const compiled = compileModule(js, { filename: 'diff-colors.svelte.js', generate: 'client' }).js.code
    .replace(/(['"])svelte\/internal\/client\1/g, JSON.stringify(import.meta.resolve('svelte/internal/client')))
    .replace(/(['"])\.\/diff-colors\1/g, JSON.stringify(new URL('../src/lib/diff-colors.ts', import.meta.url).href))
  let generation = 0
  const load = () => import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}#${generation++}`)
  try {
    const first = await load()
    first.setDiffColors(palettes.DIFF_PRESETS.blueOrange)
    assert.deepEqual((await load()).diffColors.current, palettes.DIFF_PRESETS.blueOrange)
    assert.equal(styles.get('--diff-added'), '#0072b2')
    assert.equal(storage.get('prefs'), '{"authorStyles":{}}')
    listeners[0]({ key: 'diffColors', newValue: JSON.stringify(palettes.DIFF_PRESETS.cyanRose) })
    assert.equal(styles.get('--diff-deleted'), '#cc79a7')
    first.setDiffColors(null)
    assert.equal(styles.size, 0)
    storage.set('diffColors', '{bad')
    assert.equal((await load()).diffColors.current, null)
  } finally {
    delete globalThis.localStorage; delete globalThis.document; delete globalThis.addEventListener
  }
})
