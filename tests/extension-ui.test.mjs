import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { join, resolve } from 'node:path'
import { test } from 'node:test'
import ts from 'typescript'

// Exercise host handlers without launching VS Code; the smoke test covers the actual host.
const require = createRequire(import.meta.url)
function load(name, deps) {
  const source = readFileSync(new URL(`../extension/src/${name}.ts`, import.meta.url), 'utf8')
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2023 } }).outputText
  const exports = {}
  new Function('require', 'exports', js)((id) => deps[id] ?? require(id), exports)
  return exports
}
class Event {
  listeners = []
  event = (fn) => { this.listeners.push(fn); return { dispose: () => this.listeners = this.listeners.filter((x) => x !== fn) } }
  fire = (value) => this.listeners.map((fn) => fn(value))
  dispose() {}
}
const deferred = () => { let resolve; const promise = new Promise((r) => resolve = r); return { promise, resolve } }
const uri = (fsPath) => ({ fsPath, scheme: 'file', toString() { return `${this.scheme}:${this.fsPath}?${this.query ?? ''}` }, with: (other) => ({ ...uri(fsPath), ...other }) })

test('diff editors show current-line blame on both sides and reload inactive index snapshots', () => {
  const active = new Event(), visible = new Event(), selection = new Event(), changed = new Event(), closed = new Event()
  const repoChanged = new Event(), prefs = new Event(), theme = new Event(), config = new Event()
  const repo = { root: '/repo', refs: { head_id: 'head' } }, requests = [], types = []
  const status = { show() {}, hide() {} }
  const document = (rev, text = 'unchanged\nchanged\n', empty = false) => ({
    uri: rev === undefined ? uri('/repo/a.txt') : uri('/repo/a.txt').with({ scheme: 'pushright-rev', query: JSON.stringify({ rev, empty }) }),
    version: 1, isDirty: false, lineCount: 3, getText: () => text,
  })
  const editor = (doc) => ({ document: doc, selection: { active: { line: 1 } }, decorations: new Map(), setDecorations(type, value) { this.decorations.set(type, value) } })
  const left = editor(document('before')), right = editor(document('after'))
  const vscode = {
    EventEmitter: Event, Range: class { constructor(line) { this.start = { line } } }, ThemeColor: class {},
    MarkdownString: class { appendMarkdown() {} appendText() {} },
    StatusBarAlignment: { Right: 1 }, SymbolKind: {},
    Uri: { parse: (key) => ({ toString: () => key }) },
    window: {
      visibleTextEditors: [], activeTextEditor: undefined,
      createTextEditorDecorationType: () => { const type = {}; types.push(type); return type }, createStatusBarItem: () => status,
      onDidChangeActiveTextEditor: active.event, onDidChangeVisibleTextEditors: visible.event,
      onDidChangeTextEditorSelection: selection.event, onDidChangeActiveColorTheme: theme.event,
    },
    workspace: { onDidChangeTextDocument: changed.event, onDidCloseTextDocument: closed.event, onDidChangeConfiguration: config.event },
    languages: { registerCodeLensProvider() {} }, commands: { registerCommand() {} },
  }
  const commits = {
    original: { id: 'a'.repeat(40), author: 'alice', author_email: 'alice@example.com', subject: 'original', time: 1, path: 'a.txt' },
    added: { id: 'b'.repeat(40), author: 'bob', author_email: 'bob@example.com', subject: 'added', time: 2, path: 'a.txt' },
    pending: { id: '0'.repeat(40), author: '', subject: '', time: 0, path: 'a.txt' },
  }
  const core = {
    REV: 'pushright-rev', revOf: (u) => u.scheme === 'pushright-rev' ? JSON.parse(u.query).rev : undefined,
    repoOf: () => repo, rel: () => 'a.txt', repos: [repo], onRepoChange: repoChanged, onPrefsChange: prefs,
    cfg: (key) => key === 'blame.line.format' ? '${author}: ${subject}' : true,
    authorStyles: () => ({}), ago: () => 'ago', date: () => 'date', v: { uncommitted: 'Uncommitted' }, t: { authorStyle: () => 'Style' },
  }
  const { registerBlame } = load('blame', {
    vscode, './core': core, '../../src/lib/author': { authorKey: (c) => c.author_email },
    './git-blame': {
      isUncommitted: (c) => /^0+$/.test(c.id),
      runBlame: (root, path, contents, progress, rev) => {
        const request = { root, path, contents, rev, cancelled: false }
        requests.push(request)
        progress({ done: true, lines: [commits.original, rev === 'before' ? commits.original : rev === 'after' ? commits.added : commits.pending] })
        return () => { request.cancelled = true }
      },
    },
  })
  registerBlame({ subscriptions: [] })
  const show = (...editors) => { vscode.window.visibleTextEditors = editors; visible.fire(editors) }
  const annotation = (e) => e.decorations.get(types[0])[0]?.renderOptions.after.contentText
  vscode.window.activeTextEditor = right
  show(left, right)
  assert.deepEqual(requests.map((r) => [r.rev, r.contents]), [['before', undefined], ['after', undefined]])
  assert.equal(annotation(left), 'alice: original', 'deleted line keeps its original commit')
  assert.equal(annotation(right), 'bob: added', 'added line uses the right-hand commit')
  left.selection.active.line = 0
  selection.fire({ textEditor: left })
  assert.equal(annotation(left), 'alice: original', 'unchanged line is also annotated')
  const reopened = editor(left.document)
  show(reopened, right)
  assert.equal(requests.length, 2, 'a new editor reuses the document cache')
  assert.equal(annotation(reopened), 'alice: original', 'cached blame is rendered in the new editor')

  const index = editor(document('')), working = editor(document(undefined, 'unsaved\n'))
  working.document.isDirty = true
  vscode.window.activeTextEditor = working
  show(index, working)
  assert.deepEqual(requests.slice(-2).map((r) => [r.rev, r.contents]), [['', 'unchanged\nchanged\n'], [undefined, 'unsaved\n']])
  assert.equal(annotation(index), 'Uncommitted')
  assert.equal(annotation(working), 'Uncommitted')
  index.document.version++
  index.document.getText = () => 'updated index\n'
  changed.fire({ document: index.document, contentChanges: [{}] })
  assert.equal(requests.at(-1).contents, 'updated index\n', 'inactive index document is refreshed after staging')
  assert.equal(requests[2].cancelled, true)

  const blank = editor(document('after', '', true))
  show(blank)
  assert.equal(requests.length, 5, 'empty side of an added/deleted file does not run blame')
  assert.equal(annotation(blank), undefined)
  closed.fire(index.document)
  assert.equal(requests.at(-1).cancelled, true)
})

test('file-only refreshes read status alone and stay quiet until it changes', async () => {
  const calls = []
  let status = []
  const engine = { call: async (cmd) => { calls.push(cmd); return cmd === 'open_repo' ? [0, resolve('quiet-repo-test'), 1] : cmd === 'refs' ? { head_id: 'head', refs: [] } : cmd === 'status' ? status : [] } }
  const vscode = { EventEmitter: Event, env: { language: 'en' }, Uri: { file: uri }, window: { createOutputChannel: () => ({ appendLine() {} }) },
    workspace: { workspaceFolders: [{ uri: uri(resolve('quiet-repo-test')) }] }, commands: { executeCommand() {} } }
  const locale = { messages: { vscode: {} }, explanations: {} }
  const core = load('core', { vscode, './engine': { startEngine: () => engine }, '../../src/lib/locales/en': locale, '../../src/lib/locales/zh': locale })
  await core.openRepos({ workspaceState: { get: (_, fallback) => fallback, update: async () => {} } }, '')
  const [repo] = core.repos
  let fired = 0
  core.onRepoChange.event(() => fired++)
  calls.length = 0
  await core.refresh(repo, false)
  assert.deepEqual([calls, fired], [['status'], 0], 'saving a file whose status is unchanged redraws nothing')
  status = [{ path: 'a.txt', unstaged: 'M' }]
  await core.refresh(repo, false)
  assert.equal(fired, 1, 'a status change is announced')
  await core.refresh(repo, false, true)
  assert.equal(fired, 2, 'created and deleted files are announced even when status is unchanged')
  calls.length = 0
  await core.refresh(repo)
  assert.deepEqual([calls.sort(), fired], [['commit_identity', 'refs', 'remotes', 'status'], 3], 'a .git change rereads everything and always notifies')
  const reading = deferred(), resume = deferred(), call = engine.call
  let paused = false
  engine.call = async (cmd) => {
    if (cmd === 'status' && !paused) { paused = true; reading.resolve(); await resume.promise }
    return call(cmd)
  }
  calls.length = 0
  const pending = core.refresh(repo, false)
  await reading.promise
  await core.refresh(repo)
  await core.refresh(repo, false)
  resume.resolve()
  await pending
  assert.deepEqual(calls.sort(), ['commit_identity', 'refs', 'remotes', 'status', 'status'], 'a queued full refresh survives later file-only events')
  assert.equal(fired, 4, 'the queued full refresh notifies even when status is unchanged')
})

test('closed repositories stay closed across discovery, pending refreshes and reloads until explicitly reopened', async () => {
  const root = resolve('repo-close-test'), child = join(root, 'child'), external = resolve('external-repo-test')
  const saved = {}, sessions = new Map(), contexts = new Map()
  let next = 0, delayed
  const engine = { call: async (cmd, args) => {
    if (cmd === 'open_repo') { const tab = next++; sessions.set(tab, args.path); return [tab, args.path, 1] }
    if (cmd === 'close_repo') { sessions.delete(args.tab); return }
    if (cmd === 'repositories') return sessions.get(args.tab) === root ? [child] : []
    if (cmd === 'refs') return { head_id: 'head', refs: [] }
    if (cmd === 'status' && delayed) return delayed.promise
    return []
  } }
  const vscode = { EventEmitter: Event, env: { language: 'en' }, Uri: { file: uri },
    window: { createOutputChannel: () => ({ appendLine() {} }) },
    workspace: { workspaceFolders: [{ uri: uri(root) }] },
    commands: { executeCommand: (_, key, value) => contexts.set(key, value) },
  }
  const locale = { messages: { vscode: {} }, explanations: {} }
  const deps = { vscode, './engine': { startEngine: () => engine }, '../../src/lib/locales/en': locale, '../../src/lib/locales/zh': locale,
    'node:fs/promises': { readdir: async () => [], stat: async () => ({ isDirectory: () => false }) } }
  const context = { workspaceState: { get: (key, fallback) => saved[key] ?? fallback, update: async (key, value) => { saved[key] = value } } }
  const core = load('core', deps)
  await core.openRepos(context, '')
  await core.discoverRepositories([external])
  const repo = core.repos.find((r) => r.root === external)
  delayed = deferred()
  const refreshing = core.refresh(repo)
  const changes = []
  core.onRepoChange.event((r) => changes.push(r))
  await core.closeRepo(repo)
  assert.equal(sessions.has(repo.tab), false)
  assert.equal(core.repos.includes(repo), false)
  delayed.resolve([])
  await refreshing
  delayed = undefined
  assert.deepEqual(changes, [repo], 'late refresh cannot restore a closed repository')
  await core.discoverRepositories([external])
  await core.discoverForFile(uri(join(external, 'file.txt')))
  assert.equal(core.repos.some((r) => r.root === external), false)
  const nested = core.repos.find((r) => r.root === child)
  await core.closeRepo(nested)
  assert.equal(core.repoOf(uri(join(child, 'file.txt'))), undefined, 'closed child files cannot route commands to the parent')
  await core.discoverRepositories()
  assert.equal(core.repos.length, 1)
  const reloaded = load('core', deps)
  await reloaded.openRepos(context, '')
  await reloaded.discoverRepositories([external])
  assert.deepEqual(reloaded.repos.map((r) => r.root), [root])
  await reloaded.reopenRepo(external)
  assert.ok(reloaded.repos.some((r) => r.root === external))
  await reloaded.closeRepo(reloaded.repos.find((r) => r.root === external))
  await reloaded.closeRepo(reloaded.repos[0])
  assert.equal(contexts.get('pushright.hasRepo'), false)
  assert.equal(contexts.get('pushright.hasClosedRepos'), true)
})

test('SCM graph routes only bounded reads to workspace repositories and disposes listeners', async () => {
  let provider
  const messages = new Event(), changed = new Event(), prefs = new Event(), visibility = new Event(), closed = new Event()
  const posted = [], calls = [], diffs = []
  const repos = [0, 1].map((tab) => ({ tab, root: `/repo${tab}`, name: `repo${tab}`, refs: { refs: [] } }))
  const view = { visible: true, webview: { postMessage: (m) => posted.push(m), onDidReceiveMessage: messages.event }, onDidChangeVisibility: visibility.event, onDidDispose: closed.event }
  const detail = { id: 'commit', parents: ['parent', 'second-parent'], files: [{ status: 'R', path: 'new.txt', old_path: 'old.txt' }] }
  const core = { repos, onRepoChange: changed, onPrefsChange: prefs, revUri: (repo, path, rev, empty) => ({ root: repo.root, path, rev, empty }), engine: { call: async (...args) => { calls.push(args); return args[0] === 'detail' ? detail : [] } } }
  const vscode = { window: { registerWebviewViewProvider: (id, p) => { assert.equal(id, 'pushright.graph'); provider = p } }, commands: { registerCommand: () => ({ dispose() {} }), executeCommand: (...args) => diffs.push(args) } }
  const syncs = [], saved = {}
  let picked = 'all'
  load('graph-view', { vscode, './core': core, './sync': { pickScope: async () => picked, fetch: (r) => syncs.push(['fetch', r.tab]), pull: (r) => syncs.push(['pull', r.tab]), push: (r, same) => syncs.push(['push', r.tab, same]) }, './webview': { setupWebview: () => ({ dispose() {} }) } }).registerGraphView({ subscriptions: [], workspaceState: { get: (_, fallback) => fallback, update: (key, value) => saved[key] = value } })
  provider.resolveWebviewView(view)
  core.stored = () => ({ diffColors: '{"added":"#0072b2","deleted":"#e69f00"}' })
  prefs.fire()
  assert.deepEqual(posted.at(-1), { store: 'diffColors', value: core.stored().diffColors }, 'an open sidebar receives color changes')
  const send = async (cmd, args = {}) => { await Promise.all(messages.fire({ id: 1, cmd, args })); return posted.at(-1) }
  assert.deepEqual((await send('graph_repos')).value, repos.map((r) => ({ ...r, scope: 'auto' })))
  await send('rows', { tab: 1, start: 0, count: 128 })
  assert.deepEqual(calls, [['rows', { tab: 1, start: 0, count: 128 }]])
  assert.equal((await send('reveal_commit', { tab: 1, id: 'abc' })).ok, false)
  assert.equal((await send('rows', { tab: 9, start: 0, count: 128 })).ok, false)
  assert.equal((await send('rows', { tab: 1, start: 0, count: 100000 })).ok, false)
  assert.equal((await send('op', { tab: 1, op: { op: 'reset', mode: 'hard' } })).ok, false)
  assert.equal(calls.length, 1)
  assert.equal((await send('detail', { tab: 1, id: 'commit' })).value, detail)
  assert.deepEqual(diffs, [], 'expanding a commit never opens an editor')
  await send('open_commit_file', { tab: 1, id: 'commit', path: 'new.txt' })
  assert.deepEqual(diffs[0], ['vscode.diff', { root: '/repo1', path: 'old.txt', rev: 'parent', empty: false }, { root: '/repo1', path: 'new.txt', rev: 'commit', empty: false }, 'new.txt (commit)', { preview: true, preserveFocus: true }])
  for (const status of ['A', 'D']) {
    detail.files = [{ status, path: 'file.txt', old_path: null }]
    await send('open_commit_file', { tab: 0, id: 'commit', path: 'file.txt', preview: false })
    assert.equal(diffs.at(-1)[1].empty, status === 'A')
    assert.equal(diffs.at(-1)[2].empty, status === 'D')
    assert.deepEqual(diffs.at(-1)[4], { preview: false, preserveFocus: false })
  }
  detail.parents = []
  detail.files = [{ status: 'A', path: 'root.txt', old_path: null }]
  await send('open_commit_file', { tab: 0, id: 'commit', path: 'root.txt' })
  assert.equal(diffs.at(-1)[1].empty, true)
  assert.equal((await send('open_commit_file', { tab: 0, id: 'commit', path: '../../secret' })).ok, false)
  assert.equal(diffs.length, 4, 'paths absent from the commit cannot open')
  await send('graph_scope', { tab: 1, scope: '--all' })
  await send('load_graph', { tab: 1, full: false })
  assert.deepEqual(calls.at(-1), ['load_graph', { tab: 1, full: false, scope: 'all' }])
  assert.equal(saved.graphScopes['/repo1'], 'all')
  picked = undefined
  await send('graph_scope', { tab: 1 })
  assert.equal(saved.graphScopes['/repo1'], 'all', 'the host picker chooses the scope; cancelling keeps it')
  for (const action of ['fetch', 'pull', 'push']) await send('graph_sync', { tab: 1, action })
  assert.deepEqual(syncs, [['fetch', 1], ['pull', 1], ['push', 1, true]])
  assert.equal((await send('graph_sync', { tab: 1, action: 'reset' })).ok, false)
  changed.fire(repos[0]); assert.deepEqual(posted.at(-1), { refresh: true })
  view.visible = false
  const n = posted.length
  changed.fire(repos[0]); assert.equal(posted.length, n)
  closed.fire()
  assert.equal(changed.listeners.length + prefs.listeners.length + messages.listeners.length + visibility.listeners.length, 0)
})

test('graph keeps its document on theme changes and releases both loaded and late sessions', async () => {
  const messages = new Event(), closed = new Event(), theme = new Event()
  const posted = [], calls = []
  const late = deferred()
  const webview = { html: '', cspSource: 'self', asWebviewUri: (u) => u.fsPath, postMessage: (m) => posted.push(m), onDidReceiveMessage: messages.event }
  const panel = { webview, onDidDispose: closed.event }
  const vscode = {
    Uri: { file: uri, joinPath: (u, p) => uri(`${u.fsPath}/${p}`) }, ColorThemeKind: { Dark: 2, HighContrast: 3 }, ViewColumn: { Active: 1 },
    window: { createWebviewPanel: () => panel, activeColorTheme: { kind: 2 }, onDidChangeActiveColorTheme: theme.event },
  }
  const core = { engine: { call: async (cmd, args) => { calls.push([cmd, args]); return cmd === 'open_repo' ? args.path === 'late' ? late.promise : [7, 'repo', 2] : null } }, panel: {}, stored: () => ({}), repos: [], locale: 'en' }
  const webviews = load('webview', { vscode, './core': core })
  const colors = []
  load('panel', { vscode, './core': core, './webview': webviews, './diff-colors': { applyDiffColors: async (_, value) => colors.push(value) } }).openPanel({ extensionUri: uri('/extension') }, { id: 'older', root: 'repo' })
  await Promise.all(messages.fire({ id: 0, cmd: 'set_diff_colors', args: { colors: null } }))
  assert.deepEqual(colors, [null])
  assert.deepEqual(calls, [], 'editor color preferences do not go to the Git engine')
  const html = webview.html
  await Promise.all(messages.fire({ id: 1, cmd: 'open_repo', args: { path: 'repo' } }))
  await Promise.all(messages.fire({ id: 3, cmd: 'refs', args: { tab: 7 } }))
  assert.equal(posted.some((m) => m.reveal), false)
  await Promise.all(messages.fire({ id: 4, cmd: 'load_graph', args: { tab: 7, full: true } }))
  assert.deepEqual(posted.at(-1), { reveal: { id: 'older', root: 'repo' } })
  theme.fire({ kind: 1 })
  assert.equal(webview.html, html)
  assert.deepEqual(posted.at(-1), { store: 'theme', value: 'light' })
  const opening = messages.fire({ id: 2, cmd: 'open_repo', args: { path: 'late' } })
  closed.fire()
  late.resolve([8, 'late', 2])
  await Promise.all(opening)
  assert.deepEqual(calls.filter(([cmd]) => cmd === 'close_repo').map(([, args]) => args.tab), [7, 8])
})

test('history context menus resolve their list and stale file requests cannot replace it', async () => {
  const editorChanged = new Event(), repoChanged = new Event(), commands = new Map(), views = new Map(), diffs = []
  const repo = { tab: 0, refs: { head_id: 'head' } }
  const pending = new Map()
  const vscode = {
    EventEmitter: Event, TabInputTextDiff: class {},
    window: {
      activeTextEditor: undefined, onDidChangeActiveTextEditor: editorChanged.event,
      onDidChangeTextEditorSelection: new Event().event,
      createTreeView: (id, { treeDataProvider }) => { const view = { visible: id !== 'pushright.lineHistory', onDidChangeVisibility: new Event().event, provider: treeDataProvider }; views.set(id, view); return view },
    },
    workspace: { onDidChangeTextDocument: new Event().event, onDidSaveTextDocument: new Event().event },
    commands: { registerCommand: (id, fn) => commands.set(id, fn), executeCommand: (...args) => diffs.push(args) },
  }
  const core = {
    repos: [repo], onRepoChange: repoChanged, repoOf: () => repo, rel: (_, u) => u.fsPath, v: {}, attempt: (p) => p,
    revUri: (_, path, rev) => ({ path, rev }),
    engine: { call: (_, { path }) => { const d = deferred(); pending.set(path, d); return d.promise } },
  }
  load('history', { vscode, './core': core }).registerHistory({ subscriptions: [] })
  const follow = async (path) => { vscode.window.activeTextEditor = { document: { uri: uri(path) } }; editorChanged.fire(); await Promise.resolve() }
  const a = [{ id: 'a', path: 'a.txt', subject: 'a' }]
  await follow('a.txt'); pending.get('a.txt').resolve(a); await new Promise(setImmediate)
  const list = views.get('pushright.fileHistory').provider
  assert.equal(list.items, a)
  await commands.get('pushright.item.openChanges')(a[0])
  assert.equal(diffs.at(-1)[0], 'vscode.diff')
  assert.equal(diffs.at(-1)[2].rev, 'a')
  await follow('b.txt'); await follow('a.txt')
  pending.get('b.txt').resolve([{ id: 'b', path: 'b.txt' }]); await new Promise(setImmediate)
  assert.equal(list.items, a)
  const count = pending.size
  repoChanged.fire(repo)
  assert.equal(pending.size, count)
  const page = Array.from({ length: 201 }, (_, i) => ({ id: `commit${i}`, path: 'a.txt' }))
  list.set(repo, 'a.txt', page, 'a.txt')
  const next = deferred()
  list.loader = (skip) => { assert.equal(skip, 200); return next.promise }
  const loading = list.loadMore()
  assert.equal(list.items.length, 200)
  next.resolve([{ id: 'last', path: 'old.txt' }])
  await loading
  assert.equal(list.items.length, 201)
  assert.equal(list.more, false)
  const latePage = deferred()
  list.set(repo, 'a.txt', page, 'a.txt')
  list.loader = () => latePage.promise
  const stalePage = list.loadMore()
  list.set(repo, 'b.txt', [], 'b.txt')
  latePage.resolve([{ id: 'obsolete', path: 'a.txt' }])
  await stalePage
  assert.deepEqual(list.items, [], 'late pages cannot append to another file history')
  core.repos.length = 0
  repoChanged.fire(repo)
  pending.get('a.txt').resolve(a)
  await new Promise(setImmediate)
  assert.deepEqual(list.items, [], 'closing the repository clears history and rejects a late result')
})

test('line history follows selection without focus, coalesces reads and rejects stale results', async () => {
  const selection = new Event(), editorChanged = new Event(), visibility = new Event(), changed = new Event(), saved = new Event(), repoChanged = new Event()
  const views = new Map(), commands = new Map(), executed = [], calls = []
  const repo = { tab: 0, refs: { head_id: 'head' } }
  const range = (start, end = start, character = 2) => ({ start: { line: start, character: 0 }, end: { line: end, character } })
  const editor = { document: { uri: uri('/repo/a.txt'), isDirty: false, getText: () => 'one\ntwo\nthree\n' }, selection: range(0) }
  const vscode = {
    EventEmitter: Event,
    window: {
      activeTextEditor: editor, onDidChangeActiveTextEditor: editorChanged.event, onDidChangeTextEditorSelection: selection.event,
      withProgress: (_, fn) => fn(),
      createTreeView: (id, { treeDataProvider }) => { const view = { visible: id === 'pushright.lineHistory', onDidChangeVisibility: visibility.event, provider: treeDataProvider }; views.set(id, view); return view },
    },
    workspace: { onDidChangeTextDocument: changed.event, onDidSaveTextDocument: saved.event },
    commands: { registerCommand: (id, fn) => commands.set(id, fn), executeCommand: (...args) => executed.push(args) },
  }
  const core = { repos: [repo], onRepoChange: repoChanged, repoOf: () => repo, rel: () => 'a.txt', v: { lineHistoryLoading: 'loading' }, t: { errors: { PR_LINE_HISTORY_CHANGED: 'changed' } }, errorText: String,
    engine: { call: (cmd, args) => { const job = deferred(); calls.push({ cmd, args, job }); return job.promise } },
  }
  const context = { subscriptions: [] }
  load('history', { vscode, './core': core }).registerHistory(context)
  const pause = () => new Promise((r) => setTimeout(r, 160))
  const move = (r) => { editor.selection = r; selection.fire({ textEditor: editor }) }
  const list = views.get('pushright.lineHistory').provider
  try {
    await pause()
    assert.equal(calls[0].args.start, 1)
    move(range(1)); move(range(2, 3, 0)); await pause()
    assert.equal(calls.length, 1, 'wait for the existing query instead of launching parallel Git logs')
    calls[0].job.resolve([{ id: 'stale' }]); await pause()
    assert.deepEqual(list.items, [])
    assert.equal(calls.length, 2)
    assert.deepEqual([calls[1].args.start, calls[1].args.end], [3, 3])
    const latest = [{ id: 'latest', path: 'a.txt' }]
    calls[1].job.resolve(latest); await pause()
    assert.equal(list.items, latest)
    move(range(2)); await pause()
    assert.equal(calls.length, 2, 'same line does not reload')
    assert.deepEqual(executed, [], 'following the cursor must not focus the history pane')
    editor.document.isDirty = true; changed.fire({ document: editor.document }); await pause()
    assert.equal(list.view.message, 'loading')
    assert.deepEqual(list.items, [])
    assert.equal(calls.length, 3)
    assert.equal(calls[2].cmd, 'line_history_page')
    assert.equal(calls[2].args.contents, editor.document.getText())
    calls[2].job.resolve(latest); await pause()
    assert.equal(list.items, latest, 'dirty buffers are mapped by the engine instead of being rejected wholesale')
    editor.document.isDirty = false; saved.fire(editor.document); await pause()
    assert.equal(calls.length, 4)
    calls[3].job.resolve(latest); await pause()
    list.view.visible = false; visibility.fire(); move(range(5)); await pause()
    assert.equal(calls.length, 4, 'hidden views do not request history')
    list.view.visible = true; visibility.fire(); await pause()
    assert.equal(calls[4].args.start, 6)
    calls[4].job.resolve([]); await pause()
    repo.refs.head_id = 'new-head'; repoChanged.fire(repo); await pause()
    assert.equal(calls.length, 6)
    calls[5].job.resolve([])
  } finally { context.subscriptions.forEach((s) => s?.dispose?.()) }
})

test('push mismatch recommends an explicit same-name destination; cancel sends nothing', async () => {
  const sent = [], prompts = []
  const repo = { refs: { head: 'refs/heads/feature/x', refs: [{ name: 'refs/heads/feature/x', upstream: 'refs/remotes/origin/main' }] }, remotes: ['origin'] }
  const v = {}, t = { pushTitle: (b) => b, pushToSameName: (b) => b, pushToUpstream: (b) => b, pushMismatch: (b, up) => `${b}:${up}`, pushTo: (b) => b, push: 'Push', forcePush: 'Force' }
  const vscode = { window: { showQuickPick: async (items) => { prompts.push(items); return items[0] }, showInformationMessage: async () => t.push } }
  const { splitUpstream } = await import('../src/lib/push-target.ts')
  const core = { headBranch: () => 'feature/x', run: (_, op) => sent.push(op), t, v, help: { push: { what: '' } } }
  const { push } = load('sync', { vscode, './core': core, '../../src/lib/push-target': { splitUpstream }, '../../src/lib/branch-name': {} })
  await push(repo)
  assert.equal(prompts[0][0].label, 'origin/feature/x')
  assert.deepEqual(sent, [{ op: 'push', branch: 'feature/x', force: false, remote: 'origin', remote_branch: 'feature/x', set_upstream: true }])
  vscode.window.showQuickPick = async () => undefined
  await push(repo)
  assert.equal(sent.length, 1)
  await push(repo, true)
  assert.equal(sent.length, 2)
  assert.equal(sent[1].remote_branch, 'feature/x', 'graph push cannot choose the mismatched upstream')
})

test('branch picker keeps local refs before remote refs in a stable order and ticks the checked-out branch', async () => {
  const shown = [], sent = [], saved = {}
  const repo = { root: '/repo', refs: { head: 'refs/heads/main', refs: [
    { name: 'refs/remotes/origin/old' }, { name: 'refs/heads/old' }, { name: 'refs/remotes/origin/recent' }, { name: 'refs/heads/recent' }, { name: 'refs/heads/main' },
  ] } }
  let choose = 2, typed
  const vscode = { window: { showQuickPick: async (items) => { shown.push(items); return items[choose] }, showInputBox: async ({ validateInput }) => { assert.ok(validateInput('feature/x')); assert.equal(validateInput(typed), undefined); return typed } } }
  const core = {
    activeRepo: () => repo, help: { checkout: { short: '' } }, t: { checkoutRemote: 'remote', branchRule: 'rule', opNames: { checkout: 'checkout' }, compareBranch: { pick: 'versus', hint: '', pickBranch: '', base: '' } }, v: {}, short: (name) => name.replace(/^refs\/(heads|remotes)\//, ''),
    stored: () => saved, store: async (key, value) => { saved[key] = value }, run: (_, op) => sent.push(op), headBranch: () => 'main', ago: (time) => `at ${time}`,
    engine: { call: async (cmd) => { assert.equal(cmd, 'branch_tips'); return [{ name: 'refs/heads/recent', short_id: 'abc1234', author: 'alice', time: 5, subject: 'latest' }] } },
  }
  const { checkout, pickScope } = load('sync', { vscode, './core': core, '../../src/lib/push-target': {}, '../../src/lib/branch-name': await import('../src/lib/branch-name.ts') })
  await checkout(repo)
  assert.deepEqual(shown[0].slice(1).map((item) => item.ref), ['refs/heads/old', 'refs/heads/recent', 'refs/remotes/origin/old', 'refs/remotes/origin/recent'])
  assert.deepEqual([shown[0][2].description, shown[0][2].detail], ['at 5', 'alice · abc1234 · latest'], 'each branch shows its latest commit')
  assert.deepEqual(sent, [{ op: 'checkout', target: 'recent' }])
  choose = 0; typed = 'feat_S_userLogin'
  await checkout(repo)
  assert.deepEqual(sent.at(-1), { op: 'create_branch', name: 'feat_S_userLogin', start: 'HEAD', checkout: true }, 'new branches default to the current commit')
  choose = 3
  assert.equal(await pickScope(repo), 'refs/heads/recent', 'the graph scope picker keeps the same order after Auto and All')
  assert.deepEqual([shown.at(-1)[4].ref, shown.at(-1)[4].description], ['refs/heads/main', '$(check) '], 'the checked-out branch stays in place and carries the tick')
  const order = shown.at(-1).map((item) => item.ref)
  repo.refs.head = 'refs/heads/recent'
  await pickScope(repo)
  assert.deepEqual(shown.at(-1).map((item) => item.ref), order, 'changing HEAD must not move the branch under the pointer')
  assert.equal(shown.at(-1)[3].description, '$(check) at 5')
  assert.equal(shown.at(-1)[4].description, '')
  assert.deepEqual(saved, {}, 'clicking a branch must not persist recent-use ordering')
  // 最后一项进入多选：预勾当前范围，结果以换行分隔
  let many
  vscode.window.showQuickPick = async (items, options) => options.canPickMany ? (many = items, items.filter((item) => item.picked || item.ref === 'refs/heads/old')) : items.at(-1)
  assert.equal(await pickScope(repo, 'refs/heads/main'), 'refs/heads/old\nrefs/heads/main')
  assert.deepEqual(many.filter((item) => item.picked).map((item) => item.ref), ['refs/heads/main'])
  // 倒数第二项是「与某个分支相比」：先选分支再选基准，基准列表里没有刚选的分支
  const lists = []
  vscode.window.showQuickPick = async (items) => { lists.push(items); return lists.length === 1 ? items.at(-2) : items[0] }
  assert.equal(await pickScope(repo), 'refs/heads/old\n^refs/heads/recent')
  assert.ok(!lists[2].some((item) => item.ref === 'refs/heads/old'))
})

test('SCM separates repositories, stops on failed saves and uses selections after formatting', async () => {
  const commands = new Map(), providers = new Map(), calls = [], writes = [], controls = []
  const repos = [
    { tab: 0, root: '/one', name: 'one', status: [], refs: {} },
    { tab: 1, root: '/two', name: 'two', status: [], refs: { in_progress: 'merge' }, operation: { message: "Merge branch 'x'\n\n# Conflicts:\n#\ta.txt" }, files: [{ path: 'a.txt', resolved: true }, { path: 'b.txt', resolved: false }] },
  ]
  const selection = (line) => ({ start: { line, character: 0 }, end: { line, character: 1 } })
  const editor = { document: { uri: uri('/one/file'), isDirty: true, lineCount: 2, save: async () => false }, selections: [selection(0)] }
  const event = () => ({ dispose() {} })
  const vscode = {
    EventEmitter: Event, Uri: { file: uri },
    FileType: { File: 1 }, FileChangeType: { Changed: 1 }, FileSystemError: { NoPermissions: () => new Error('readonly') },
    scm: { createSourceControl: (id, label, rootUri) => { const control = { id, label, rootUri, groups: [], createResourceGroup(id) { const group = { id }; this.groups.push(group); return group }, inputBox: { value: '' }, dispose() { this.disposed = true } }; controls.push(control); return control } },
    commands: { registerCommand: (id, fn) => commands.set(id, fn) },
    window: { activeTextEditor: editor, onDidChangeWindowState: event },
    workspace: {
      getConfiguration: () => ({ get: (_, fallback) => fallback }),
      textDocuments: [], createFileSystemWatcher: () => ({ onDidChange: event, onDidCreate: event, onDidDelete: event }),
      onDidCloseTextDocument: new Event().event,
      registerFileSystemProvider: (scheme, provider, options) => { assert.equal(options.isReadonly, true); providers.set(scheme, provider) },
    },
  }
  vscode.window.registerFileDecorationProvider = event
  const diff = { hunks: [{ header: '@@ -1,0 +1,2 @@', lines: [1, 2].map((n) => ({ kind: '+', new_no: n })) }] }
  const core = {
    repos, onRepoChange: new Event(), REV: 'rev', v: { conflicts: 'Conflicts' }, t: { identityMissing: '', conflictProgress: (done, total) => `${done}/${total}` }, help: {},
    repoOf: () => repos[0], rel: () => 'file', revOf: () => '', errorText: String, attempt: (p) => p,
    revUri: () => uri('/one/file').with({ scheme: 'rev', query: '{"rev":""}' }),
    engine: { call: async (cmd) => { calls.push(cmd); return diff } },
    write: async (repo, cmd, args) => { writes.push([repo.tab, cmd, args]); return null },
  }
  load('scm', { vscode, './core': core, './sync': { statusCommands: () => [], conflictFiles: (repo) => repo.files ?? [] } }).registerScm({ subscriptions: [] })
  // 合并进行中：输入框预填 Git 的信息（含冲突清单），冲突分组带进度；用户写过的内容和操作结束后的空框都不被覆盖
  assert.deepEqual(controls.map((c) => [c.inputBox.value, c.groups[0].label]), [['', 'Conflicts'], ["Merge branch 'x'\n\n# Conflicts:\n#\ta.txt", 'Conflicts · 1/2']])
  controls[1].inputBox.value = 'my own message'
  core.onRepoChange.fire(repos[1])
  assert.equal(controls[1].inputBox.value, 'my own message')
  controls[1].inputBox.value = ''
  repos[1].refs = {}
  core.onRepoChange.fire(repos[1])
  assert.equal(controls[1].inputBox.value, '')
  repos[1].files = []
  assert.deepEqual(controls.map(({ id, label, rootUri }) => [id, label, rootUri.fsPath]), [['pushright', 'PushRight', '/one'], ['pushright', 'PushRight', '/two']], 'shared provider IDs use one quick-diff menu label while roots distinguish repositories')
  await commands.get('pushright.stage')({ repo: repos[0], entry: { path: 'a' } }, { repo: repos[1], entry: { path: 'b' } })
  assert.deepEqual(writes.splice(0), [[0, 'stage', { paths: ['a'] }], [1, 'stage', { paths: ['b'] }]])
  await commands.get('pushright.stageSelection')()
  assert.deepEqual(calls, [])
  editor.document.save = async () => { editor.selections = [selection(1)]; return true }
  await commands.get('pushright.stageSelection')()
  assert.deepEqual(writes[0][2].lines, [1])
  core.engine.call = async () => null
  const provider = providers.get('rev')
  const original = core.revUri()
  assert.equal((await provider.readFile(original)).toString(), '')
  assert.equal(await controls[0].quickDiffProvider.provideOriginalResource(editor.document.uri), undefined)
  core.onRepoChange.fire(repos[0])
  core.engine.call = async () => { throw new Error('PR_FILE_TOO_LARGE') }
  await assert.rejects(provider.readFile(original), /PR_FILE_TOO_LARGE/)
  let reads = 0
  core.engine.call = async () => { reads++; return '原文\n' }
  await Promise.all([controls[0].quickDiffProvider.provideOriginalResource(editor.document.uri), provider.stat(original), provider.readFile(original)])
  assert.equal(reads, 1, 'quick diff, metadata and content share one read, including retries after an error')
  assert.equal((await provider.stat(original)).size, Buffer.byteLength('原文\n'))
  assert.equal((await provider.readFile(original)).toString(), '原文\n')
  const oldTime = (await provider.stat(original)).mtime
  const events = []
  provider.onDidChangeFile((event) => events.push(...event))
  vscode.workspace.textDocuments.push({ uri: original })
  core.onRepoChange.fire(repos[0])
  core.engine.call = async () => { reads++; return '暂存后\n' }
  assert.equal((await provider.readFile(original)).toString(), '暂存后\n')
  assert.equal(reads, 2)
  assert.ok((await provider.stat(original)).mtime > oldTime)
  assert.deepEqual(events, [{ type: 1, uri: original }])
  for (const action of ['createDirectory', 'writeFile', 'delete', 'rename']) assert.throws(() => provider[action](original), /readonly/)
  Object.assign(core.t, { repositorySettings: 'Repository settings', disableRevert: 'Disable revert', allowRevert: 'Allow revert', wtCurrent: 'Current' })
  core.refresh = async (repo) => { repo.refs = { revert_disabled: true } }
  core.run = async (repo, op) => { writes.push([repo.tab, 'op', op]); return true }
  vscode.window.showQuickPick = async (items, options) => {
    assert.match(options.title, /^two · Repository settings/)
    assert.equal(items[0].description, 'Current', 'settings are refreshed before showing the choice')
    return items[1]
  }
  await commands.get('pushright.repositorySettings')(controls[1])
  assert.deepEqual(writes.at(-1), [1, 'op', { op: 'set_revert_disabled', disabled: false }], 'settings target the selected repository, not the active editor')
  const writeCount = writes.length
  vscode.window.showQuickPick = async () => undefined
  await commands.get('pushright.repositorySettings')(controls[1])
  assert.equal(writes.length, writeCount, 'cancel leaves the policy unchanged')
  core.closeRepo = async (repo) => { repos.splice(repos.indexOf(repo), 1); core.onRepoChange.fire(repo) }
  await commands.get('pushright.closeRepository')(controls[1])
  assert.equal(controls[1].disposed, true, 'repository context menu closes its target, not the active editor repository')
  assert.equal(controls[0].disposed, undefined)
  assert.equal(repos.length, 1)
})

test('disabled revert offers abort only, while other operations retain continue', async () => {
  const calls = [], shown = []
  const repo = { refs: { in_progress: 'revert', revert_disabled: true } }
  const t = { inProgress: {}, inProgressHint: '', continueOperation: (what) => `git ${what} --continue`, abortOperation: (what) => `git ${what} --abort` }
  const vscode = { window: { showWarningMessage: async (_, options, ...choices) => { shown.push(choices); return choices[0] } } }
  const sync = load('sync', { vscode, './core': { t, run: async (_, op) => calls.push(op) }, '../../src/lib/branch-name': {}, '../../src/lib/push-target': {} })
  await sync.inProgress(repo)
  assert.deepEqual(shown[0], ['git revert --abort'])
  assert.deepEqual(calls[0], { op: 'abort', what: 'revert' })
  repo.refs.in_progress = 'rebase'
  await sync.inProgress(repo)
  assert.deepEqual(shown[1], ['git rebase --continue', 'git rebase --abort'])
  assert.deepEqual(calls[1], { op: 'continue', what: 'rebase' })

  // 冲突信息：正在处理的提交、变基步数、进度和逐个文件的状态；已解决的文件仍在清单里
  Object.assign(t, { inProgressHint: 'hint', rebaseConflictHint: 'sides', operationCommit: (id, subject) => `${id} ${subject}`, operationStep: (n, total) => `step ${n}/${total}`, conflictProgress: (done, total) => `${done}/${total}` })
  Object.assign(repo, { status: [{ path: 'b.txt', conflicted: true }, { path: 'c.txt' }], operation: { id: '0123456789abcdef', message: 'subject\n\n# Conflicts:', conflicts: ['a.txt', 'b.txt'], step: [2, 5] } })
  assert.deepEqual(sync.conflictFiles(repo), [{ path: 'a.txt', resolved: true }, { path: 'b.txt', resolved: false }])
  vscode.window.showWarningMessage = async (_, options) => void shown.push(options.detail)
  await sync.inProgress(repo)
  assert.equal(shown[2], '01234567 subject · step 2/5 · 1/2\n\n✓ a.txt\n! b.txt\n\nhint\n\nsides')
})

test('local files expand lazily and tracking commands distinguish local rules, shared rules and deletion', async () => {
  let provider, tracked = false, accept = true
  const commands = new Map(), writes = [], reads = [], prompts = [], discoveries = []
  const repo = { tab: 2, root: '/parent/child', name: 'parent/child', status: [{ path: 'skills/a.md', unstaged: '?' }, { path: 'skills/deep/b.md', unstaged: '?' }] }
  const vscode = {
    EventEmitter: Event, commands: { registerCommand: (id, fn) => commands.set(id, fn) },
    window: { createTreeView: (_, { treeDataProvider }) => { provider = treeDataProvider; return { visible: true, onDidChangeVisibility: new Event().event } },
      showInformationMessage: async (...args) => { prompts.push(args); return accept ? args[2] : undefined }, showWarningMessage: (message) => prompts.push(message) },
  }
  const core = { repos: [repo], onRepoChange: new Event(), attempt: (p) => p, rel: (_, u) => u.fsPath.endsWith('b.md') ? 'skills/deep/b.md' : 'skills/a.md', repoOf: () => repo,
    discoverForFile: (uri) => discoveries.push(uri.fsPath), fileUri: (_, p) => uri(`${repo.root}/${p}`),
    engine: { call: async (cmd, args) => { reads.push([cmd, args]); return cmd === 'is_tracked' ? tracked : ['cache/'] } },
    write: async (...args) => writes.push(args),
    v: { ignoreLocal: 'local', ignoreShared: 'shared', trackFile: 'track', untrackFile: 'untrack', untrackHint: 'stages deletion for the team', ignoreLocalHint: 'clone only', ignoreSharedHint: 'shared rule', trackHint: 'stage contents', ignoreTracked: 'tracked warning' },
  }
  load('local-files', { vscode, './core': core }).registerLocalFiles({ subscriptions: [] })
  const [root] = await provider.getChildren()
  const [untracked, ignored] = await provider.getChildren(root)
  assert.equal(reads.length, 0)
  const [folder] = await provider.getChildren(untracked)
  assert.equal(folder.path, 'skills')
  assert.deepEqual((await provider.getChildren(folder)).map((f) => [f.path, f.directory]), [['skills/deep', true], ['skills/a.md', false]])
  await provider.getChildren(ignored)
  assert.deepEqual(reads.pop(), ['ignored', { tab: 2, path: '' }])
  const file = uri('/parent/child/skills/a.md')
  await commands.get('pushright.ignoreLocal')(file)
  await commands.get('pushright.ignoreShared')(file)
  assert.deepEqual(writes.map(([, cmd, args]) => [cmd, args]), [['ignore_file', { path: 'skills/a.md', shared: false }], ['ignore_file', { path: 'skills/a.md', shared: true }]])
  tracked = true
  await commands.get('pushright.ignoreShared')(file)
  assert.equal(writes.length, 2)
  assert.equal(prompts.at(-1), 'tracked warning')
  accept = false
  await commands.get('pushright.untrackFile')(file)
  assert.equal(writes.length, 2)
  assert.match(prompts.at(-1)[1].detail, /stages deletion for the team/)
  accept = true
  await commands.get('pushright.untrackFile')(file)
  assert.deepEqual(writes.at(-1), [repo, 'track_file', { path: 'skills/a.md', track: false }])
  await commands.get('pushright.trackFile')(file)
  assert.deepEqual(writes.at(-1), [repo, 'track_file', { path: 'skills/a.md', track: true }])
  await commands.get('pushright.untrackFile')({ repo, entry: { path: 'skills/a.md' }, resourceUri: file })
  assert.deepEqual(writes.at(-1), [repo, 'track_file', { path: 'skills/a.md', track: false }], 'SCM resource arguments use their URI, not the local-tree node shape')
  tracked = false
  const second = uri('/parent/child/skills/deep/b.md')
  await commands.get('pushright.ignoreLocal')(file, [file, second])
  assert.deepEqual(writes.slice(-2).map(([, cmd, args]) => [cmd, args]), [
    ['ignore_file', { path: 'skills/a.md', shared: false }],
    ['ignore_file', { path: 'skills/deep/b.md', shared: false }],
  ])
  assert.deepEqual(new Set(discoveries), new Set([file.fsPath, second.fsPath]))
})
