import { basename } from 'node:path'
import * as vscode from 'vscode'
import type { Worktree } from '../../src/lib/api'
import { BRANCH_NAME } from '../../src/lib/branch-name'
import { activeRepo, attempt, engine, help, onRepoChange, repos, run, short, t, v, type Repo } from './core'

/** 没有 `worktree` 的是仓库分组行：窗口里有多个仓库时才出现 */
type Item = { repo: Repo; worktree?: Worktree }

/** 「PushRight」页面里的工作树视图：列出、在新窗口打开、新建、删除。 */
export function registerWorktrees(context: vscode.ExtensionContext) {
  const changed = new vscode.EventEmitter<void>()
  const list = async (repo: Repo): Promise<Item[]> =>
    (await attempt(engine.call<Worktree[]>('worktrees', { tab: repo.tab })) ?? []).map((worktree) => ({ repo, worktree }))
  // 原窗口保留：工作树总是开在新窗口里
  const open = (path: string) => vscode.commands.executeCommand('vscode.openFolder', vscode.Uri.file(path), { forceNewWindow: true })

  const provider: vscode.TreeDataProvider<Item> = {
    onDidChangeTreeData: changed.event,
    getTreeItem({ repo, worktree: w }) {
      if (!w) {
        const tree = new vscode.TreeItem(repo.name, vscode.TreeItemCollapsibleState.Expanded)
        Object.assign(tree, { id: repo.root, tooltip: repo.root, iconPath: new vscode.ThemeIcon('repo'), contextValue: 'worktreeRepo' })
        return tree
      }
      const tree = new vscode.TreeItem(basename(w.path))
      const [ahead, behind] = w.ahead_behind ?? [0, 0]
      tree.id = `${repo.root}:${w.path}`
      tree.description = [w.branch ? short(w.branch) : `${t.detached} ${w.head.slice(0, 7)}`, w.current && t.wtCurrent, w.changes && t.wtDirty(w.changes), ahead && t.toPush(ahead), behind && t.toPull(behind)].filter(Boolean).join(' · ')
      tree.tooltip = `${w.path}\n${w.subject}`
      tree.iconPath = new vscode.ThemeIcon(w.current ? 'check' : 'folder')
      // 当前窗口所在的工作树不能打开自己，也不能删除自己
      if (!w.current) {
        tree.contextValue = 'worktree'
        tree.command = { command: 'pushright.worktree.open', title: '', arguments: [{ repo, worktree: w }] }
      }
      return tree
    },
    getChildren: (item) => (!item ? (repos.length === 1 ? list(repos[0]) : repos.map((repo) => ({ repo }))) : item.worktree ? [] : list(item.repo)),
  }
  const view = vscode.window.createTreeView('pushright.worktrees', { treeDataProvider: provider })

  /** 先选已有分支或新建分支，再定文件夹位置；建好后在新窗口打开 */
  async function add(item?: Item) {
    const repo = item?.repo ?? (repos.length === 1 ? repos[0] : activeRepo())
    if (!repo) return
    // 同一个分支同时只能在一个工作树里打开，已经打开的不列出来
    const taken = new Set((await list(repo)).map((i) => i.worktree!.branch))
    const create = { label: `$(plus) ${v.worktreeNewBranch}` }
    const branches = repo.refs.refs.filter((r) => r.name.startsWith('refs/heads/') && !taken.has(r.name)).map((r) => ({ label: short(r.name) }))
    const pick = await vscode.window.showQuickPick([create, ...branches], { title: `${repo.name} · ${t.newWorktree}`, placeHolder: help.worktree.short })
    if (!pick) return
    const branch = pick !== create ? pick.label : await vscode.window.showInputBox({
      title: v.worktreeNewBranch,
      prompt: t.branchRule,
      placeHolder: 'feat_S_userLogin',
      ignoreFocusOut: true,
      validateInput: (value) => (BRANCH_NAME.test(value) ? undefined : t.branchRule),
    })
    if (!branch) return
    const path = await vscode.window.showInputBox({ title: `${branch} · ${t.worktreePath}`, value: `${repo.root}-${branch.replace(/[\\/]/g, '-')}`, prompt: help.worktree.what, ignoreFocusOut: true })
    if (!path) return
    if (await run(repo, { op: 'worktree_add', path, start: pick === create ? 'HEAD' : branch, new_branch: pick === create ? branch : null })) await open(path)
    changed.fire()
  }

  async function remove(item?: Item) {
    const w = item?.worktree
    if (!w) return
    const detail = [w.path, w.changes ? t.wtRemoveWarning(w.changes) : '', help.worktree.undo, v.worktreeRemoveOpen].filter(Boolean).join('\n\n')
    if (!(await vscode.window.showWarningMessage(t.wtRemoveTitle(basename(w.path)), { modal: true, detail }, t.wtRemove))) return
    await run(item.repo, { op: 'worktree_remove', path: w.path, force: w.changes > 0 })
    changed.fire()
  }

  const refresh = () => { if (view.visible) changed.fire() }
  context.subscriptions.push(
    view, changed, onRepoChange.event(refresh), view.onDidChangeVisibility(refresh),
    vscode.commands.registerCommand('pushright.worktree.open', (item?: Item) => item?.worktree && open(item.worktree.path)),
    vscode.commands.registerCommand('pushright.worktree.add', add),
    vscode.commands.registerCommand('pushright.worktree.remove', remove),
  )
}
