// 由 tests/vscode-smoke.mjs 启动，在 VS Code 扩展主机里执行。
const assert = require('node:assert/strict')
const { execFileSync } = require('node:child_process')
const { mkdirSync, writeFileSync } = require('node:fs')
const { join } = require('node:path')
const vscode = require('vscode')

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
async function until(what, check, timeout = 20000) {
  for (const start = Date.now(); Date.now() - start < timeout; await sleep(200)) {
    const value = await check()
    if (value) return value
  }
  throw new Error(`timed out waiting for ${what}`)
}

exports.run = async () => {
  const passed = []
  let error
  try {
    const root = vscode.workspace.workspaceFolders[0].uri.fsPath
    const git = (...args) => execFileSync('git', ['-C', root, ...args], { encoding: 'utf8' }).trim()
    const file = vscode.Uri.file(join(root, 'src.ts'))
    const step = (name) => { passed.push(name); console.log(`[smoke ${new Date().toISOString()}] ${name}`) }

    const extension = vscode.extensions.getExtension(process.env.PUSHRIGHT_SMOKE_ID)
    const api = await extension.activate()
    assert.equal(api.repos.length, 1)
    assert.equal(api.repos[0].refs.head, 'refs/heads/feature/x')
    step('activates and opens the workspace repository')
    const initialTab = api.repos[0].tab
    const comparison = await api.engine.call('compare', { tab: initialTab, left: 'HEAD~1', right: 'HEAD', commonBase: false })
    assert.ok(comparison.files.some((f) => f.path === 'mine.txt'))
    const comparedDiff = await api.engine.call('diff_between', { tab: initialTab, left: comparison.left, right: comparison.right, path: 'mine.txt', oldPath: null })
    assert.ok(comparedDiff.hunks.length)
    assert.ok((await api.engine.call('remote_details', { tab: initialTab })).some((r) => r.name === 'origin'))
    assert.ok((await api.engine.call('reflog', { tab: initialTab, skip: 0, limit: 201 })).length)
    assert.equal((await api.engine.call('rebase_plan', { tab: initialTab, base: 'HEAD~1' })).steps.length, 1)
    step('routes comparison, diff, remote details, reflog and rebase planning through the bundled engine')

    const registered = new Set(await vscode.commands.getCommands())
    const missing = extension.packageJSON.contributes.commands.map((c) => c.command).filter((c) => !registered.has(c))
    assert.deepEqual(missing, [])
    step(`registers all ${extension.packageJSON.contributes.commands.length} contributed commands`)

    const workbench = vscode.workspace.getConfiguration('workbench')
    const originalColors = workbench.inspect('colorCustomizations').globalValue
    const themeColors = { 'editor.background': '#123456', '[Default Dark Modern]': { 'diffEditor.insertedLineBackground': '#11223344' } }
    await workbench.update('colorCustomizations', themeColors, vscode.ConfigurationTarget.Global)
    const colorPicker = vscode.commands.executeCommand('pushright.diffColors')
    await sleep(750)
    await vscode.commands.executeCommand('workbench.action.acceptSelectedQuickOpenItem')
    await colorPicker
    const appliedColors = vscode.workspace.getConfiguration('workbench').inspect('colorCustomizations').globalValue
    assert.equal(appliedColors['diffEditor.insertedLineBackground'], '#0072b238')
    assert.equal(appliedColors['[Default Dark Modern]']['diffEditor.removedLineBackground'], '#e69f0038')
    const resetColors = vscode.commands.executeCommand('pushright.diffColors')
    await sleep(750)
    for (let i = 0; i < 4; i++) await vscode.commands.executeCommand('workbench.action.quickOpenSelectNext')
    await vscode.commands.executeCommand('workbench.action.acceptSelectedQuickOpenItem')
    await resetColors
    assert.deepEqual(vscode.workspace.getConfiguration('workbench').inspect('colorCustomizations').globalValue, themeColors)
    await workbench.update('colorCustomizations', originalColors, vscode.ConfigurationTarget.Global)
    step('diff color command applies blue/orange to native editors and restores prior user and theme settings')

    const settings = vscode.commands.executeCommand('pushright.repositorySettings', api.repos[0])
    await sleep(750)
    await vscode.commands.executeCommand('workbench.action.acceptSelectedQuickOpenItem')
    await settings
    assert.equal(git('config', '--local', '--get', 'pushright.disableRevert'), 'true')
    assert.equal((await api.engine.call('refs', { tab: initialTab })).revert_disabled, true)
    const protectedHead = git('rev-parse', 'HEAD')
    await assert.rejects(api.engine.call('op', { tab: initialTab, op: { op: 'revert', id: 'HEAD' } }), /PR_REVERT_DISABLED/)
    assert.equal(git('rev-parse', 'HEAD'), protectedHead)
    await api.engine.call('op', { tab: initialTab, op: { op: 'set_revert_disabled', disabled: false } })
    await vscode.commands.executeCommand('pushright.refresh')
    await until('revert setting refreshed', () => api.repos[0].refs.revert_disabled === false)
    step('repository settings persist locally and the shared engine blocks revert without changing HEAD')

    // Pull：选择框里「Rebase instead of merge」默认勾选，直接确认应当是变基
    const pulling = vscode.commands.executeCommand('pushright.pull')
    await sleep(1500)
    await vscode.commands.executeCommand('workbench.action.acceptSelectedQuickOpenItem')
    await pulling
    assert.equal(git('log', '--format=%s', '-3'), 'local commit\ncolleague commit\nraise value10')
    assert.equal(git('log', '--merges', '--format=%s'), '')
    step('pull with the default choice rebases instead of merging')

    const head = await vscode.workspace.openTextDocument(file.with({ scheme: 'pushright-rev', query: JSON.stringify({ rev: 'HEAD' }) }))
    assert.equal(head.getText(), git('show', 'HEAD:src.ts') + '\n')
    step('serves file contents at a revision')
    const image = vscode.Uri.file(join(root, 'preview.png')).with({ scheme: 'pushright-rev', query: JSON.stringify({ rev: 'HEAD' }) })
    const imageBytes = await vscode.workspace.fs.readFile(image)
    assert.equal(Buffer.from(imageBytes).toString('base64'), 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jvWoAAAAASUVORK5CYII=')
    step('serves binary image revisions without text decoding')

    const showCall = api.engine.call.bind(api.engine)
    let reads = 0
    api.engine.call = async (cmd, args) => {
      if (cmd === 'show' && args.rev === '' && args.path === 'src.ts') reads++
      return showCall(cmd, args)
    }
    const editor = await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file))
    await until('quick diff baseline', () => editor.diffInformation?.some((d) => d.original?.scheme === 'pushright-rev'))
    assert.equal(reads, 1, 'opening a file reads the index only once for both existence and content')
    step('quick diff loads its baseline with one index read')
    api.engine.call = showCall
    const lens = await until('blame code lens', async () => {
      const lenses = await vscode.commands.executeCommand('vscode.executeCodeLensProvider', file)
      return lenses?.find((l) => l.command?.command === 'pushright.fileHistory' && /^bob, /.test(l.command.title))
    })
    assert.match(lens.command.title, /^bob, /)
    step('blames the file and shows the latest author as a code lens')

    const lineCalls = []
    const originalCall = api.engine.call.bind(api.engine)
    api.engine.call = async (cmd, args) => {
      const result = await originalCall(cmd, args)
      if (cmd === 'line_history_page') lineCalls.push({ args, result })
      return result
    }
    // 仅展开视图，随后移动光标，不执行 pushright.lineHistory 命令。
    await vscode.commands.executeCommand('pushright.lineHistory.focus')
    await vscode.window.showTextDocument(editor.document)
    editor.selection = new vscode.Selection(10, 0, 10, 0)
    await until('automatic line history', () => lineCalls.some((c) => c.args.start === 11 && c.args.end === 11 && c.result.some((r) => r.subject === 'raise value10')))
    assert.equal(vscode.window.activeTextEditor.document.uri.toString(), file.toString())
    step('line history follows the selected line without its context-menu command')
    await vscode.commands.executeCommand('workbench.view.explorer')
    await sleep(250)
    const beforeHidden = lineCalls.length
    editor.selection = new vscode.Selection(20, 0, 20, 0)
    await sleep(350)
    assert.equal(lineCalls.length, beforeHidden)
    step('hidden line history does not query on cursor movement')
    api.engine.call = originalCall

    // 改两处，只选中第一处暂存
    const text = editor.document.getText().split('\n')
    text[0] = 'export const value0 = -1'
    text[20] = 'export const value20 = -20'
    writeFileSync(file.fsPath, text.join('\n'))
    await until('editor reload', () => editor.document.lineAt(0).text.endsWith('-1'))
    await vscode.commands.executeCommand('pushright.refresh')
    const change = { repo: api.repos[0], entry: api.repos[0].status.find((e) => e.path === 'src.ts'), staged: false, resourceUri: file }
    await until('two gutter changes', () => editor.diffInformation?.some((d) => d.original?.scheme === 'pushright-rev' && d.changes.length === 2))
    await vscode.commands.executeCommand('editor.action.dirtydiff.next')
    for (let i = 0; i < 2; i++) {
      await vscode.commands.executeCommand('pushright.openChange', change)
      await until('unstaged diff', () => vscode.window.tabGroups.activeTabGroup.activeTab?.input instanceof vscode.TabInputTextDiff)
      const input = vscode.window.tabGroups.activeTabGroup.activeTab.input
      const [left, right] = await Promise.all([vscode.workspace.openTextDocument(input.original), vscode.workspace.openTextDocument(input.modified)])
      assert.equal(left.getText(), git('show', ':src.ts') + '\n')
      assert.equal(right.getText(), text.join('\n'))
      await vscode.window.tabGroups.close(vscode.window.tabGroups.activeTabGroup.activeTab)
    }
    step('opens and reopens an unstaged diff while the file and quick diff are already loaded')
    const selectedEditor = await vscode.window.showTextDocument(editor.document)
    selectedEditor.selection = new vscode.Selection(0, 0, 0, 5)
    await vscode.commands.executeCommand('pushright.stageSelection')
    assert.match(git('diff', '--cached'), /\+export const value0 = -1/)
    assert.doesNotMatch(git('diff', '--cached'), /value20 = -20/)
    assert.match(git('diff'), /\+export const value20 = -20/)
    await until('gutter updates after staging', () => selectedEditor.diffInformation?.some((d) => d.original?.scheme === 'pushright-rev' && d.changes.length === 1))
    step('stages only the selected lines')

    await until('status refresh', () => api.repos[0].status.some((e) => e.staged))
    await vscode.commands.executeCommand('pushright.unstageAll')
    assert.equal(git('diff', '--cached', '--name-only'), '')
    await vscode.commands.executeCommand('pushright.stageAll')
    assert.equal(git('diff', '--cached', '--name-only'), 'src.ts')
    await vscode.commands.executeCommand('pushright.commitAmend')
    assert.equal(git('status', '--porcelain'), '')
    assert.equal(git('log', '--format=%s', '-1'), 'local commit')
    await until('amended HEAD refresh', () => api.repos[0].refs.head_id === git('rev-parse', 'HEAD'))
    step('stages, unstages and amends through the source control commands')

    await vscode.commands.executeCommand('pushright.previousRevision')
    const diffOf = () => vscode.window.tabGroups.activeTabGroup.activeTab?.input
    await until('diff with HEAD', () => diffOf() instanceof vscode.TabInputTextDiff && diffOf().modified.scheme === 'file')
    const latest = JSON.parse(diffOf().original.query).rev
    await vscode.commands.executeCommand('pushright.previousRevision')
    await until('diff of the latest commit', () => diffOf() instanceof vscode.TabInputTextDiff && JSON.parse(diffOf().modified.query || '{}').rev === latest)
    await vscode.commands.executeCommand('pushright.previousRevision')
    await until('diff of an older commit', () => diffOf() instanceof vscode.TabInputTextDiff && ![latest, undefined].includes(JSON.parse(diffOf().modified.query || '{}').rev))
    step('steps back through the revisions of a file')

    // 关闭原生 Git 后，SCM 侧栏图仍会自动读取仓库中的提交行。
    const graphCalls = []
    const call = api.engine.call.bind(api.engine)
    api.engine.call = async (cmd, args) => {
      const result = await call(cmd, args)
      graphCalls.push({ cmd, args, result })
      return result
    }
    // 只打开 SCM 容器，不执行图的展开命令；首次启用后提交图应已自动展开。
    await vscode.commands.executeCommand('workbench.view.scm')
    await until('source control sidebar graph rows', () => graphCalls.some((c) => c.cmd === 'rows' && c.args.tab === 0 && c.result.some((r) => r.subject === 'local commit')))
    step('source control sidebar graph opens by default with built-in Git disabled')
    git('-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-m', 'sidebar refresh')
    await vscode.commands.executeCommand('pushright.refresh')
    await until('sidebar graph refresh', () => graphCalls.some((c) => c.cmd === 'rows' && c.args.tab === 0 && c.result.some((r) => r.subject === 'sidebar refresh')))
    step('source control sidebar graph refreshes after a new commit')
    git('branch', 'graph-scope-old', 'HEAD~1')
    await vscode.commands.executeCommand('pushright.refresh')
    await until('new reference loaded', () => api.repos[0].refs.refs.some((r) => r.name === 'refs/heads/graph-scope-old'))
    graphCalls.length = 0
    git('checkout', '-q', 'graph-scope-old')
    await vscode.commands.executeCommand('pushright.refresh')
    await until('Auto follows checkout with unchanged all-ref tips', () => graphCalls.some((c) => c.cmd === 'rows' && c.result.length && c.result.every((r) => r.subject !== 'sidebar refresh')))
    assert.ok(graphCalls.some((c) => c.cmd === 'load_graph' && c.args.scope === 'auto'))
    git('checkout', '-q', 'feature/x')
    await vscode.commands.executeCommand('pushright.refresh')
    step('Auto rebuilds its ancestry when the checked-out branch changes')
    api.engine.call = call

    mkdirSync(join(root, '新增 目录', 'nested'), { recursive: true })
    const newPaths = ['新增 目录/a.txt', '新增 目录/nested/b.txt']
    for (const path of newPaths) writeFileSync(join(root, path), `${path}\n`)
    await vscode.commands.executeCommand('pushright.refresh')
    const repo = api.repos[0]
    await until('new files status refresh', () => repo.status.length === newPaths.length)
    assert.deepEqual(repo.status.map((e) => e.path), newPaths)
    for (const entry of repo.status) {
      const diff = await api.engine.call('diff_worktree', { tab: repo.tab, path: entry.path, staged: false, untracked: true })
      assert.equal(diff.hunks[0].lines[0].text, entry.path)
      await vscode.commands.executeCommand('pushright.openChange', { repo, entry, staged: false, resourceUri: vscode.Uri.file(join(root, entry.path)) })
      await until('untracked file opens', () => vscode.window.activeTextEditor?.document.getText() === `${entry.path}\n`)
    }
    step('new folders list and preview every untracked file without directory access errors')

    // 侧栏文件入口必须打开不可变的父版本/提交版本，包括根提交、新增、删除和重命名。
    const checkCommitFile = async (id, path, before, after) => {
      await vscode.commands.executeCommand('pushright.graph.openFile', { tab: repo.tab, id, path, preview: false })
      await until('commit file diff', () => diffOf() instanceof vscode.TabInputTextDiff && JSON.parse(diffOf().modified.query).rev === id && diffOf().modified.fsPath === join(root, path))
      const [left, right] = await Promise.all([vscode.workspace.openTextDocument(diffOf().original), vscode.workspace.openTextDocument(diffOf().modified)])
      assert.equal(left.getText(), before)
      assert.equal(right.getText(), after)
    }
    const initial = git('rev-list', '--max-parents=0', 'HEAD')
    await checkCommitFile(initial, 'src.ts', '', git('show', `${initial}:src.ts`) + '\n')
    const previous = git('rev-parse', 'HEAD')
    git('mv', 'src.ts', '重命名.ts')
    git('rm', 'mine.txt')
    git('add', '--', ...newPaths)
    git('-c', 'commit.gpgsign=false', 'commit', '-m', 'rename, delete and add files')
    const revision = git('rev-parse', 'HEAD')
    const original = git('show', `${previous}:src.ts`) + '\n'
    await checkCommitFile(revision, '重命名.ts', original, original)
    await checkCommitFile(revision, 'mine.txt', 'local work\n', '')
    await checkCommitFile(revision, newPaths[0], '', `${newPaths[0]}\n`)
    assert.ok(!vscode.window.tabGroups.all.some((g) => g.tabs.some((tab) => tab.input instanceof vscode.TabInputWebview)))
    step('commit files open native diffs for root, renamed, deleted and added files without a graph editor')

    // 实际子模块使用 .git 文件，嵌套仓库使用 .git 目录；刷新后两者都应独立提供 SCM。
    git('-c', 'protocol.file.allow=always', 'submodule', 'add', '-q', join(root, '..', 'origin.git'), 'modules/shared')
    const nested = join(root, 'tools/private-repo')
    mkdirSync(nested, { recursive: true })
    execFileSync('git', ['-C', nested, 'init', '-q', '-b', 'main'])
    execFileSync('git', ['-C', nested, '-c', 'user.name=test', '-c', 'user.email=test@example.com', '-c', 'commit.gpgsign=false', 'commit', '-q', '--allow-empty', '-m', 'nested root'])
    await vscode.commands.executeCommand('pushright.refresh')
    assert.equal(api.repos.length, 3)
    const sub = api.repos.find((r) => r.root === join(root, 'modules/shared'))
    assert.ok(sub)
    assert.match(sub.name, /modules\/shared$/)
    assert.ok(api.repos.some((r) => r.root === nested))
    writeFileSync(join(sub.root, 'private-skill.md'), 'private skill\n')
    await api.engine.call('ignore_file', { tab: sub.tab, path: 'private-skill.md', shared: false })
    assert.deepEqual(await api.engine.call('ignored', { tab: sub.tab, path: '' }), ['private-skill.md'])
    assert.ok(!git('status', '--porcelain', '--untracked-files=all').includes('private-skill.md'))
    await vscode.commands.executeCommand('pushright.localFiles.focus')
    step('discovers submodules and nested repositories and keeps local ignore rules in the selected repository')

    // 单独打开工作区外的文件会发现新仓库；关闭后保留文件，并且刷新、重新聚焦都不能打开它。
    const outside = vscode.Uri.file(join(root, '..', 'other', 'src.ts'))
    const outsideEditor = await vscode.window.showTextDocument(outside)
    const external = await until('external repository discovery', () => api.repos.find((r) => r.root === join(root, '..', 'other')))
    await vscode.commands.executeCommand('pushright.closeRepository', external)
    assert.equal(api.repos.includes(external), false)
    await assert.rejects(api.engine.call('refs', { tab: external.tab }), /PR_TAB_CLOSED/)
    assert.equal(outsideEditor.document.isClosed, false)
    await vscode.window.showTextDocument(vscode.Uri.file(join(root, '重命名.ts')))
    await vscode.window.showTextDocument(outside)
    await vscode.commands.executeCommand('pushright.refresh')
    assert.equal(api.repos.length, 3)
    const reopening = vscode.commands.executeCommand('pushright.reopenRepository')
    await sleep(500)
    await vscode.commands.executeCommand('workbench.action.acceptSelectedQuickOpenItem')
    await reopening
    assert.ok(api.repos.some((r) => r.root === external.root))
    await vscode.commands.executeCommand('pushright.closeRepository', sub)
    await vscode.commands.executeCommand('pushright.refresh')
    assert.equal(api.repos.some((r) => r.root === sub.root), false)
    assert.ok(api.repos.includes(repo), 'closing a subrepository preserves its parent')
    step('closes external and nested repositories, keeps their files open and supports explicit reopening')

    const configs = ['MeleeConfig.luau', 'GunConfig.luau']
    for (const name of configs) writeFileSync(join(root, name), 'return { id = 1 }\n')
    git('add', '--', ...configs)
    git('-c', 'commit.gpgsign=false', 'commit', '--only', '-m', 'config casing fixture', '--', ...configs)
    await vscode.commands.executeCommand('pushright.refresh')
    for (const name of configs) {
      const stale = vscode.Uri.file(join(root, name[0].toLowerCase() + name.slice(1)))
      const editor = await vscode.window.showTextDocument(stale)
      assert.equal(editor.document.uri.fsPath, stale.fsPath, 'host retains the stale spelling')
      await until('blame with stale config casing', async () => {
        const lenses = await vscode.commands.executeCommand('vscode.executeCodeLensProvider', editor.document.uri)
        return lenses?.some((l) => l.command?.command === 'pushright.fileHistory' && /^carol, /.test(l.command.title))
      })
    }
    step('blames MeleeConfig and GunConfig despite lowercase cached editor paths')
  } catch (e) {
    error = e.stack ?? String(e)
  }
  writeFileSync(process.env.PUSHRIGHT_SMOKE_RESULT, JSON.stringify({ passed, error }))
  if (error) throw new Error(error)
}
