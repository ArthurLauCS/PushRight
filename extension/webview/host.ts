// 侧栏提交图和扩展主机之间的通道：引擎调用经 postMessage 交给扩展主机，由它转给 sidecar。

declare function acquireVsCodeApi(): { postMessage(message: unknown): void }

const vscode = acquireVsCodeApi()
const pending = new Map<number, [(value: any) => void, (error: unknown) => void]>()
let next = 0

export const invoke = <T>(cmd: string, args: object = {}) =>
  new Promise<T>((resolve, reject) => {
    pending.set(++next, [resolve, reject])
    vscode.postMessage({ id: next, cmd, args })
  })

// 界面偏好写 localStorage 的同时交给扩展主机保存：Webview 自己的 localStorage 不保证留存
const store = (key: string, value: string | null) => vscode.postMessage({ store: key, value })
const { setItem, removeItem } = Storage.prototype
Storage.prototype.setItem = function (key, value) {
  setItem.call(this, key, value)
  store(key, value)
}
Storage.prototype.removeItem = function (key) {
  removeItem.call(this, key)
  store(key, null)
}

addEventListener('message', ({ data }) => {
  if ('id' in data) {
    const waiter = pending.get(data.id)
    pending.delete(data.id)
    waiter?.[data.ok ? 0 : 1](data.value)
  } else if ('store' in data) {
    // 主机那边改了偏好（在编辑器里设置作者样式）
    setItem.call(localStorage, data.store, data.value)
    dispatchEvent(new StorageEvent('storage', { key: data.store, newValue: data.value }))
  } else if ('refresh' in data) {
    // 界面在窗口重新获得焦点时刷新，借用同一个入口
    dispatchEvent(new Event('focus'))
  }
})
