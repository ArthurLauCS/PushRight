import { existsSync } from 'node:fs'
import { join } from 'node:path'
import * as vscode from 'vscode'
import { registerBlame } from './blame'
import { activeRepo, discoverForFile, discoverRepositories, engine, log, onPrefsChange, onRepoChange, openRepos, repos, v } from './core'
import { registerHistory } from './history'
import { registerGraphView } from './graph-view'
import { openPanel } from './panel'
import { offerTakeover, registerScm, repoArg } from './scm'
import { checkout, fastForward, fetch, inProgress, pull, push } from './sync'
import { registerLocalFiles } from './local-files'
import { registerDiffColors } from './diff-colors'
import { registerWorktrees } from './worktrees'

export async function activate(context: vscode.ExtensionContext) {
  const exe = join(context.extensionPath, 'bin', process.platform === 'win32' ? 'pushright-engine.exe' : 'pushright-engine')
  // 引擎按平台打包，目前只有 Windows x64
  if (!existsSync(exe)) return void vscode.window.showErrorMessage(v.unsupported)
  await openRepos(context, exe)
  await vscode.commands.executeCommand('setContext', 'pushright.hasRepo', repos.length > 0)

  const command = (id: string, fn: (...args: any[]) => unknown) => vscode.commands.registerCommand(`pushright.${id}`, fn)
  // 参数是状态栏传来的仓库或面板标题栏传来的 SourceControl；从命令面板执行时没有参数，取当前文件所在的仓库
  const onRepo = (fn: (repo: NonNullable<ReturnType<typeof activeRepo>>) => unknown) => (arg: unknown) => {
    const repo = repoArg(arg) ?? activeRepo()
    return repo && fn(repo)
  }
  context.subscriptions.push(
    log, onRepoChange, onPrefsChange,
    { dispose: () => engine.dispose() },
    command('openGraph', () => openPanel(context)),
    command('revealCommit', (id: string, root?: string) => openPanel(context, { id, root: root ?? activeRepo()?.root })),
    command('pull', onRepo(pull)),
    command('push', onRepo(push)),
    command('fetch', onRepo(fetch)),
    command('fastForward', onRepo(fastForward)),
    command('checkout', onRepo(checkout)),
    command('inProgress', onRepo(inProgress)),
    command('showLog', () => log.show()),
  )
  registerScm(context)
  registerGraphView(context)
  registerBlame(context)
  registerHistory(context)
  registerLocalFiles(context)
  registerDiffColors(context)
  registerWorktrees(context)
  context.subscriptions.push(
    vscode.workspace.onDidChangeWorkspaceFolders(() => discoverRepositories()),
    vscode.window.onDidChangeActiveTextEditor((editor) => editor && discoverForFile(editor.document.uri)),
  )
  // SCM 会忽略视图的初始展开声明；每个工作区只主动展开一次，之后保留用户的收起选择。
  if (repos.length && !context.workspaceState.get('graphOpened')) {
    await vscode.commands.executeCommand('pushright.graph.focus', { preserveFocus: true })
    await context.workspaceState.update('graphOpened', true)
  }
  if (repos.length) offerTakeover(context)
  // 供 tests/vscode-smoke-host.cjs 使用
  return { engine, repos }
}
