import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { startEngine } from '../extension/src/engine.ts'
import { isUncommitted, runBlame } from '../extension/src/git-blame.ts'
import { splitUpstream } from '../src/lib/push-target.ts'

/** 两位作者各提交一次的临时仓库：a.txt 三行，第三行由 bob 改过，之后改名为 b.txt */
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), 'pushright-ext-'))
  const git = (...args) => execFileSync('git', ['-C', dir, '-c', 'commit.gpgsign=false', '-c', 'core.autocrlf=false', ...args])
  git('init', '-q', '-b', 'main')
  git('config', 'user.name', 'carol')
  git('config', 'user.email', 'carol@example.com')
  writeFileSync(join(dir, 'a.txt'), 'one\ntwo\nthree\n')
  git('add', '.')
  git('-c', 'user.name=alice', '-c', 'user.email=alice@example.com', 'commit', '-q', '-m', 'add file')
  writeFileSync(join(dir, 'a.txt'), 'one\ntwo\nTHREE\n')
  git('-c', 'user.name=bob', '-c', 'user.email=bob@example.com', 'commit', '-q', '-am', 'shout three')
  git('mv', 'a.txt', 'b.txt')
  git('-c', 'user.name=bob', '-c', 'user.email=bob@example.com', 'commit', '-q', '-m', 'rename')
  return dir
}

const blame = (dir, path, contents, rev) => new Promise((resolve, reject) => runBlame(dir, path, contents, (b) => b.done && resolve(b), rev, (message) => reject(new Error(message))))

test('push target: upstream is split by known remotes, including remotes with slashes', () => {
  assert.deepEqual(splitUpstream('refs/remotes/origin/feature/login', ['origin']), { remote: 'origin', branch: 'feature/login' })
  // 本地 feature/login 误跟踪 origin/main：拆出来的分支名与本地不同，调用方据此要求明确选择
  assert.deepEqual(splitUpstream('refs/remotes/origin/main', ['origin']), { remote: 'origin', branch: 'main' })
  assert.deepEqual(splitUpstream('refs/remotes/team/fork/main', ['team', 'team/fork']), { remote: 'team', branch: 'fork/main' })
  assert.deepEqual(splitUpstream('refs/remotes/team/fork/main', ['team/fork']), { remote: 'team/fork', branch: 'main' })
  assert.equal(splitUpstream(null, ['origin']), null)
  assert.equal(splitUpstream('refs/remotes/gone/main', ['origin']), null)
})

test('blame follows renames, keeps authors apart and marks unsaved lines as uncommitted', async () => {
  const dir = fixture()
  const saved = await blame(dir, 'b.txt')
  assert.deepEqual(saved.lines.map((c) => [c.author, c.author_email, c.subject, c.path]), [
    ['alice', 'alice@example.com', 'add file', 'a.txt'],
    ['alice', 'alice@example.com', 'add file', 'a.txt'],
    ['bob', 'bob@example.com', 'shout three', 'a.txt'],
  ])
  assert.equal(saved.lines[0], saved.lines[1])
  assert.match(saved.lines[2].previous, /^[0-9a-f]{40} a\.txt$/)
  assert.equal(saved.lines[0].previous, undefined)
  assert.ok(saved.lines[0].time > 1_600_000_000)

  // 编辑器里插入了一行还没保存：行号按未保存的内容算
  const unsaved = await blame(dir, 'b.txt', 'one\nnew line\ntwo\nTHREE\n')
  assert.deepEqual(unsaved.lines.map((c) => (isUncommitted(c) ? null : c.author)), ['alice', null, 'alice', 'bob'])
})

test('blame can be cancelled and reports nothing for untracked files', async () => {
  const dir = fixture()
  let calls = 0
  runBlame(dir, 'b.txt', undefined, () => calls++)()
  writeFileSync(join(dir, 'untracked.txt'), 'x\n')
  runBlame(dir, 'untracked.txt', undefined, () => calls++)
  runBlame(dir, 'b.txt', undefined, () => calls++, '--help')
  await new Promise((resolve) => setTimeout(resolve, 1500))
  assert.equal(calls, 0)
})

test('diff blame uses each historical side, including deleted paths and added or removed lines', { timeout: 10000 }, async () => {
  const dir = fixture()
  const git = (...args) => execFileSync('git', ['-C', dir, '-c', 'commit.gpgsign=false', ...args], { encoding: 'utf8' }).trim()
  const before = git('rev-parse', 'HEAD')
  writeFileSync(join(dir, 'b.txt'), 'one\nnew line\nTHREE\n')
  git('commit', '-q', '-am', 'replace two')
  const after = git('rev-parse', 'HEAD')
  git('rm', '-q', 'b.txt')
  git('commit', '-q', '-m', 'delete file')

  const left = await blame(dir, 'b.txt', undefined, before)
  const right = await blame(dir, 'b.txt', undefined, after)
  assert.deepEqual(left.lines.map((c) => c.subject), ['add file', 'add file', 'shout three'])
  assert.deepEqual(right.lines.map((c) => c.subject), ['add file', 'replace two', 'shout three'])
  assert.equal(right.lines[1].id, after, 'added line belongs to the compared commit, not current HEAD')
  const renamed = await blame(dir, 'a.txt', undefined, `${before}~1`)
  assert.deepEqual(renamed.lines.map((c) => c.id), left.lines.map((c) => c.id))
})

test('index blame uses the displayed snapshot rather than later working-tree or unsaved edits', { timeout: 10000 }, async () => {
  const dir = fixture()
  const indexText = 'one\nstaged line\ntwo\nTHREE\n'
  writeFileSync(join(dir, 'b.txt'), indexText)
  execFileSync('git', ['-C', dir, 'add', 'b.txt'])
  writeFileSync(join(dir, 'b.txt'), 'disk edit\n')
  const index = await blame(dir, 'b.txt', indexText)
  assert.deepEqual(index.lines.map((c) => isUncommitted(c) ? null : c.author), ['alice', null, 'alice', 'bob'])
  const unsaved = await blame(dir, 'b.txt', 'one\nunsaved line\nTHREE\n')
  assert.deepEqual(unsaved.lines.map((c) => isUncommitted(c) ? null : c.author), ['alice', null, 'bob'])
  writeFileSync(join(dir, 'new.txt'), 'new file\n')
  execFileSync('git', ['-C', dir, 'add', 'new.txt'])
  const added = await blame(dir, 'new.txt', 'new file\n')
  assert.equal(added.lines.length, 1)
  assert.ok(isUncommitted(added.lines[0]), 'a newly staged file is also annotated')
})

test('blame preserves Unicode paths for opening historical files', async () => {
  const dir = fixture()
  const git = (...args) => execFileSync('git', ['-C', dir, '-c', 'commit.gpgsign=false', ...args])
  git('mv', 'b.txt', '新文件.txt')
  writeFileSync(join(dir, '新文件.txt'), 'ONE\ntwo\nTHREE\n')
  git('commit', '-q', '-am', 'Unicode path')
  const result = await blame(dir, '新文件.txt')
  assert.equal(result.lines[0].path, '新文件.txt')
})

test('Windows blame resolves stale editor casing against the index and each historical revision', { skip: process.platform !== 'win32', timeout: 15000 }, async () => {
  const dir = fixture()
  const git = (...args) => execFileSync('git', ['-C', dir, '-c', 'commit.gpgsign=false', ...args], { encoding: 'utf8' }).trim()
  mkdirSync(join(dir, 'Config'))
  git('mv', 'b.txt', 'Config/武器[1]Config.luau')
  git('commit', '-q', '-m', 'move config')
  const before = git('rev-parse', 'HEAD')
  const stale = 'config/武器[1]config.luau'
  const saved = await blame(dir, stale)
  assert.deepEqual(saved.lines.map((c) => c.author), ['alice', 'alice', 'bob'])
  const snapshot = await blame(dir, stale, 'one\nunsaved\nTHREE\n', '')
  assert.deepEqual(snapshot.lines.map((c) => isUncommitted(c) ? null : c.author), ['alice', null, 'bob'])

  git('mv', '-f', 'Config/武器[1]Config.luau', 'Config/武器[1]CONFIG.luau')
  writeFileSync(join(dir, 'Config/武器[1]CONFIG.luau'), 'one\nnew config\nTHREE\n')
  git('commit', '-q', '-am', 'rename case and edit')
  const after = git('rev-parse', 'HEAD')
  git('rm', '-q', 'Config/武器[1]CONFIG.luau')
  git('commit', '-q', '-m', 'remove config')
  const left = await blame(dir, stale, undefined, before)
  const right = await blame(dir, stale, undefined, after)
  assert.deepEqual(left.lines.map((c) => c.id), saved.lines.map((c) => c.id))
  assert.equal(right.lines[1].id, after)
  assert.equal(right.lines[1].path, 'Config/武器[1]CONFIG.luau', 'historical spelling survives even when the file no longer exists')
})

test('blame reports Git failures and does not guess between case-colliding historical paths', { timeout: 10000 }, async () => {
  const dir = fixture()
  await assert.rejects(blame(dir, 'missing.txt'), /git blame.*missing\.txt: fatal:/)
  await assert.rejects(blame(dir, 'b.txt', undefined, 'missing-revision'), /git blame.*missing-revision.*fatal:/)
  if (process.platform !== 'win32') return
  const git = (...args) => execFileSync('git', ['-C', dir, '-c', 'commit.gpgsign=false', ...args], { encoding: 'utf8' }).trim()
  const blob = git('rev-parse', 'HEAD:b.txt')
  git('update-index', '--add', '--cacheinfo', `100644,${blob},Mixed.txt`)
  git('update-index', '--add', '--cacheinfo', `100644,${blob},mixed.txt`)
  git('commit', '-q', '-m', 'case collision')
  await assert.rejects(blame(dir, 'MIXED.txt', undefined, 'HEAD'), /no such path/)
  const exact = await blame(dir, 'Mixed.txt', undefined, 'HEAD')
  assert.equal(exact.lines.length, 3, 'an exact match remains usable')
})

const exe = join(import.meta.dirname, '../target/release/pushright-engine.exe')

test('sidecar answers the same commands as the desktop backend, plus history', { skip: !existsSync(exe) && 'run `cargo build --release -p sidecar` first' }, async () => {
  const dir = fixture()
  const engine = startEngine(exe)
  try {
    const [tab, path, count] = await engine.call('open_repo', { path: dir })
    assert.equal(count, 3)
    assert.ok(existsSync(join(path, 'b.txt')))
    const [rows, refs, history] = await Promise.all([
      engine.call('rows', { tab, start: 0, count: 10 }),
      engine.call('refs', { tab }),
      engine.call('file_history', { tab, path: 'b.txt', limit: 10 }),
    ])
    assert.deepEqual(rows.map((r) => r.subject), ['rename', 'shout three', 'add file'])
    assert.equal(refs.head, 'refs/heads/main')
    assert.equal(refs.revert_disabled, false)
    assert.equal((await engine.call('op', { tab, op: { op: 'fetch' } })).ok, true, 'existing fetch callers remain compatible without the background flag')
    const [secondTab] = await engine.call('open_repo', { path: dir })
    assert.equal((await engine.call('op', { tab, op: { op: 'set_revert_disabled', disabled: true } })).ok, true)
    assert.equal((await engine.call('refs', { tab: secondTab })).revert_disabled, true)
    await assert.rejects(engine.call('op', { tab: secondTab, op: { op: 'revert', id: 'HEAD' } }), /PR_REVERT_DISABLED/)
    assert.equal((await engine.call('op', { tab: secondTab, op: { op: 'set_revert_disabled', disabled: false } })).ok, true)
    assert.equal((await engine.call('refs', { tab })).revert_disabled, false)
    await engine.call('close_repo', { tab: secondTab })
    assert.deepEqual(history.map((c) => c.path), ['b.txt', 'a.txt', 'a.txt'])
    const stats = await engine.call('stats', { tab })
    assert.equal(stats.commits, 3)
    assert.deepEqual(stats.times, execFileSync('git', ['-C', dir, 'log', '--format=%at'], { encoding: 'utf8' }).trim().split(/\r?\n/).map(Number))
    const comparison = await engine.call('compare', { tab, left: history[2].id, right: 'HEAD', commonBase: false })
    assert.equal(comparison.right, history[0].id)
    assert.ok(comparison.files.some((f) => f.path === 'b.txt'))
    assert.ok((await engine.call('reflog', { tab, skip: 0, limit: 201 })).length)
    assert.deepEqual(await engine.call('remote_details', { tab }), [])
    assert.equal((await engine.call('rebase_plan', { tab, base: 'HEAD~1' })).steps.length, 1)
    assert.equal((await engine.call('file_history', { tab, path: 'b.txt', limit: 1, skip: 1 }))[0].path, 'a.txt')
    const mapped = await engine.call('line_history_page', { tab, path: 'b.txt', start: 4, end: 4, limit: 201, skip: 0, contents: 'new\none\ntwo\nTHREE\n' })
    assert.equal(mapped[0].subject, 'shout three')
    execFileSync('git', ['-C', dir, 'branch', 'older', history[1].id])
    assert.equal(await engine.call('load_graph', { tab, full: true, scope: 'refs/heads/older' }), 2)
    assert.deepEqual((await engine.call('rows', { tab, start: 0, count: 10 })).map((r) => r.subject), ['shout three', 'add file'])
    assert.equal(await engine.call('load_graph', { tab, full: true, scope: 'auto' }), 3)
    assert.equal(await engine.call('load_graph', { tab, full: true, scope: 'all' }), 3)
    await assert.rejects(engine.call('load_graph', { tab, full: true, scope: '--all' }), /PR_GRAPH_REF_GONE/)
    assert.equal(await engine.call('show', { tab, rev: history[2].id, path: 'a.txt' }), 'one\ntwo\nthree\n')
    assert.deepEqual((await engine.call('search', { tab, kind: 'author', query: 'alice', limit: 10 })).map((c) => c.subject), ['add file'])

    // 写操作走同一条通道；只读命令不改写索引
    writeFileSync(join(dir, 'b.txt'), 'one\ntwo\nTHREE\nfour\n')
    assert.deepEqual((await engine.call('status', { tab })).map((e) => [e.path, e.unstaged]), [['b.txt', 'M']])
    await engine.call('stage', { tab, paths: ['b.txt'] })
    await engine.call('commit', { tab, message: 'add four', amend: false })
    assert.deepEqual(await engine.call('status', { tab }), [])
    const log = await engine.call('op', { tab, op: { op: 'pull', rebase: true } })
    assert.match(log.command, /pull --rebase --no-autostash/)
    assert.equal(log.ok, false)

    await assert.rejects(engine.call('rows', { tab: 999, start: 0, count: 1 }), /PR_TAB_CLOSED/)
    await assert.rejects(engine.call('nonsense', { tab }), /unknown command/)
  } finally {
    engine.dispose()
  }
  await assert.rejects(engine.call('refs', { tab: 0 }), /PR_ENGINE_EXITED/)
})

test('editor author colors match the commit graph palette', () => {
  const css = readFileSync(new URL('../src/app.css', import.meta.url), 'utf8')
  const source = readFileSync(new URL('../extension/src/blame.ts', import.meta.url), 'utf8')
  const [dark, light] = css.split("[data-theme='light']")
  for (const [theme, block] of [['dark', dark], ['light', light]]) {
    const expected = [...block.matchAll(/--author-\d: (#[0-9a-f]{6})/g)].map((m) => m[1])
    assert.equal(expected.length, 10)
    assert.deepEqual(new RegExp(`${theme}: \\[(.*?)\\]`).exec(source)[1].match(/#[0-9a-f]{6}/g), expected)
  }
})
