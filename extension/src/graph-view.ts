import { basename } from 'node:path'
import * as vscode from 'vscode'
import type { Detail } from '../../src/lib/api'
import { engine, onPrefsChange, onRepoChange, repos, revUri, store, stored } from './core'
import { setupWebview } from './webview'
import { fetch, pickScope, pull, push } from './sync'

/** 原生历史图仍需要 proposed API；使用稳定的 WebviewView 放在相同的 SCM 容器内。 */
export function registerGraphView(context: vscode.ExtensionContext) {
  const scopes = { ...context.workspaceState.get<Record<string, string>>('graphScopes', {}) }
  const scopeOf = (root: string, refs: { name: string }[]) => {
    const scope = scopes[root] ?? 'auto'
    if (scope === 'auto' || scope === 'all') return scope
    // 多选时是换行分隔的引用名；已删除的拿掉，一个不剩就回到 Auto
    return scope.split('\n').filter((name) => refs.some((r) => r.name === name)).join('\n') || 'auto'
  }
  const openFile = async (args: { tab: number; id: string; path: string; preview?: boolean }) => {
    const repo = repos.find((r) => r.tab === args?.tab)
    if (!repo || typeof args.id !== 'string' || typeof args.path !== 'string') throw new Error('Unsupported graph request')
    // 只打开这个提交实际改动的文件；父版本和重命名路径以引擎结果为准，不接受 Webview 指定的任意版本/路径。
    const detail = await engine.call<Detail>('detail', { tab: repo.tab, id: args.id })
    const file = detail.files.find((f) => f.path === args.path)
    if (!file) throw new Error('Unsupported graph request')
    const parent = detail.parents[0]
    const left = revUri(repo, file.old_path ?? file.path, parent ?? detail.id, !parent || file.status === 'A')
    const right = revUri(repo, file.path, detail.id, file.status === 'D')
    await vscode.commands.executeCommand('vscode.diff', left, right, `${basename(file.path)} (${detail.id.slice(0, 7)})`, { preview: args.preview !== false, preserveFocus: args.preview !== false })
  }
  context.subscriptions.push(vscode.commands.registerCommand('pushright.graph.openFile', openFile), vscode.window.registerWebviewViewProvider('pushright.graph', {
    resolveWebviewView(view) {
      const { webview } = view
      const changed = () => { if (view.visible) webview.postMessage({ refresh: true }) }
      const subscriptions = [
        setupWebview(context, webview, 'sidebar.js'),
        onRepoChange.event(changed),
        view.onDidChangeVisibility(changed),
        onPrefsChange.event(() => {
          webview.postMessage({ store: 'prefs', value: stored().prefs ?? '{}' })
          webview.postMessage({ store: 'diffColors', value: stored().diffColors ?? 'null' })
        }),
        webview.onDidReceiveMessage(async (m) => {
          if (m.store === 'prefs') return void await store(m.store, m.value)
          if (!m.cmd) return
          try {
            let value: unknown
            if (m.cmd === 'graph_repos') {
              value = repos.map(({ tab, root, name, refs }) => ({ tab, root, name, refs, scope: scopeOf(root, refs.refs) }))
            } else {
              const repo = repos.find((r) => r.tab === m.args?.tab)
              if (!repo) throw new Error('PR_TAB_CLOSED')
              if (m.cmd === 'graph_scope') {
                // 范围在主机的选择框里选，Webview 不能指定任意引用。
                const scope = await pickScope(repo, scopeOf(repo.root, repo.refs.refs))
                if (scope) {
                  scopes[repo.root] = scope
                  await context.workspaceState.update('graphScopes', scopes)
                }
              } else if (m.cmd === 'graph_sync' && ['fetch', 'pull', 'push'].includes(m.args.action)) {
                if (m.args.action === 'push') await push(repo, true)
                else await (m.args.action === 'pull' ? pull : fetch)(repo)
              } else if (m.cmd === 'detail' && typeof m.args.id === 'string') {
                value = await engine.call<Detail>('detail', { tab: repo.tab, id: m.args.id })
              } else if (m.cmd === 'open_commit_file') {
                await openFile(m.args)
              } else if (m.cmd === 'load_graph' && typeof m.args.full === 'boolean') {
                value = await engine.call(m.cmd, { tab: repo.tab, full: m.args.full, scope: scopeOf(repo.root, repo.refs.refs) })
              } else if (m.cmd === 'rows' && Number.isInteger(m.args.start) && m.args.start >= 0 && Number.isInteger(m.args.count) && m.args.count > 0 && m.args.count <= 128) {
                value = await engine.call(m.cmd, { tab: repo.tab, start: m.args.start, count: m.args.count })
              } else throw new Error('Unsupported graph request')
            }
            webview.postMessage({ id: m.id, ok: true, value })
          } catch (error) {
            webview.postMessage({ id: m.id, ok: false, value: String(error) })
          }
        }),
      ]
      view.onDidDispose(() => subscriptions.forEach((s) => s.dispose()))
    },
  }))
}
