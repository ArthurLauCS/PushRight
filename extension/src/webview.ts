import { randomUUID } from 'node:crypto'
import * as vscode from 'vscode'
import { locale, stored } from './core'

/** 侧栏提交图的资源、偏好、CSP 和主题同步。 */
export function setupWebview(context: vscode.ExtensionContext, webview: vscode.Webview, entry: string) {
  const media = vscode.Uri.joinPath(context.extensionUri, 'media')
  webview.options = { enableScripts: true, localResourceRoots: [media] }
  const nonce = randomUUID().replaceAll('-', '')
  const asset = (file: string) => webview.asWebviewUri(vscode.Uri.joinPath(media, file))
  const theme = (kind: vscode.ColorThemeKind) => kind === vscode.ColorThemeKind.Dark || kind === vscode.ColorThemeKind.HighContrast ? 'dark' : 'light'
  const seed = JSON.stringify({ ...stored(), theme: theme(vscode.window.activeColorTheme.kind), locale }).replace(/</g, '\\u003c')
  webview.html = `<!doctype html>
<html>
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${webview.cspSource} data:; font-src ${webview.cspSource}; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}' ${webview.cspSource};" />
    <link rel="stylesheet" href="${asset('style.css')}" />
  </head>
  <body style="padding: 0">
    <div id="app"></div>
    <script nonce="${nonce}">
      localStorage.clear()
      for (const [key, value] of Object.entries(${seed})) localStorage.setItem(key, value)
    </script>
    <script type="module" nonce="${nonce}" src="${asset(entry)}"></script>
  </body>
</html>`
  return vscode.window.onDidChangeActiveColorTheme(({ kind }) => {
    webview.postMessage({ store: 'theme', value: theme(kind) })
  })
}
