import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { test } from 'node:test'
import { compileModule } from 'svelte/compiler'
import ts from 'typescript'
import { messages as en, explanations as enHelp } from '../src/lib/locales/en.ts'
import { messages as zh, explanations as zhHelp } from '../src/lib/locales/zh.ts'

test('language packs cover the same UI, errors and operation explanations', () => {
  const compare = (a, b) => {
    assert.deepEqual(Object.keys(a).sort(), Object.keys(b).sort())
    for (const key of Object.keys(a)) {
      assert.equal(typeof a[key], typeof b[key], key)
      if (typeof a[key] === 'object') compare(a[key], b[key])
      else if (typeof a[key] === 'string') {
        assert.ok(a[key].length && b[key].length, key)
        assert.doesNotMatch(a[key], /\p{Script=Han}/u, key)
      } else if (typeof a[key] === 'function') {
        assert.equal(a[key].length, b[key].length, key)
        assert.doesNotMatch(a[key](2, 'branch'), /undefined|\p{Script=Han}/u, key)
      }
    }
  }
  compare(en, zh)
  compare(enHelp, zhHelp)
  const sources = readdirSync(new URL('../src/lib/', import.meta.url)).filter(f => f.endsWith('.svelte'))
  for (const file of sources) {
    const source = readFileSync(new URL(`../src/lib/${file}`, import.meta.url), 'utf8')
    assert.doesNotMatch(source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*|<!--[\s\S]*?-->/g, ''), /\p{Script=Han}/u, file)
  }
})

test('language defaults to English, switches without resetting state, and survives reload', async () => {
  const storage = new Map()
  const listeners = []
  globalThis.addEventListener = (type, listener) => { if (type === 'storage') listeners.push(listener) }
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: {
    getItem: key => storage.get(key) ?? null,
    setItem: (key, value) => storage.set(key, value),
  } })
  Object.defineProperty(globalThis, 'document', { configurable: true, value: { documentElement: { lang: '' } } })
  const source = readFileSync(new URL('../src/lib/i18n.svelte.ts', import.meta.url), 'utf8')
  const js = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText
  const compiled = compileModule(js, { filename: 'i18n.svelte.js', generate: 'client' }).js.code
    .replace(/(['"])svelte\/internal\/client\1/g, JSON.stringify(import.meta.resolve('svelte/internal/client')))
    .replace(/(['"])(\.\/[^'"]+)\1/g, (_, quote, path) => JSON.stringify(new URL(`../src/lib/${path.slice(2)}.ts`, import.meta.url).href))
  let generation = 0
  const load = () => import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}#${generation++}`)
  const first = await load()
  assert.equal(first.locale.current, 'en')
  assert.equal(first.t.push, 'Push')
  assert.equal(first.errorText('PR_FILE_CHANGED'), en.errors.PR_FILE_CHANGED)
  const ref = first.t
  first.setLocale('zh-CN')
  assert.equal(first.t, ref)
  assert.equal(first.t.push, '推送（git push）')
  assert.equal(first.explain.push.name, '推送（git push）')
  assert.equal(document.documentElement.lang, 'zh-CN')
  assert.equal(first.errorText('PR_INVALID_NAME: "--flag"'), '名称不合法: "--flag"')
  assert.equal(first.errorText('fatal: 原始 Git 输出'), 'fatal: 原始 Git 输出')
  assert.equal(first.errorText('constructor'), 'constructor')
  const restored = await load()
  assert.equal(restored.locale.current, 'zh-CN')
  restored.setLocale('en')
  assert.equal((await load()).t.push, 'Push')
  storage.set('locale', 'unsupported')
  assert.equal((await load()).locale.current, 'en')

  const instant = Date.parse('2026-01-01T00:15:00Z') / 1000
  assert.equal(first.timeZone.current, '')
  first.setTimeZone('Asia/Shanghai')
  assert.equal(storage.get('timeZone'), 'Asia/Shanghai')
  assert.equal(first.fmtTime(instant), '2026-01-01 08:15')
  assert.equal((await load()).fmtTime(instant), '2026-01-01 08:15')
  storage.set('timeZone', 'America/Los_Angeles')
  for (const listener of listeners) listener({ key: 'timeZone' })
  assert.equal(first.fmtTime(instant), '2025-12-31 16:15', 'open views receive host and cross-window changes')
  first.setTimeZone('')
  assert.equal((await load()).timeZone.current, '', 'system selection also survives reload')
  storage.set('timeZone', 'invalid')
  assert.equal((await load()).timeZone.current, '')
  delete globalThis.addEventListener
})

test('Chinese Git labels cover actions outside menus and distinguish commands with different effects', () => {
  const cases = [
    [zh.fetch, 'git fetch'], [zh.stageAll, 'git add -A'], [zh.unstageAll, 'git reset'],
    [zh.stageLines(2), 'git apply --cached'], [zh.unstageLines(2), 'git apply --cached --reverse'],
    [zh.discardAll, 'git stash push --include-untracked'], [zh.discardLinesConfirm, 'git apply --reverse'],
    [zh.resetSoft, 'git reset --soft'], [zh.resetHard, 'git reset --hard'], [zh.amend, 'git commit --amend'],
    [zh.rename, 'git branch -m'], [zh.tools.renameRemote, 'git remote rename'],
    [zh.delete, 'git branch -d'], [zh.tools.deleteRemote, 'git remote remove'],
    [zh.tools.editPush, 'git remote set-url --push'], [zh.tools.applyPlan, 'git rebase -i'],
    [zh.forcePush, 'git push --force-with-lease'], [zh.newWorktree, 'git worktree add'],
    [zh.continueOperation('revert'), 'git revert --continue'], [zh.abortOperation('revert'), 'git revert --abort'],
    [zh.vscode.searchKinds.content, 'git log -S'], [zh.vscode.untrackFile, 'git rm --cached'],
  ]
  for (const [label, command] of cases) assert.ok(label.includes(command), `${label}: ${command}`)
  const english = JSON.parse(readFileSync(new URL('../extension/package.nls.json', import.meta.url), 'utf8'))
  const chinese = JSON.parse(readFileSync(new URL('../extension/package.nls.zh-cn.json', import.meta.url), 'utf8'))
  assert.deepEqual(Object.keys(english).sort(), Object.keys(chinese).sort())
  for (const key of ['commit', 'checkout', 'stageAll', 'fileHistory', 'compareWith', 'repositorySettings']) {
    assert.match(chinese[`command.${key}`], /（git .+）$/)
    assert.doesNotMatch(english[`command.${key}`], /（git /)
  }
  assert.doesNotMatch(zh.copyId, /git /, 'pure UI actions must not invent a Git command')
})
