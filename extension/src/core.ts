import { basename, dirname, join, relative, sep } from 'node:path'
import { readdir, stat } from 'node:fs/promises'
import * as vscode from 'vscode'
import type { Entry, Identity, Log, Op, Operation, Refs, WorkflowSettings } from '../../src/lib/api'
import { explanations as enHelp, messages as en } from '../../src/lib/locales/en'
import { explanations as zhHelp, messages as zh } from '../../src/lib/locales/zh'
import type { AuthorStyle } from '../../src/lib/prefs.svelte'
import { startEngine, type Engine } from './engine'
import { formatTime, readTimeZone } from '../../src/lib/time'

const chinese = vscode.env.language.toLowerCase().startsWith('zh')
export const locale = chinese ? 'zh-CN' : 'en'
/** 与桌面版共用的语言包；`v` 是只有扩展用到的那部分 */
export const t: typeof zh = chinese ? zh : en
export const help: typeof zhHelp = chinese ? zhHelp : enHelp
export const v = t.vscode

/** 只翻译应用自己的错误码；Git 的输出保持原文。 */
export function errorText(error: unknown): string {
  const text = String(error)
  const [code, ...detail] = text.split(': ')
  return Object.hasOwn(t.errors, code) ? t.errors[code] + (detail.length ? ': ' + detail.join(': ') : '') : text
}

export interface Repo {
  /** sidecar 里的会话 id */
  tab: number
  root: string
  name: string
  refs: Refs
  /** 进行中的合并 / 变基 / 摘取 / 撤销在 Git 里记下的信息；没有时为 null */
  operation: Operation | null
  status: Entry[]
  identity: Identity | null
  remotes: string[]
  /** 提交信息模板和提交前检查（仓库设置里的） */
  workflow: WorkflowSettings | null
}

export const repos: Repo[] = []
export const closedRepos = new Map<string, string>()
/** 状态更新或仓库关闭；接收方通过 repos 判断仓库是否仍然打开。 */
export const onRepoChange = new vscode.EventEmitter<Repo>()
export const onPrefsChange = new vscode.EventEmitter<void>()
export const log = vscode.window.createOutputChannel('PushRight')
export let engine: Engine
let context: vscode.ExtensionContext
/** 提交图面板打开时由它填上，用来通知面板 */
export const panel: { post?: (message: object) => void } = {}

export async function openRepos(ctx: vscode.ExtensionContext, exe: string) {
  context = ctx
  for (const root of context.workspaceState.get<string[]>('closedRepositories', [])) closedRepos.set(fold(root), root)
  engine = startEngine(exe)
  await discoverRepositories()
}

let discovering: Promise<void> | undefined
export function discoverRepositories(extra: string[] = []): Promise<void> {
  if (discovering) return discovering.then(() => extra.length ? discoverRepositories(extra) : undefined)
  discovering = discover(extra).finally(() => discovering = undefined)
  return discovering
}

async function discover(extra: string[]) {
  const queue = [...(vscode.workspace.workspaceFolders ?? []).map((f) => f.uri.fsPath), ...repos.map((r) => r.root), ...extra]
  const seen = new Set<string>()
  for (const path of queue) {
    if (seen.has(fold(path)) || closedRepos.has(fold(path))) continue
    seen.add(fold(path))
    try {
      let repo = repos.find((r) => fold(r.root) === fold(path))
      let loading: Promise<void> | undefined
      if (!repo) {
        const [tab, actual] = await engine.call<[number, string, number]>('open_repo', { path })
        const root = vscode.Uri.file(actual).fsPath
        if (closedRepos.has(fold(root))) { await engine.call('close_repo', { tab }); continue }
        repo = repos.find((r) => fold(r.root) === fold(root))
        if (repo) await engine.call('close_repo', { tab })
        else {
          const parent = repos.filter((r) => fold(root).startsWith(fold(r.root) + sep)).sort((a, b) => b.root.length - a.root.length)[0]
          repo = {
            tab, root, name: parent ? `${parent.name}/${relative(parent.root, root).split(sep).join('/')}` : basename(root), status: [], identity: null, remotes: [], operation: null, workflow: null,
            refs: { head: null, head_id: null, ahead_behind: null, in_progress: null, revert_disabled: false, refs: [] },
          }
          repos.push(repo)
          loading = refresh(repo)
        }
      }
      // 读状态和找子仓库互不依赖，一起跑
      const [nested] = await Promise.all([engine.call<string[]>('repositories', { tab: repo.tab }), loading])
      if (repos.includes(repo)) queue.push(...nested)
    } catch {
      // 工作区本身可能是装着多个仓库的容器；只扫描它的直接子目录。
      if (vscode.workspace.workspaceFolders?.some((f) => fold(f.uri.fsPath) === fold(path))) {
        const dirs = await readdir(path, { withFileTypes: true }).catch(() => [])
        queue.push(...dirs.filter((d) => d.isDirectory() && !d.name.startsWith('.')).map((d) => join(path, d.name)))
      }
    }
  }
  await vscode.commands.executeCommand('setContext', 'pushright.hasRepo', repos.length > 0)
  await vscode.commands.executeCommand('setContext', 'pushright.hasClosedRepos', closedRepos.size > 0)
}

/** 仅关闭当前窗口里的仓库会话；记住选择，避免文件切换或刷新时重新发现。 */
export async function closeRepo(repo: Repo) {
  const index = repos.indexOf(repo)
  if (index < 0) return
  closedRepos.set(fold(repo.root), repo.root)
  repos.splice(index, 1)
  onRepoChange.fire(repo)
  await Promise.all([
    context.workspaceState.update('closedRepositories', [...closedRepos.values()]),
    engine.call('close_repo', { tab: repo.tab }),
    vscode.commands.executeCommand('setContext', 'pushright.hasRepo', repos.length > 0),
    vscode.commands.executeCommand('setContext', 'pushright.hasClosedRepos', true),
  ])
}

export async function reopenRepo(root: string) {
  if (!closedRepos.delete(fold(root))) return
  await context.workspaceState.update('closedRepositories', [...closedRepos.values()])
  await discoverRepositories([root])
}

/** 打开被父仓库忽略的嵌套仓库文件时，也要按最近的 .git 归属，不能在父仓库执行操作。 */
export async function discoverForFile(uri: vscode.Uri) {
  if (uri.scheme !== 'file') return
  const known = repoOf(uri)
  const start = await stat(uri.fsPath).then((s) => s.isDirectory() ? uri.fsPath : dirname(uri.fsPath), () => dirname(uri.fsPath))
  for (let dir = start; dir !== dirname(dir) && fold(dir) !== fold(known?.root ?? ''); dir = dirname(dir)) {
    if (closedRepos.has(fold(dir))) return
    if (await stat(join(dir, '.git')).then(() => true, () => false)) { await discoverRepositories([dir]); break }
  }
}

const refreshing = new WeakMap<Repo, { again: boolean; git: boolean; notify: boolean }>()

/**
 * 重新读取引用和工作区状态。进行中又被调用时，结束后再读一次，期间的多次调用合并。
 * `git` 为 false 表示只有工作区文件变了：引用、身份、远程不会变，只重读状态（少开 8 个 git 进程）；
 * `notify` 为 false 时状态没变就不通知，免得保存文件、切回窗口让各个视图白白重画。
 */
export async function refresh(repo: Repo, git = true, notify = git) {
  if (!repos.includes(repo)) return
  const running = refreshing.get(repo)
  if (running) return void Object.assign(running, { again: true, git: running.git || git, notify: running.notify || notify })
  const state = { again: false, git, notify }
  refreshing.set(repo, state)
  do {
    ;({ git, notify } = state)
    Object.assign(state, { again: false, git: false, notify: false })
    const a = { tab: repo.tab }
    const before = notify || JSON.stringify(repo.status)
    try {
      if (git) {
        ;[repo.refs, repo.status, repo.identity, repo.remotes, repo.workflow] = await Promise.all([
          engine.call<Refs>('refs', a),
          engine.call<Entry[]>('status', a),
          engine.call<Identity>('commit_identity', a).catch(() => null),
          engine.call<string[]>('remotes', a),
          engine.call<WorkflowSettings>('workflow_settings', a).catch(() => null),
        ])
        repo.operation = repo.refs.in_progress ? await engine.call<Operation | null>('operation', a) : null
      } else repo.status = await engine.call<Entry[]>('status', a)
    } catch (e) {
      log.appendLine(errorText(e))
    }
    if (repos.includes(repo) && (notify || JSON.stringify(repo.status) !== before)) onRepoChange.fire(repo)
  } while (state.again && repos.includes(repo))
  refreshing.delete(repo)
}

/** 出错时提示并返回 undefined。 */
export async function attempt<T>(promise: Promise<T>): Promise<T | undefined> {
  try {
    return await promise
  } catch (e) {
    vscode.window.showErrorMessage(errorText(e))
  }
}

/** 调一个会改动仓库的引擎命令，然后刷新。失败时返回 undefined。 */
export async function write<T>(repo: Repo, cmd: string, args: object): Promise<T | undefined> {
  const result = await attempt(engine.call<T>(cmd, { tab: repo.tab, ...args }))
  await refresh(repo)
  panel.post?.({ refresh: true })
  return result
}

/** 执行一个 Git 操作，命令和输出记进「PushRight」输出面板。 */
export async function run(repo: Repo, op: Op): Promise<boolean> {
  const result = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Window, title: `PushRight: ${t.opNames[op.op] ?? op.op}` },
    () => write<Log>(repo, 'op', { op }),
  )
  if (!result) return false
  log.appendLine(`> ${result.command}\n${result.output}\n`)
  if (!result.ok) {
    vscode.window
      .showErrorMessage(result.output.split('\n').find((l) => /^(fatal|error|hint|CONFLICT)/.test(l)) ?? result.output.split('\n')[0] ?? result.command, t.log)
      .then((show) => show && log.show())
  }
  return result.ok
}

const fold = (path: string) => (process.platform === 'win32' ? path.toLowerCase() : path)

/** 文件所在的仓库；嵌套时取最里层。 */
export function repoOf(uri: vscode.Uri | undefined): Repo | undefined {
  if (!uri || (uri.scheme !== 'file' && uri.scheme !== REV)) return
  const file = fold(uri.fsPath)
  const repo = repos
    .filter((r) => file === fold(r.root) || file.startsWith(fold(r.root) + sep))
    .sort((a, b) => b.root.length - a.root.length)[0]
  // 关闭子仓库后，其文件不能落到仍打开的父仓库上执行 Git 操作。
  for (const root of closedRepos.keys()) {
    if ((!repo || root.length > repo.root.length) && (file === root || file.startsWith(root + sep))) return
  }
  return repo
}

export function activeRepo(): Repo | undefined {
  const repo = repoOf(vscode.window.activeTextEditor?.document.uri) ?? repos[0]
  if (!repo) vscode.window.showWarningMessage(v.noRepo)
  return repo
}

/** 相对仓库根目录的路径，用正斜杠 */
export const rel = (repo: Repo, uri: vscode.Uri) => relative(repo.root, uri.fsPath).split(sep).join('/')
export const fileUri = (repo: Repo, path: string) => vscode.Uri.file(join(repo.root, path))

export const REV = 'pushright-rev'
/** 文件在某个版本的内容；`rev` 为空表示暂存区 */
export const revUri = (repo: Repo, path: string, rev: string, empty?: boolean) => fileUri(repo, path).with({ scheme: REV, query: JSON.stringify({ rev, empty }) })
export const revOf = (uri: vscode.Uri): string | undefined => (uri.scheme === REV ? JSON.parse(uri.query).rev : undefined)

/** 当前分支的短名；游离 HEAD 时为 null */
export const headBranch = (repo: Repo) => (repo.refs.head?.startsWith('refs/heads/') ? repo.refs.head.slice(11) : null)
export const short = (name: string) => name.replace(/^refs\/(heads|tags|remotes)\//, '')
export const cfg = <T>(key: string) => vscode.workspace.getConfiguration('pushright').get<T>(key)!

export function ago(sec: number): string {
  const format = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })
  const delta = sec - Date.now() / 1000
  for (const [unit, size] of [['year', 31536000], ['month', 2592000], ['day', 86400], ['hour', 3600], ['minute', 60]] as const) {
    if (Math.abs(delta) >= size) return format.format(Math.round(delta / size), unit)
  }
  return format.format(0, 'minute')
}
let savedTimeZone: string | undefined
let timeZone = ''
let dateFormatter: Intl.DateTimeFormat
export function date(sec: number, dateOnly = false): string {
  const saved = stored().timeZone ?? ''
  if (saved !== savedTimeZone) {
    savedTimeZone = saved
    timeZone = readTimeZone(saved)
    dateFormatter = new Intl.DateTimeFormat(locale, { timeZone: timeZone || undefined, year: 'numeric', month: 'numeric', day: 'numeric', hour: 'numeric', minute: 'numeric', second: 'numeric' })
  }
  return dateOnly ? formatTime(sec, timeZone).slice(0, 10) : dateFormatter.format(new Date(sec * 1000))
}

// 界面偏好与提交图面板共用一份，存在 globalState 的 webview 键下（键和值就是面板的 localStorage）

export const stored = () => context.globalState.get<Record<string, string>>('webview', {})

export async function store(key: string, value: string | null) {
  const all = { ...stored() }
  if (value === null) delete all[key]
  else all[key] = value
  await context.globalState.update('webview', all)
  if (key === 'prefs' || key === 'diffColors' || key === 'timeZone') onPrefsChange.fire()
}

export function authorStyles(): Record<string, AuthorStyle> {
  try {
    return JSON.parse(stored().prefs ?? '{}').authorStyles ?? {}
  } catch {
    return {}
  }
}

export async function setAuthorStyle(key: string, style: AuthorStyle) {
  const styles = authorStyles()
  if (style.border || style.fill) styles[key] = style
  else delete styles[key]
  const value = JSON.stringify({ authorStyles: styles })
  await store('prefs', value)
  panel.post?.({ store: 'prefs', value })
}
