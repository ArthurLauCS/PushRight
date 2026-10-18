// 引擎命令返回的数据结构（命令表在 crates/engine/src/rpc.rs），供 VS Code 扩展的主机和侧栏提交图使用。

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
  | { op: 'stash_push'; message: string; include_untracked: boolean; paths?: string[] }
  | { op: 'stash_apply'; name: string; pop: boolean }
  | { op: 'stash_drop'; name: string }
  | { op: 'worktree_add'; path: string; start: string; new_branch: string | null }
  | { op: 'worktree_remove'; path: string; force: boolean }
  | { op: 'fetch'; background?: boolean }
  | { op: 'pull'; rebase: boolean; autostash?: boolean }
  | { op: 'fast_forward'; branch: string }
  | { op: 'push'; remote: string; branch: string; remote_branch: string; force: boolean; set_upstream: boolean; tags?: boolean }
  | { op: 'create_tag'; name: string; target: string; message: string }
  | { op: 'delete_tag'; name: string }
  | { op: 'remote_add'; name: string; url: string }
  | { op: 'remote_url'; name: string; url: string; push: boolean }
  | { op: 'remote_remove'; name: string }
  | { op: 'remote_rename'; old: string; new: string }
  | { op: 'continue'; what: InProgress }
  | { op: 'abort'; what: InProgress }
  | { op: 'apply_patch'; text: string; index: boolean }
  | { op: 'bisect_start'; bad: string; good: string }
  | { op: 'bisect_mark'; verdict: 'good' | 'bad' | 'skip' }
  | { op: 'bisect_reset' | 'submodule_update' | 'submodule_sync' | 'lfs_install' | 'lfs_pull' }
  | { op: 'lfs_track' | 'lfs_untrack'; pattern: string }
  | { op: 'lfs_lock' | 'lfs_unlock'; path: string }
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
/** `custom` 是用户手动改出来的内容，整段替换这个冲突块 */
export type Side = 'ours' | 'theirs' | 'both' | { custom: string }
/** 进行中的合并 / 变基 / 摘取 / 撤销在 Git 里记下的信息 */
export interface Operation {
  /** Git 准备好的提交信息 */
  message: string
  /** 正在并入或重放的提交 */
  id: string | null
  /** 操作停下来时冲突的全部文件，已解决的也在内 */
  conflicts: string[]
  /** 变基的 [第几步, 共几步] */
  step: [number, number] | null
}
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
/** diff 的显示参数；不传就是 git 的默认（3 行上下文、不忽略空白） */
export interface DiffOptions {
  context?: number
  ignore_ws?: boolean
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
/** `branches`：指向这个提交的其他本地分支（引用全名） */
export interface RebaseStep { id: string; subject: string; action: string; message: string; branches: string[] }
export interface RebasePlan { head: string; base: string; steps: RebaseStep[] }

/** 搜索结果或文件历史里的一条提交；`path` 是文件当时的路径，搜索结果里为空 */
export interface HistoryCommit { id: string; author: string; author_email: string; time: number; subject: string; path: string }
export type SearchKind = 'message' | 'author' | 'file' | 'content' | 'id'
/** 仓库本地配置里和提交有关的约定 */
export interface WorkflowSettings { template: string; check_command: string; check_before_commit: boolean }
/** 外部对比 / 合并工具：git 认识的工具名或一整条命令行；存在全局 git 配置里 */
export interface ExternalTools { diff: string; merge: string }
export interface Submodule { path: string; id: string; state: 'ok' | 'missing' | 'moved' | 'conflict' }
/** `version` 为 null 表示这台机器没装 git-lfs */
export interface Lfs { version: string | null; installed: boolean; patterns: string[] }
export interface Count { name: string; count: number }
/** 当前分支的提交统计；`times` 为作者时间戳，`files` 是改动次数最多的 100 个 */
export interface Stats { commits: number; times: number[]; authors: Count[]; files: Count[] }
