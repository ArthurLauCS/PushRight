import * as vscode from 'vscode'
import { DIFF_PRESETS, validDiffColors, readDiffColors, type DiffColors } from '../../src/lib/diff-colors'
import { store, stored, t } from './core'

type Customizations = Record<string, string | Record<string, string>>
interface Backup { original: Record<string, Record<string, string | null>>; applied: Record<string, string> }
const BACKUP = 'diffColorBackup'

function nativeColors(colors: DiffColors) {
  const result: Record<string, string> = {}
  for (const [side, color] of [['inserted', colors.added], ['removed', colors.deleted]]) {
    result[`diffEditor.${side}LineBackground`] = color + '38'
    result[`diffEditor.${side}TextBackground`] = color + '55'
    result[`diffEditorGutter.${side}LineBackground`] = color + '66'
    result[`diffEditorOverview.${side}Foreground`] = color
    result[`gitDecoration.${side === 'inserted' ? 'added' : 'deleted'}ResourceForeground`] = color
  }
  return result
}

// Restore only values still owned by us; preserve later manual edits and unrelated theme settings.
export function updateDiffColors(current: Customizations, previous: Backup | undefined, colors: DiffColors | null) {
  const next = { ...current }
  const applied = colors ? nativeColors(colors) : previous?.applied ?? {}
  const original: Backup['original'] = {}
  for (const scope of ['', ...Object.keys(current).filter((key) => key.startsWith('[') && typeof current[key] === 'object')]) {
    const values = scope ? { ...current[scope] as Record<string, string> } : next
    original[scope] = {}
    for (const key of Object.keys(applied)) {
      const owned = previous && values[key] === previous.applied[key] && Object.hasOwn(previous.original[scope] ?? {}, key)
      original[scope][key] = owned ? previous.original[scope][key] : values[key] as string ?? null
      if (colors) values[key] = applied[key]
      else if (owned) {
        const value = previous.original[scope][key]
        if (value === null) delete values[key]
        else values[key] = value
      }
    }
    if (scope) next[scope] = values as Record<string, string>
  }
  return { colors: next, backup: colors ? { original, applied } : undefined }
}

export async function applyDiffColors(context: vscode.ExtensionContext, colors: unknown) {
  if (colors !== null && !validDiffColors(colors)) throw new Error(t.diffColors.invalid)
  const config = vscode.workspace.getConfiguration('workbench')
  const current = config.inspect<Customizations>('colorCustomizations')?.globalValue ?? {}
  const previous = context.globalState.get<Backup>(BACKUP)
  const next = updateDiffColors(current, previous, colors)
  if (colors || previous) {
    if (colors) await context.globalState.update(BACKUP, next.backup)
    try { await config.update('colorCustomizations', Object.keys(next.colors).length ? next.colors : undefined, vscode.ConfigurationTarget.Global) }
    catch (error) { await context.globalState.update(BACKUP, previous); throw error }
    if (!colors) await context.globalState.update(BACKUP, undefined)
  }
  const value = JSON.stringify(colors)
  await store('diffColors', value)
}

export function registerDiffColors(context: vscode.ExtensionContext) {
  context.subscriptions.push(vscode.commands.registerCommand('pushright.diffColors', async () => {
    const picked = await vscode.window.showQuickPick([
      ...Object.entries(DIFF_PRESETS).map(([key, colors]) => ({ label: t.diffColors[key as keyof typeof DIFF_PRESETS], colors, key })),
      { label: t.diffColors.custom, key: 'custom', colors: null },
      { label: t.diffColors.default, key: 'default', colors: null },
    ], { title: t.diffColors.title, placeHolder: t.diffColors.editorHint })
    if (!picked) return
    let colors = picked.colors
    if (picked.key === 'custom') {
      const current = readDiffColors(stored().diffColors ?? null) ?? DIFF_PRESETS.blueOrange
      const added = await vscode.window.showInputBox({ title: t.diffColors.added, value: current.added, validateInput: (added) => validDiffColors({ ...current, added }) ? null : t.diffColors.invalid })
      if (added === undefined) return
      const deleted = await vscode.window.showInputBox({ title: t.diffColors.deleted, value: current.deleted, validateInput: (deleted) => validDiffColors({ added, deleted }) ? null : t.diffColors.invalid })
      if (deleted === undefined) return
      colors = { added, deleted }
    }
    try { await applyDiffColors(context, colors) }
    catch (error) { await vscode.window.showErrorMessage(String(error)) }
  }))
}
