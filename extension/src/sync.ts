import * as vscode from 'vscode'
import { BRANCH_NAME } from '../../src/lib/branch-name'
import { splitUpstream } from '../../src/lib/push-target'
import { activeRepo, ago, engine, headBranch, help, run, short, t, v, type Repo } from './core'

const upstreamOf = (repo: Repo) => repo.refs.refs.find((r) => r.name === repo.refs.head)?.upstream

/** 当前分支的上游与它不同名时返回 `<远程>/<分支>`。 */
export function mismatch(repo: Repo): string | null {
  const up = splitUpstream(upstreamOf(repo), repo.remotes)
  return up && up.branch !== headBranch(repo) ? `${up.remote}/${up.branch}` : null
}

/** 每次都弹出选择框，「Rebase instead of merge」每次都默认勾选，不记忆上次的选择。 */
export async function pull(repo = activeRepo()) {
  if (!repo) return
  const upstream = upstreamOf(repo)
  if (!upstream) return void vscode.window.showWarningMessage(t.noUpstream)
  const autostash = { label: t.pullAutostash, picked: false }
  const picked = await vscode.window.showQuickPick([{ label: t.pullRebase, detail: help.pull.what, picked: true }, autostash], {
    canPickMany: true,
    ignoreFocusOut: true,
    title: `${repo.name} · ${t.pull} · ${short(upstream)}`,
    placeHolder: t.pullFrom(short(upstream)),
  })
  if (!picked) return
  if (!(await run(repo, { op: 'pull', rebase: picked.some((p) => p !== autostash), autostash: picked.includes(autostash) })) && repo.refs.in_progress) inProgress(repo)
}

/** 把一个落后于上游的本地分支快进过去；分叉的分支不列出来，引擎也会拒绝。 */
export async function fastForward(repo = activeRepo()) {
  if (!repo) return
  const tracks = await engine.call<{ name: string; ahead: number; behind: number; gone: boolean }[]>('tracking', { tab: repo.tab })
  const items = tracks.filter((x) => x.behind && !x.ahead && !x.gone).map((x) => ({ label: short(x.name), description: `${x.name === repo.refs.head ? '$(check) ' : ''}${t.toPull(x.behind)}` }))
  if (!items.length) return void vscode.window.showInformationMessage(v.fastForwardNone)
  const pick = await vscode.window.showQuickPick(items, { title: `${repo.name} · ${v.fastForwardPick}`, placeHolder: help.fast_forward.what })
  if (pick) await run(repo, { op: 'fast_forward', branch: pick.label })
}

export async function push(repo = activeRepo(), sameNameOnly = false) {
  if (!repo) return
  const branch = headBranch(repo)
  if (!branch) return void vscode.window.showWarningMessage(t.noBranch)
  if (!repo.remotes.length) return void vscode.window.showWarningMessage(t.noRemote)
  const up = splitUpstream(upstreamOf(repo), repo.remotes)
  const title = `${repo.name} · ${t.pushTitle(branch)}`

  let target: { remote: string; remote_branch: string; set_upstream: boolean }
  if (up?.branch === branch) {
    target = { remote: up.remote, remote_branch: branch, set_upstream: false }
  } else if (up && sameNameOnly) {
    target = { remote: up.remote, remote_branch: branch, set_upstream: true }
  } else if (up) {
    // 上游与本地分支不同名：先推荐同名分支，仍需明确选一个目标
    const pick = await vscode.window.showQuickPick(
      [
        { label: t.pushToSameName(`${up.remote}/${branch}`), same: true },
        { label: t.pushToUpstream(`${up.remote}/${up.branch}`), same: false },
      ],
      { title, placeHolder: t.pushMismatch(branch, `${up.remote}/${up.branch}`), ignoreFocusOut: true },
    )
    if (!pick) return
    target = { remote: up.remote, remote_branch: pick.same ? branch : up.branch, set_upstream: pick.same }
  } else {
    const remote = repo.remotes.length === 1 ? repo.remotes[0] : await vscode.window.showQuickPick(repo.remotes, { title, placeHolder: t.remote })
    if (!remote) return
    target = { remote, remote_branch: branch, set_upstream: true }
  }

  const dest = `${target.remote}/${target.remote_branch}`
  const detail = `${up ? t.pushTo(dest) : t.pushNew(dest)}\n\n${help.push.what}`
  const choice = await vscode.window.showInformationMessage(title, { modal: true, detail }, t.push, t.forcePush)
  if (!choice) return
  const force = choice === t.forcePush
  if (force && !(await vscode.window.showWarningMessage(t.forcePush, { modal: true, detail: v.forcePushWarning(dest) }, t.forcePush))) return
  await run(repo, { op: 'push', branch, force, ...target })
}

export async function fetch(repo = activeRepo()) {
  if (repo) await run(repo, { op: 'fetch' })
}

type Tip = { name: string; short_id: string; author: string; time: number; subject: string }

/** 顶部选择框里的分支项：本地在前、顺序固定，当前分支带对钩，每项下面一行是该分支的最新提交。 */
async function branchItems(repo: Repo, withHead: boolean) {
  const tips = new Map((await engine.call<Tip[]>('branch_tips', { tab: repo.tab })).map((tip) => [tip.name, tip]))
  const candidates = repo.refs.refs.filter((r) => (withHead || r.name !== repo.refs.head) && /^refs\/(heads|remotes)\//.test(r.name) && !r.name.endsWith('/HEAD'))
  const group = (prefix: string) => candidates.filter((r) => r.name.startsWith(prefix))
  return [...group('refs/heads/'), ...group('refs/remotes/')].map((r) => {
    const tip = tips.get(r.name)
    return {
      label: short(r.name),
      description: `${r.name === repo.refs.head ? '$(check) ' : ''}${tip ? ago(tip.time) : ''}`,
      detail: tip && `${tip.author} · ${tip.short_id} · ${tip.subject}`,
      ref: r.name,
    }
  })
}

export async function checkout(repo = activeRepo()) {
  if (!repo) return
  const create = { label: `$(plus) ${t.newBranch}…`, ref: '' }
  const pick = await vscode.window.showQuickPick([create, ...await branchItems(repo, false)], { title: t.opNames.checkout, placeHolder: help.checkout.short, matchOnDetail: true })
  if (!pick) return
  if (pick === create) return createBranch(repo)
  await run(repo, pick.ref.startsWith('refs/remotes/') ? { op: 'track', remote_branch: pick.label } : { op: 'checkout', target: pick.label })
}

/** 先按命名规则输入分支名，再选从哪里签出；默认是当前所在的提交。 */
async function createBranch(repo: Repo) {
  const name = await vscode.window.showInputBox({
    title: t.newBranch,
    prompt: t.branchRule,
    placeHolder: 'feat_S_userLogin',
    ignoreFocusOut: true,
    validateInput: (value) => (BRANCH_NAME.test(value) ? undefined : t.branchRule),
  })
  if (!name) return
  const here = { label: `${t.currentBranch} (${headBranch(repo) ?? t.detached})`, ref: 'HEAD' }
  const from = await vscode.window.showQuickPick([here, ...await branchItems(repo, false)], { title: `${name} · ${t.branchFrom}`, matchOnDetail: true, ignoreFocusOut: true })
  if (from) await run(repo, { op: 'create_branch', name, start: from === here ? 'HEAD' : from.label, checkout: true })
}

/** 提交图显示哪个分支：与切换分支用同一个选择框，但只筛选，不检出。 */
export async function pickScope(repo: Repo, current = '') {
  const fixed = [{ label: v.graphAuto, detail: v.graphAutoHint, ref: 'auto' }, { label: v.graphAll, ref: 'all' }]
  const branches = await branchItems(repo, true)
  // 多选入口放在最后，前面各项的位置和单选时完全一样
  const many = { label: `$(checklist) ${v.graphPickMany}`, ref: '' }
  const title = `${repo.name} · ${v.graphScope}`
  const versus = { label: `$(git-compare) ${t.compareBranch.pick}`, detail: t.compareBranch.hint, ref: '' }
  const pick = await vscode.window.showQuickPick([...fixed, ...branches, versus, many], { title, matchOnDetail: true })
  if (pick === versus) {
    // 先选要看的分支，再选基准；结果是「分支」加一行「^基准」，引擎只留基准上没有的提交
    const branch = await vscode.window.showQuickPick(branches, { title: `${title} · ${t.compareBranch.pickBranch}`, matchOnDetail: true })
    const base = branch && await vscode.window.showQuickPick(branches.filter((b) => b.ref !== branch.ref), { title: `${branch.label} · ${t.compareBranch.base}`, matchOnDetail: true })
    return base ? `${branch.ref}\n^${base.ref}` : undefined
  }
  if (pick !== many) return pick?.ref
  const chosen = current.split('\n')
  const picked = await vscode.window.showQuickPick(branches.map((b) => ({ ...b, picked: chosen.includes(b.ref) })), { title, matchOnDetail: true, canPickMany: true })
  // 多个引用以换行分隔交给引擎；一个都没勾等于取消
  return picked?.map((b) => b.ref).join('\n') || undefined
}

/** 这次操作冲突过的全部文件：Git 记下的清单加上现在还没解决的。 */
export function conflictFiles(repo: Repo) {
  const open = new Set((repo.status ?? []).filter((e) => e.conflicted).map((e) => e.path))
  return [...new Set([...(repo.operation?.conflicts ?? []), ...open])].map((path) => ({ path, resolved: !open.has(path) }))
}

/** 进行中的操作正在处理哪个提交、变基到第几步、冲突解决了几个；Git 没记下这些时为空。 */
export function operationSummary(repo: Repo): string {
  const operation = repo.operation
  if (!operation?.id) return ''
  const files = conflictFiles(repo)
  return [
    t.operationCommit(operation.id.slice(0, 8), operation.message.split('\n')[0]),
    ...(operation.step ? [t.operationStep(...operation.step)] : []),
    ...(files.length ? [t.conflictProgress(files.filter((f) => f.resolved).length, files.length)] : []),
  ].join(' · ')
}

/** 合并 / 变基停在冲突上时：继续或中止。 */
export async function inProgress(repo = activeRepo()) {
  const what = repo?.refs.in_progress
  if (!repo || !what) return
  const files = conflictFiles(repo).map((f) => `${f.resolved ? '✓' : '!'} ${f.path}`)
  const hint = [operationSummary(repo), files.join('\n'), t.inProgressHint, what === 'rebase' ? t.rebaseConflictHint : ''].filter(Boolean).join('\n\n')
  const choices = what === 'revert' && repo.refs.revert_disabled ? [t.abortOperation(what)] : [t.continueOperation(what), t.abortOperation(what)]
  const choice = await vscode.window.showWarningMessage(t.inProgress[what], { modal: true, detail: hint }, ...choices)
  if (choice) await run(repo, { op: choice === t.continueOperation(what) ? 'continue' : 'abort', what })
}

/** 状态栏里的分支、拉取、推送入口；上游不同名和操作进行中时多出警告项。 */
export function statusCommands(repo: Repo): vscode.Command[] {
  const [ahead, behind] = repo.refs.ahead_behind ?? [0, 0]
  const wrong = mismatch(repo)
  const args = [repo]
  const commands: vscode.Command[] = [
    { command: 'pushright.checkout', arguments: args, title: `$(git-branch) ${headBranch(repo) ?? t.detached}`, tooltip: t.opNames.checkout },
    { command: 'pushright.pull', arguments: args, title: `$(arrow-down) ${behind}`, tooltip: `${t.pull} · ${t.toPull(behind)}` },
    { command: 'pushright.push', arguments: args, title: `$(arrow-up) ${ahead}`, tooltip: `${t.push} · ${t.toPush(ahead)}` },
  ]
  if (wrong) commands.push({ command: 'pushright.push', arguments: args, title: `$(warning) ${v.upstreamMismatch}`, tooltip: t.pushMismatch(headBranch(repo)!, wrong) })
  if (repo.refs.in_progress) {
    commands.push({ command: 'pushright.inProgress', arguments: args, title: `$(debug-pause) ${t.inProgress[repo.refs.in_progress]}`, tooltip: [operationSummary(repo), t.inProgressHint].filter(Boolean).join('\n') })
  }
  return commands
}
