import { invoke as host, isTauri } from '@tauri-apps/api/core'
import type { DiffColors } from './diff-colors'

// 桌面版的引擎命令统一走 Tauri 的 `call`（命令表在 engine::rpc，与扩展的 sidecar 共用）；
// 扩展里 host 已经是发给扩展主机的 postMessage，原样转发。
const invoke: typeof host = isTauri() ? (cmd, args = {}) => host('call', { cmd, args }) : host

export const saveDiffColors = (colors: DiffColors | null) => host<void>('set_diff_colors', { colors })

export interface Row {
  id: string
  lane: number
  incoming: number[]
  through: number[]
  out: number[]
  author: string
  author_email: string
  time: number
  subject: string
}
export interface Ref {
  name: string
  id: string
  upstream: string | null
}
export interface Refs {
  head: string | null
  head_id: string | null
  /** 当前分支相对上游的 [领先, 落后] */
  ahead_behind: [number, number] | null
  in_progress: InProgress | null
  revert_disabled: boolean
  refs: Ref[]
}
export type InProgress = 'merge' | 'rebase' | 'cherry-pick' | 'revert'
export interface Log {
  command: string
  output: string
  ok: boolean
}
export type Op =
  | { op: 'checkout'; target: string }
  | { op: 'track'; remote_branch: string }
  | { op: 'create_branch'; name: string; start: string; checkout: boolean }
  | { op: 'delete_branch'; name: string; force: boolean }
  | { op: 'rename_branch'; old: string; new: string }
  | { op: 'merge'; target: string }
  | { op: 'rebase'; onto: string }
  | { op: 'cherry_pick'; id: string }
  | { op: 'revert'; id: string }
  | { op: 'set_revert_disabled'; disabled: boolean }
  | { op: 'reset'; target: string; mode: 'soft' | 'mixed' | 'hard' | 'keep' }
  | { op: 'discard'; paths: string[] }
  | { op: 'stash_push'; message: string; include_untracked: boolean }
  | { op: 'stash_apply'; name: string; pop: boolean }
  | { op: 'stash_drop'; name: string }
  | { op: 'worktree_add'; path: string; start: string; new_branch: string | null }
  | { op: 'worktree_remove'; path: string; force: boolean }
  | { op: 'fetch'; background?: boolean }
  | { op: 'pull'; rebase: boolean }
  | { op: 'push'; remote: string; branch: string; remote_branch: string; force: boolean; set_upstream: boolean }
  | { op: 'create_tag'; name: string; target: string; message: string }
  | { op: 'delete_tag'; name: string }
  | { op: 'remote_add'; name: string; url: string }
  | { op: 'remote_url'; name: string; url: string; push: boolean }
  | { op: 'remote_remove'; name: string }
  | { op: 'remote_rename'; old: string; new: string }
  | { op: 'continue'; what: InProgress }
  | { op: 'abort'; what: InProgress }
export interface Stash {
  name: string
  id: string
  subject: string
}
export interface Worktree {
  path: string
  head: string
  branch: string | null
  current: boolean
  changes: number
  ahead_behind: [number, number] | null
  subject: string
  time: number
}
export type Block =
  | { kind: 'text'; text: string }
  | { kind: 'conflict'; ours: string; theirs: string; ours_label: string; theirs_label: string }
export interface Conflict {
  binary: boolean
  blocks: Block[]
}
export type Side = 'ours' | 'theirs' | 'both'
export interface Track {
  name: string
  ahead: number
  behind: number
  gone: boolean
}
export interface FileChange {
  status: string
  path: string
  old_path: string | null
}
export interface Detail {
  id: string
  parents: string[]
  author: string
  author_email: string
  author_time: number
  committer: string
  committer_time: number
  message: string
  files: FileChange[]
}

export interface Entry {
  path: string
  old_path: string | null
  staged: string | null
  unstaged: string | null
  conflicted: boolean
}
export interface Identity {
  name: string
  email: string
  author: string | null
  committer: string | null
}
export interface Line {
  kind: string
  text: string
  old_no: number | null
  new_no: number | null
}
export interface Hunk {
  header: string
  lines: Line[]
}
export interface Diff {
  binary: boolean
  too_large: boolean
  hunks: Hunk[]
  images?: [string | null, string | null] | null
  next?: number | null
  paged?: boolean
  clipped?: boolean
}

export interface Comparison { left: string; right: string; files: FileChange[] }
export interface Remote { name: string; fetch: string; push: string }
export interface Reflog { id: string; selector: string; subject: string }
export interface RebaseStep { id: string; subject: string; action: string; message: string }
export interface RebasePlan { head: string; base: string; steps: RebaseStep[] }
export const createRepo = (path: string, url: string | null) => invoke<string>('create_repo', { path, url })
export const compare = (tab: number, left: string, right: string, commonBase: boolean) => invoke<Comparison>('compare', { tab, left, right, commonBase })
export const diffBetween = (tab: number, left: string, right: string, path: string, oldPath: string | null) => invoke<Diff>('diff_between', { tab, left, right, path, oldPath })
export const diffPage = (tab: number, mode: string, left: string, right: string, path: string, skip: number, oldPath: string | null = null) => invoke<Diff>('diff_page', { tab, mode, left, right, path, skip, oldPath })
export const remoteDetails = (tab: number) => invoke<Remote[]>('remote_details', { tab })
export const reflog = (tab: number, skip = 0) => invoke<Reflog[]>('reflog', { tab, skip, limit: 201 })
export const rebasePlan = (tab: number, base: string) => invoke<RebasePlan>('rebase_plan', { tab, base })
export const rebaseRun = (tab: number, plan: RebasePlan) => invoke<Log>('rebase_run', { tab, ...plan })

export const initialRepos = () => host<string[]>('initial_repos')
/** 返回 [页签 id, 工作区路径, 已加载的提交数] */
export const openRepo = (path: string) => invoke<[number, string, number]>('open_repo', { path })
export const closeRepo = (tab: number) => invoke<void>('close_repo', { tab })
export const loadGraph = (tab: number, full: boolean) => invoke<number>('load_graph', { tab, full })
export const rows = (tab: number, start: number, count: number) => invoke<Row[]>('rows', { tab, start, count })
export const rowOf = (tab: number, id: string) => invoke<number | null>('row_of', { tab, id })
export const refs = (tab: number) => invoke<Refs>('refs', { tab })
export const stashes = (tab: number) => invoke<Stash[]>('stashes', { tab })
export const worktrees = (tab: number) => invoke<Worktree[]>('worktrees', { tab })
export const detail = (tab: number, id: string) => invoke<Detail>('detail', { tab, id })
export const stashDetail = (tab: number, name: string) => invoke<Detail>('stash_detail', { tab, name })
export const status = (tab: number) => invoke<Entry[]>('status', { tab })
export const stage = (tab: number, paths: string[]) => invoke<void>('stage', { tab, paths })
export const unstage = (tab: number, paths: string[]) => invoke<void>('unstage', { tab, paths })
export const commit = (tab: number, message: string, amend: boolean) => invoke<void>('commit', { tab, message, amend })
export const lastMessage = (tab: number) => invoke<string>('last_message', { tab })
export const commitIdentity = (tab: number) => invoke<Identity>('commit_identity', { tab })
export const setCommitIdentity = (tab: number, name: string, email: string) =>
  invoke<Identity>('set_commit_identity', { tab, name, email })
export const diffWorktree = (tab: number, path: string, staged: boolean, untracked: boolean) =>
  invoke<Diff>('diff_worktree', { tab, path, staged, untracked })
export const diffCommit = (tab: number, id: string, path: string) => invoke<Diff>('diff_commit', { tab, id, path })
export const stashDiff = (tab: number, id: string, path: string) => invoke<Diff>('stash_diff', { tab, id, path })
export const applyLines = (tab: number, path: string, staged: boolean, hunk: number, header: string, lines: number[]) =>
  invoke<void>('apply_lines', { tab, path, staged, hunk, header, lines })
export const op = (tab: number, op: Op) => invoke<Log>('op', { tab, op })
export const remotes = (tab: number) => invoke<string[]>('remotes', { tab })
export const tracking = (tab: number) => invoke<Track[]>('tracking', { tab })
export const discardLines = (tab: number, path: string, hunk: number, header: string, lines: number[]) =>
  invoke<void>('discard_lines', { tab, path, hunk, header, lines })
export const conflictRead = (tab: number, path: string) => invoke<Conflict>('conflict_read', { tab, path })
export const conflictResolve = (tab: number, path: string, choices: Side[]) => invoke<void>('conflict_resolve', { tab, path, choices })
export const conflictTake = (tab: number, path: string, theirs: boolean) => invoke<void>('conflict_take', { tab, path, theirs })
