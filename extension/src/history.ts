import { basename } from 'node:path'
import * as vscode from 'vscode'
import { activeRepo, ago, attempt, date, engine, errorText, fileUri, onPrefsChange, onRepoChange, rel, repoOf, repos, REV, revOf, revUri, short, t, v, type Repo } from './core'

/** 与 crates/engine/src/history.rs 的 Commit 对应 */
interface Commit {
  id: string
  author: string
  author_email: string
  time: number
  subject: string
  path: string
}

// Pages are fetched on demand; the extra row indicates whether another page exists.
const LIMIT = 200
const MORE: Commit = { id: '', author: '', author_email: '', time: 0, subject: '', path: '' }

/** 侧栏里的一个提交列表 */
class CommitList implements vscode.TreeDataProvider<Commit> {
  repo: Repo | undefined
  /** 列表对应的文件（当前名字）；搜索结果没有 */
  path = ''
  items: Commit[] = []
  more = false
  loader?: (skip: number) => Promise<Commit[]>
  private generation = 0
  private loading = false
  private readonly changed = new vscode.EventEmitter<void>()
  readonly onDidChangeTreeData = this.changed.event
  readonly view: vscode.TreeView<Commit>

  constructor(id: string, private readonly click: string) {
    this.view = vscode.window.createTreeView(id, { treeDataProvider: this })
  }

  set(repo: Repo | undefined, path: string, items: Commit[], description: string) {
    ++this.generation
    this.more = items.length > LIMIT
    Object.assign(this, { repo, path, items: this.more ? items.slice(0, LIMIT) : items })
    this.view.description = description
    this.view.message = items.length ? undefined : v.noHistory
    this.changed.fire()
  }

  getChildren = () => this.more ? [...this.items, MORE] : this.items
  refresh = () => this.changed.fire()

  async loadMore() {
    if (!this.more || !this.loader || this.loading) return
    this.loading = true
    const generation = this.generation
    try {
      const items = await this.loader(this.items.length)
      if (generation !== this.generation) return
      this.more = items.length > LIMIT
      this.items = [...this.items, ...items.slice(0, LIMIT)]
      this.changed.fire()
    } finally { this.loading = false }
  }

  getTreeItem(c: Commit) {
    if (c === MORE) {
      const item = new vscode.TreeItem(t.tools.loadMore)
      item.command = { command: 'pushright.loadMore', title: '', arguments: [this] }
      return item
    }
    const item = new vscode.TreeItem(c.subject)
    item.description = `${c.author} · ${ago(c.time)}`
    item.tooltip = `${c.id.slice(0, 8)} · ${date(c.time)}`
    item.iconPath = new vscode.ThemeIcon('git-commit')
    item.contextValue = c.path ? 'commitFile' : 'commit'
    item.command = { command: this.click, title: '', arguments: [c, this] }
    return item
  }
}

export function registerHistory(context: vscode.ExtensionContext) {
  const files = new CommitList('pushright.fileHistory', 'pushright.item.openChanges')
  const lines = new CommitList('pushright.lineHistory', 'pushright.item.openChanges')
  const found = new CommitList('pushright.search', 'pushright.item.reveal')
  lines.view.message = v.lineHistoryHint
  let fileRequest = 0
  let fileHead: string | null = null
  let lineRequest = 0
  let lineKey = ''
  let lineHead: string | null = null
  let lineTimer: ReturnType<typeof setTimeout>
  let lineBusy = false
  let lineAgain = false

  /** 让文件历史列表对应这个文件；已经是它就不重取。 */
  async function loadFile(repo: Repo, path: string) {
    const request = ++fileRequest
    const head = repo.refs.head_id
    if (files.repo === repo && files.path === path && fileHead === head) return
    const loader = (skip: number) => engine.call<Commit[]>('file_history', { tab: repo.tab, path, limit: LIMIT + 1, skip })
    const items = (await attempt(loader(0))) ?? []
    if (request !== fileRequest || !repos.includes(repo)) return
    fileHead = head
    files.set(repo, path, items, basename(path))
    files.loader = loader
  }

  /** 文件历史跟随当前编辑器，只在视图可见时去取。 */
  function follow() {
    const uri = vscode.window.activeTextEditor?.document.uri
    const repo = uri?.scheme === 'file' ? repoOf(uri) : undefined
    if (repo && files.view.visible) loadFile(repo, rel(repo, uri!))
  }

  /** 这个提交对该文件做了什么：与它的上一版对比。 */
  async function openChanges(c: Commit, list = [files, lines].find((l) => l.items.includes(c))) {
    if (!list?.repo) return
    if (list === files && list.more && list.items.at(-1) === c) await list.loadMore()
    if (!list.items.includes(c)) return
    const older = list.items[list.items.indexOf(c) + 1]
    // 行历史的相邻两条不一定是文件的相邻两版，所以左边固定取父提交；路径取更早那条的，重命名时才对得上
    const left = revUri(list.repo, list === files && older ? older.path : c.path, `${c.id}^`)
    return vscode.commands.executeCommand('vscode.diff', left, revUri(list.repo, c.path, c.id), `${basename(c.path)} (${c.id.slice(0, 8)})`)
  }

  /** 在当前文件的历次版本间移动：-1 是工作区对比最近一版，往后每一步是更早的一个提交。 */
  async function step(delta: number) {
    const input = vscode.window.tabGroups.activeTabGroup.activeTab?.input
    const uri = input instanceof vscode.TabInputTextDiff ? input.modified : vscode.window.activeTextEditor?.document.uri
    const repo = repoOf(uri)
    if (!uri || !repo) return
    if (uri.scheme === 'file') await loadFile(repo, rel(repo, uri))
    if (files.repo !== repo || !files.items.length) return void vscode.window.showInformationMessage(v.noHistory)
    const rev = revOf(uri)
    // 对比视图里按右侧的版本定位；右侧是工作区文件、左侧是最近一版时为 -1；普通编辑器算还没开始（-2）
    const found = rev === undefined ? -1 : files.items.findIndex((c) => c.id === rev)
    const at = found >= 0 ? found : input instanceof vscode.TabInputTextDiff && revOf(input.original) === files.items[0].id && rev === undefined ? -1 : -2
    if (delta > 0 && at + delta >= files.items.length) await files.loadMore()
    const to = Math.max(-1, Math.min(files.items.length - 1, at + delta))
    if (to === -1) {
      const head = files.items[0]
      return vscode.commands.executeCommand('vscode.diff', revUri(repo, head.path, head.id), fileUri(repo, files.path), `${basename(files.path)} (${head.id.slice(0, 8)} ↔ ${v.workingTree})`)
    }
    return openChanges(files.items[to], files)
  }

  async function lineHistory(uri?: vscode.Uri, start?: number, end?: number, focus = true) {
    const editor = vscode.window.activeTextEditor
    uri ??= editor?.document.uri
    const repo = uri?.scheme === 'file' ? repoOf(uri) : undefined
    if (!uri || !repo) {
      if (!focus && uri?.scheme !== REV) {
        lineKey = ''
        lines.set(undefined, '', [], '')
        lines.view.message = v.lineHistoryHint
      }
      return
    }
    start ??= editor!.selection.start.line + 1
    end ??= editor!.selection.end.line + (editor!.selection.end.character === 0 && editor!.selection.end.line >= start ? 0 : 1)
    const path = rel(repo, uri)
    if (focus) {
      await vscode.commands.executeCommand('pushright.lineHistory.focus')
      clearTimeout(lineTimer)
      lineAgain = false
    }
    const request = ++lineRequest
    const description = `${basename(path)}:${start}-${end}`
    lineHead = repo.refs.head_id
    const key = JSON.stringify([repo.tab, path, start, end, lineHead])
    if (lineKey === key) return
    lineKey = ''
    lines.set(repo, path, [], description)
    lines.view.message = v.lineHistoryLoading
    const contents = editor?.document.uri.toString() === uri.toString() ? editor.document.getText() : undefined
    const loader = (skip: number) => engine.call<Commit[]>('line_history_page', { tab: repo.tab, path, start, end, limit: LIMIT + 1, skip, contents })
    try {
      const items = await vscode.window.withProgress({ location: { viewId: 'pushright.lineHistory' } }, () =>
        loader(0),
      )
      if (request !== lineRequest || !repos.includes(repo)) return
      lineKey = key
      lines.set(repo, path, items, description)
      lines.loader = loader
    } catch (error) {
      if (request === lineRequest) {
        lineKey = ''
        lines.view.message = errorText(error)
      }
    }
  }

  /** 自动跟随不切走编辑器焦点；最多一项自动查询在途，连续移动只取最后的选区。 */
  function followLines() {
    clearTimeout(lineTimer)
    ++lineRequest
    if (!lines.view.visible) return
    lineTimer = setTimeout(async () => {
      if (lineBusy) { lineAgain = true; return }
      lineBusy = true
      try { await lineHistory(undefined, undefined, undefined, false) }
      finally {
        lineBusy = false
        if (lineAgain) { lineAgain = false; followLines() }
      }
    }, 120)
  }

  async function search() {
    const repo = activeRepo()
    if (!repo) return
    const kind = await vscode.window.showQuickPick(
      (['message', 'author', 'file', 'content', 'id'] as const).map((by) => ({ label: v.searchKinds[by], by })),
      { title: v.searchTitle },
    )
    const query = kind && (await vscode.window.showInputBox({ title: `${v.searchTitle} · ${kind.label}` }))
    if (!query || !repos.includes(repo)) return
    await vscode.commands.executeCommand('pushright.search.focus')
    const loader = (skip: number) => engine.call<Commit[]>('search', { tab: repo.tab, kind: kind.by, query, limit: LIMIT + 1, skip })
    const items = await vscode.window.withProgress({ location: { viewId: 'pushright.search' } }, () =>
      attempt(loader(0)),
    )
    if (repos.includes(repo)) { found.set(repo, '', items ?? [], `${kind.label}: ${query}`); found.loader = loader }
  }

  /** 当前文件与某个分支、标签或提交对比。 */
  async function compareWith() {
    const uri = vscode.window.activeTextEditor?.document.uri
    const repo = uri?.scheme === 'file' ? repoOf(uri) : undefined
    if (!uri || !repo) return
    const other = { label: `$(search) ${v.enterCommit}`, rev: '' }
    const pick = await vscode.window.showQuickPick(
      [other, ...repo.refs.refs.filter((r) => !r.name.endsWith('/HEAD')).map((r) => ({ label: short(r.name), rev: r.name }))],
      { title: v.compareTitle(basename(uri.fsPath)) },
    )
    const rev = pick === other ? await vscode.window.showInputBox({ title: v.enterCommit }) : pick?.rev
    if (rev) vscode.commands.executeCommand('vscode.diff', revUri(repo, rel(repo, uri), rev), uri, `${basename(uri.fsPath)} (${short(rev)} ↔ ${v.workingTree})`)
  }

  async function openAtRevision() {
    const uri = vscode.window.activeTextEditor?.document.uri
    const repo = uri?.scheme === 'file' ? repoOf(uri) : undefined
    if (!uri || !repo) return
    await loadFile(repo, rel(repo, uri))
    const pick = await vscode.window.showQuickPick(
      files.items.map((c) => ({ label: c.subject, description: `${c.author} · ${ago(c.time)}`, detail: c.id.slice(0, 8), c })),
      { title: v.openAtRevision },
    )
    if (pick) vscode.window.showTextDocument(revUri(repo, pick.c.path, pick.c.id))
  }

  const command = (id: string, fn: (...args: any[]) => unknown) => vscode.commands.registerCommand(`pushright.${id}`, fn)
  context.subscriptions.push(
    files.view, lines.view, found.view,
    onPrefsChange.event(() => { for (const list of [files, lines, found]) list.refresh() }),
    vscode.window.onDidChangeActiveTextEditor(follow),
    vscode.window.onDidChangeActiveTextEditor(followLines),
    vscode.window.onDidChangeTextEditorSelection((e) => { if (e.textEditor === vscode.window.activeTextEditor) followLines() }),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (e.document === vscode.window.activeTextEditor?.document) { lineKey = ''; followLines() }
    }),
    vscode.workspace.onDidSaveTextDocument((doc) => {
      if (doc === vscode.window.activeTextEditor?.document) { lineKey = ''; followLines() }
    }),
    lines.view.onDidChangeVisibility(followLines),
    { dispose: () => { clearTimeout(lineTimer); ++lineRequest; lineAgain = false } },
    files.view.onDidChangeVisibility(follow),
    onRepoChange.event((repo) => {
      if (repos.includes(repo)) return
      if (lines.repo === repo) { ++lineRequest; clearTimeout(lineTimer); lineAgain = false; lineKey = '' }
      for (const list of [files, lines, found]) if (list.repo === repo) list.set(undefined, '', [], '')
    }),
    // 有了新提交后文件历史要重取
    onRepoChange.event((repo) => files.repo === repo && files.view.visible && loadFile(repo, files.path)),
    onRepoChange.event((repo) => { if (lines.repo === repo && lineHead !== repo.refs.head_id) followLines() }),
    command('fileHistory', async () => {
      await vscode.commands.executeCommand('pushright.fileHistory.focus')
      follow()
    }),
    command('lineHistory', lineHistory),
    command('previousRevision', () => step(1)),
    command('nextRevision', () => step(-1)),
    command('compareWith', compareWith),
    command('openAtRevision', openAtRevision),
    command('searchCommits', search),
    command('loadMore', (list: CommitList) => attempt(list.loadMore())),
    command('item.openChanges', openChanges),
    command('item.reveal', (c: Commit) => vscode.commands.executeCommand('pushright.revealCommit', c.id, [files, lines, found].find((l) => l.items.includes(c))?.repo?.root)),
    command('item.openFile', (c: Commit) => {
      const list = [files, lines].find((l) => l.items.includes(c))
      return list?.repo && vscode.window.showTextDocument(revUri(list.repo, c.path, c.id))
    }),
  )
  follow()
  followLines()
}
