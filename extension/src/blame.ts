import { basename } from 'node:path'
import * as vscode from 'vscode'
import { authorColor, authorKey } from '../../src/lib/author'
import { ago, authorStyles, cfg, date, log, onPrefsChange, onRepoChange, rel, repoOf, repos, REV, revOf, revUri, setAuthorStyle, t, v, type Repo } from './core'
import { isUncommitted, runBlame, type Blame, type BlameCommit } from './git-blame'

/** 作者色，与 src/app.css 的 --author-0..9 一致（tests/author-palette.test.mjs 核对） */
export const PALETTE = {
  dark: ['#e59a7f', '#9cc281', '#e3bd6e', '#c1a6e3', '#84b1dc', '#6fc4b5', '#e79abb', '#c9b98a', '#9fb0e8', '#d6a36a'],
  light: ['#9a4522', '#40682f', '#77540a', '#63469a', '#2b5e93', '#1c6c60', '#973a66', '#63552a', '#4250a3', '#84511a'],
}

function colorOf(author: string): string {
  const kind = vscode.window.activeColorTheme.kind
  const light = kind === vscode.ColorThemeKind.Light || kind === vscode.ColorThemeKind.HighContrastLight
  // authorColor 返回 var(--author-N)，取出 N
  return PALETTE[light ? 'light' : 'dark'][+/\d+/.exec(authorColor(author))![0]]
}

const END = Number.MAX_SAFE_INTEGER
const link = (command: string, ...args: unknown[]) => `command:pushright.${command}?${encodeURIComponent(JSON.stringify(args))}`

function lineText(c: BlameCommit): string {
  if (isUncommitted(c)) return v.uncommitted
  const values: Record<string, string> = { author: c.author, ago: ago(c.time), date: date(c.time), subject: c.subject, id: c.id.slice(0, 8) }
  return cfg<string>('blame.line.format').replace(/\$\{(\w+)\}/g, (all, key) => values[key] ?? all)
}

function hover(root: string, c: BlameCommit): vscode.MarkdownString | undefined {
  if (isUncommitted(c)) return
  const md = new vscode.MarkdownString(undefined, true)
  md.isTrusted = { enabledCommands: ['pushright.openCommitFile', 'pushright.revealCommit', 'pushright.copyId', 'pushright.authorStyle'] }
  md.appendMarkdown(`$(person) **${c.author.replace(/[\\`*_{}[\]<>]/g, '\\$&')}** · ${ago(c.time)} · ${date(c.time)}\n\n`)
  md.appendText(c.subject)
  md.appendMarkdown(
    `\n\n\`${c.id.slice(0, 8)}\` · [${v.openChanges}](${link('openCommitFile', root, c.id, c.path, c.previous)})` +
      ` · [${v.showInGraph}](${link('revealCommit', c.id, root)}) · [${t.copyId}](${link('copyId', c.id)})` +
      ` · [${t.authorStyle(c.author.replace(/[[\]]/g, ''))}](${link('authorStyle', authorKey(c), c.author)})`,
  )
  return md
}

export function registerBlame(context: vscode.ExtensionContext) {
  interface Entry {
    repo: Repo
    version: number
    blame: Blame
    cancel: () => void
  }
  const cache = new Map<string, Entry>()
  /** 开着整文件 blame 的文档 */
  const fileBlame = new Set<string>()
  /** 文档 → 只高亮的那位作者 */
  const focus = new Map<string, { key: string; type: vscode.TextEditorDecorationType }>()
  const lensChanged = new vscode.EventEmitter<void>()

  const lineType = vscode.window.createTextEditorDecorationType({ after: { margin: '0 0 0 3em' } })
  // contentText 之外的布局靠 textDecoration 注入：固定宽度，超出截断
  const gutterType = vscode.window.createTextEditorDecorationType({
    before: { width: '26ch', margin: '0 1.5em 0 0', textDecoration: 'none; display: inline-block; overflow: hidden; white-space: pre; padding: 0 0.5ch; box-sizing: border-box' },
  })
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 200)

  const current = (doc: vscode.TextDocument): Blame | undefined => {
    const entry = cache.get(doc.uri.toString())
    return entry?.version === doc.version ? entry.blame : undefined
  }

  function request(doc: vscode.TextDocument) {
    const repo = repoOf(doc.uri)
    if (doc.uri.scheme === REV && JSON.parse(doc.uri.query).empty) return
    const rev = revOf(doc.uri)
    const key = doc.uri.toString()
    const old = cache.get(key)
    if (!repo || old?.version === doc.version) return
    old?.cancel()
    const entry: Entry = { repo, version: doc.version, blame: { lines: [], done: false }, cancel: () => {} }
    cache.set(key, entry)
    const contents = rev === '' || doc.isDirty ? doc.getText() : undefined
    entry.cancel = runBlame(repo.root, rel(repo, doc.uri), contents, (blame) => {
      entry.blame = blame
      for (const editor of vscode.window.visibleTextEditors) if (editor.document === doc) render(editor, true)
      if (blame.done) lensChanged.fire()
    }, rev, (message) => log.appendLine(`[Blame] ${repo.root}\n${message}`))
  }

  function drop(filter: (uri: vscode.Uri, entry: Entry) => boolean) {
    for (const [key, entry] of cache) {
      if (!filter(vscode.Uri.parse(key), entry)) continue
      entry.cancel()
      cache.delete(key)
    }
  }

  /** 行末注释和状态栏每次光标移动都更新；整文件的装饰只在数据或设置变化时重算（`all`）。 */
  function render(editor: vscode.TextEditor, all: boolean) {
    const doc = editor.document
    const key = doc.uri.toString()
    const blame = current(doc)
    const root = repoOf(doc.uri)?.root ?? ''
    const styles = authorStyles()
    const styleOf = (c: BlameCommit) => (isUncommitted(c) ? undefined : styles[authorKey(c)])

    const line = editor.selection.active.line
    const commit = blame?.lines[line]
    if (editor === vscode.window.activeTextEditor) {
      if (commit && cfg<boolean>('blame.statusBar.enabled')) {
        status.text = `$(person) ${isUncommitted(commit) ? v.uncommitted : `${commit.author}, ${ago(commit.time)}`}`
        status.tooltip = commit.subject
        status.command = isUncommitted(commit) ? undefined : { command: 'pushright.revealCommit', title: '', arguments: [commit.id, root] }
        status.show()
      } else status.hide()
    }
    const style = commit && styleOf(commit)
    const color = commit && style ? colorOf(commit.author) : undefined
    editor.setDecorations(
      lineType,
      commit && cfg<boolean>('blame.line.enabled') && !fileBlame.has(key)
        ? [{
            range: new vscode.Range(line, END, line, END),
            hoverMessage: hover(root, commit),
            renderOptions: {
              after: {
                contentText: lineText(commit),
                color: color ?? new vscode.ThemeColor('editorCodeLens.foreground'),
                backgroundColor: style?.fill ? color + '33' : undefined,
                border: style?.border ? `1px solid ${color}` : undefined,
              },
            },
          }]
        : [],
    )
    if (!all) return

    const gutter: vscode.DecorationOptions[] = []
    if (blame && fileBlame.has(key)) {
      const byAge = cfg<string>('blame.file.colors') === 'age'
      let oldest = Infinity, newest = -Infinity
      if (byAge) for (const c of blame.lines) {
        if (!c || isUncommitted(c)) continue
        oldest = Math.min(oldest, c.time)
        newest = Math.max(newest, c.time)
      }
      if (oldest === Infinity) oldest = newest = 0
      const span = newest - oldest || 1
      for (let i = 0; i < doc.lineCount; i++) {
        const c = blame.lines[i]
        if (!c) continue
        const first = blame.lines[i - 1] !== c
        const author = isUncommitted(c) ? v.uncommitted : c.author
        const tint = isUncommitted(c) ? '#888888' : colorOf(c.author)
        const s = styleOf(c)
        // 按新旧上色时越新越深；按作者上色时设置了底色的作者更醒目
        const alpha = byAge ? 0x10 + Math.round((0x70 * ((c.time || oldest) - oldest)) / span) : s?.fill ? 0x70 : 0x22
        gutter.push({
          range: new vscode.Range(i, 0, i, 0),
          hoverMessage: first ? hover(root, c) : undefined,
          renderOptions: {
            before: {
              contentText: first ? `${author} · ${isUncommitted(c) ? '' : date(c.time, true)}` : ' ',
              color: new vscode.ThemeColor('editor.foreground'),
              backgroundColor: (byAge ? '#d9822b' : tint) + alpha.toString(16).padStart(2, '0'),
              border: `none; border-left: 3px solid ${tint}${s?.border ? `; outline: 1px solid ${tint}; outline-offset: -1px` : ''}`,
            },
          },
        })
      }
    }
    editor.setDecorations(gutterType, gutter)

    const focused = focus.get(key)
    if (focused) {
      const ranges: vscode.Range[] = []
      blame?.lines.forEach((c, i) => {
        if (!c || isUncommitted(c) || authorKey(c) !== focused.key) return
        const last = ranges.at(-1)
        // 相邻的行并成一段
        if (last?.end.line === i - 1) ranges[ranges.length - 1] = new vscode.Range(last.start, new vscode.Position(i, 0))
        else ranges.push(new vscode.Range(i, 0, i, 0))
      })
      editor.setDecorations(focused.type, ranges)
    }
  }

  const renderAll = () => vscode.window.visibleTextEditors.forEach((editor) => render(editor, true))
  const blamedLine = (): [vscode.TextEditor, BlameCommit] | undefined => {
    const editor = vscode.window.activeTextEditor
    const commit = editor && current(editor.document)?.lines[editor.selection.active.line]
    return editor && commit && !isUncommitted(commit) ? [editor, commit] : undefined
  }

  async function authorStyle(key?: string, name?: string) {
    if (!key) {
      const line = blamedLine()
      if (!line) return void vscode.window.showInformationMessage(v.noBlame)
      key = authorKey(line[1])
      name = line[1].author
    }
    const style = authorStyles()[key]
    const picked = await vscode.window.showQuickPick(
      [{ label: t.authorBorder, picked: style?.border }, { label: t.authorFill, picked: style?.fill }],
      { canPickMany: true, title: t.authorStyle(name ?? '') },
    )
    if (picked) await setAuthorStyle(key, { border: picked.some((p) => p.label === t.authorBorder), fill: picked.some((p) => p.label === t.authorFill) })
  }

  function focusAuthor() {
    const editor = vscode.window.activeTextEditor
    if (!editor) return
    const key = editor.document.uri.toString()
    const old = focus.get(key)
    old?.type.dispose()
    focus.delete(key)
    const line = blamedLine()
    if (!line) return void (old || vscode.window.showInformationMessage(v.noBlame))
    // 对同一位作者再执行一次是取消
    if (old?.key === authorKey(line[1])) return
    const color = colorOf(line[1].author)
    const type = vscode.window.createTextEditorDecorationType({
      isWholeLine: true,
      backgroundColor: color + '33',
      overviewRulerColor: color,
      overviewRulerLane: vscode.OverviewRulerLane.Full,
    })
    focus.set(key, { key: authorKey(line[1]), type })
    render(editor, true)
    vscode.window.setStatusBarMessage(t.onlyAuthor(line[1].author), 4000)
  }

  function openCommitFile(root: string, id: string, path: string, previous?: string) {
    const repo = repoOf(vscode.Uri.file(root))
    if (!repo) return
    const [beforeId, beforePath] = previous ? [previous.slice(0, 40), previous.slice(41)] : [`${id}^`, path]
    vscode.commands.executeCommand('vscode.diff', revUri(repo, beforePath, beforeId), revUri(repo, path, id), `${basename(path)} (${id.slice(0, 8)})`)
  }

  const KINDS = new Set([
    vscode.SymbolKind.Class, vscode.SymbolKind.Interface, vscode.SymbolKind.Enum, vscode.SymbolKind.Struct,
    vscode.SymbolKind.Function, vscode.SymbolKind.Method, vscode.SymbolKind.Constructor,
  ])
  type AnySymbol = vscode.DocumentSymbol | vscode.SymbolInformation
  const flatten = (symbols: AnySymbol[]): AnySymbol[] => symbols.flatMap((s) => [s, ...flatten('children' in s ? s.children : [])])

  async function codeLenses(doc: vscode.TextDocument): Promise<vscode.CodeLens[]> {
    const blame = current(doc)
    // 没算完时先不显示，算完后 lensChanged 会让编辑器重新来取
    if (!cfg<boolean>('codeLens.enabled') || !blame?.done) return []
    const symbols = (await vscode.commands.executeCommand<AnySymbol[] | undefined>('vscode.executeDocumentSymbolProvider', doc.uri)) ?? []
    const ranges = [
      new vscode.Range(0, 0, doc.lineCount - 1, 0),
      ...flatten(symbols).filter((s) => KINDS.has(s.kind)).map((s) => ('range' in s ? s.range : s.location.range)),
    ]
    return ranges.flatMap((range, i) => {
      const commits = new Set(blame.lines.slice(range.start.line, range.end.line + 1).filter((c): c is BlameCommit => !!c && !isUncommitted(c)))
      if (!commits.size) return []
      const latest = [...commits].reduce((a, b) => (b.time > a.time ? b : a))
      const authors = new Set([...commits].map(authorKey)).size
      const at = new vscode.Range(range.start.line, 0, range.start.line, 0)
      // 第一个是整份文件，点开文件历史；其余点开这段代码的行历史
      const open = i === 0
        ? { command: 'pushright.fileHistory', arguments: [] as unknown[] }
        : { command: 'pushright.lineHistory', arguments: [doc.uri, range.start.line + 1, range.end.line + 1] }
      return [
        new vscode.CodeLens(at, { ...open, title: `${latest.author}, ${ago(latest.time)}`, tooltip: latest.subject }),
        new vscode.CodeLens(at, { ...open, title: v.authors(authors) }),
      ]
    })
  }

  let editTimer: ReturnType<typeof setTimeout> | undefined
  const heads = new Map<Repo, string | null>()
  const command = (id: string, fn: (...args: any[]) => unknown) => vscode.commands.registerCommand(`pushright.${id}`, fn)

  context.subscriptions.push(
    lineType, gutterType, status, lensChanged,
    { dispose: () => drop(() => true) },
    vscode.window.onDidChangeActiveTextEditor((editor) => {
      if (!editor) return void status.hide()
      request(editor.document)
      render(editor, true)
    }),
    vscode.window.onDidChangeVisibleTextEditors((editors) => {
      for (const editor of editors) {
        request(editor.document)
        render(editor, true)
      }
    }),
    vscode.window.onDidChangeTextEditorSelection((e) => render(e.textEditor, false)),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (!e.contentChanges.length) return
      const editors = vscode.window.visibleTextEditors.filter((editor) => editor.document === e.document)
      if (!editors.length) return
      // 行号已经对不上了：先清掉，停手 600 毫秒后重算
      for (const editor of editors) render(editor, true)
      if (e.document.uri.scheme === REV) return request(e.document)
      clearTimeout(editTimer)
      editTimer = setTimeout(() => request(e.document), 600)
    }),
    vscode.workspace.onDidCloseTextDocument((doc) => {
      const key = doc.uri.toString()
      drop((uri) => uri.toString() === key)
      fileBlame.delete(key)
      focus.get(key)?.type.dispose()
      focus.delete(key)
    }),
    // 有了新提交（或切了分支），原来「尚未提交」的行有了归属
    onRepoChange.event((repo) => {
      if (!repos.includes(repo)) {
        heads.delete(repo)
        drop((_, entry) => entry.repo === repo)
        renderAll()
        lensChanged.fire()
        return
      }
      if (heads.get(repo) === repo.refs.head_id) return
      heads.set(repo, repo.refs.head_id)
      drop((uri) => repoOf(uri) === repo)
      for (const editor of vscode.window.visibleTextEditors) if (repoOf(editor.document.uri) === repo) request(editor.document)
    }),
    onPrefsChange.event(renderAll),
    vscode.window.onDidChangeActiveColorTheme(renderAll),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (!e.affectsConfiguration('pushright')) return
      renderAll()
      lensChanged.fire()
    }),
    vscode.languages.registerCodeLensProvider({ scheme: 'file' }, { onDidChangeCodeLenses: lensChanged.event, provideCodeLenses: codeLenses }),
    command('toggleFileBlame', () => {
      const editor = vscode.window.activeTextEditor
      if (!editor) return
      const key = editor.document.uri.toString()
      if (!fileBlame.delete(key)) fileBlame.add(key)
      request(editor.document)
      render(editor, true)
    }),
    command('toggleLineBlame', () => {
      const config = vscode.workspace.getConfiguration('pushright')
      return config.update('blame.line.enabled', !config.get('blame.line.enabled'), vscode.ConfigurationTarget.Global)
    }),
    command('focusAuthor', focusAuthor),
    command('authorStyle', authorStyle),
    command('openCommitFile', openCommitFile),
    command('copyId', (id: string | { id: string }) => vscode.env.clipboard.writeText(typeof id === 'string' ? id : id.id)),
  )
  for (const repo of repos) heads.set(repo, repo.refs.head_id)
  for (const editor of vscode.window.visibleTextEditors) request(editor.document)
}
