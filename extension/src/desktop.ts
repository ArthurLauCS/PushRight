import { execFileSync, spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import * as vscode from 'vscode'
import { activeRepo, v } from './core'

/** 安装包（Inno Setup）在卸载项里记下的安装目录；没装时为 undefined */
function installed(): string | undefined {
  try {
    const out = execFileSync('reg', ['query', 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\PushRight_is1', '/v', 'InstallLocation'], { encoding: 'utf8', windowsHide: true, stdio: ['ignore', 'pipe', 'ignore'] })
    const dir = out.match(/InstallLocation\s+REG_SZ\s+(.+)/)?.[1].trim()
    return dir && join(dir, 'pushright.exe')
  } catch {
    return undefined
  }
}

/** 桌面版在哪：设置里指定的优先，其次是安装位置，最后是安装包的默认位置 */
function desktopExe(): string | undefined {
  const configured = vscode.workspace.getConfiguration('pushright').get<string>('desktopPath')?.trim()
  const fallback = process.env.LOCALAPPDATA && join(process.env.LOCALAPPDATA, 'Programs', 'PushRight', 'pushright.exe')
  return [configured, installed(), fallback].find((p): p is string => !!p && existsSync(p))
}

/** 打开提交图：交给原生桌面版。它已经开着时会在原来的窗口里切到这个仓库，不会再开一个 */
export async function openDesktop(root?: string, reveal?: string) {
  const exe = desktopExe()
  if (!exe) {
    const download = await vscode.window.showErrorMessage(v.desktopMissing, v.desktopDownload)
    if (download) await vscode.env.openExternal(vscode.Uri.parse('https://github.com/ArthurLauCS/PushRight/releases/latest'))
    return
  }
  const repo = root ?? activeRepo()?.root
  const args = [...(repo ? [repo] : []), ...(reveal ? ['--reveal', reveal] : [])]
  spawn(exe, args, { detached: true, stdio: 'ignore' }).unref()
}
