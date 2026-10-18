import * as vscode from 'vscode'
import { store, stored, t } from './core'

/** 历史、Blame、侧栏提交图的显示时区：跟随系统、UTC、本机时区，其余按名字排。存在编辑器里，所有仓库共用 */
export async function pickTimeZone() {
  const local = Intl.DateTimeFormat().resolvedOptions().timeZone
  const zones = [...new Set(['UTC', local, ...Intl.supportedValuesOf('timeZone')])]
  const current = stored().timeZone ?? ''
  const items = [{ label: t.systemTimeZone, value: '' }, ...zones.map((z) => ({ label: z.replaceAll('_', ' '), value: z }))]
    .map((item) => ({ ...item, picked: item.value === current, description: item.value === current ? '✓' : undefined }))
  const pick = await vscode.window.showQuickPick(items, { title: t.timeZone, placeHolder: t.timeZoneHint })
  if (pick) await store('timeZone', pick.value)
}
