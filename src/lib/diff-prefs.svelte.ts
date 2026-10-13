// diff 工具栏和文件列表的显示偏好，存 localStorage 的 diffPrefs，所有页签共用一份。
// 不并进 prefs：扩展主机改作者样式时会整体覆写那个键。

import type { DiffOptions } from './api'

/** 上下文行数的档位，减少 / 增加按钮在两头禁用 */
export const CONTEXT_STEPS = [0, 1, 3, 6, 12, 25, 50]

const DEFAULTS = { ignoreWs: false, hidden: false, wrap: false, sideBySide: false, context: 3, tree: true, splitRatio: 0.5 }
type Saved = typeof DEFAULTS

let saved: Partial<Saved> = {}
try {
  saved = JSON.parse(localStorage.getItem('diffPrefs') ?? '{}') ?? {}
} catch {
  // 损坏的本地偏好不应阻止查看改动。
}

export const diffPrefs = $state({
  ...DEFAULTS,
  ...saved,
  context: CONTEXT_STEPS.includes(saved.context!) ? saved.context! : DEFAULTS.context,
  /** 并排时左栏占的比例，拖中间的分隔条调整 */
  splitRatio: Math.min(0.8, Math.max(0.2, Number(saved.splitRatio) || DEFAULTS.splitRatio)),
  /** 显示整个文件：只在本次运行内有效，免得每打开一个文件都取全文 */
  whole: false,
})

export function setDiffPref<K extends keyof Saved | 'whole'>(key: K, value: (typeof diffPrefs)[K]) {
  diffPrefs[key] = value
  const { whole: _, ...keep } = diffPrefs
  localStorage.setItem('diffPrefs', JSON.stringify(keep))
}

/** 传给引擎的参数。在 $effect 里调用，偏好一变就会重新取 diff。 */
export const diffOptions = (): Required<DiffOptions> => ({ context: diffPrefs.whole ? 1_000_000 : diffPrefs.context, ignore_ws: diffPrefs.ignoreWs })

/** 过滤过的视图和真实补丁对不上，不能按行 / 按区块暂存或丢弃 */
export const partialBlocked = () => diffPrefs.ignoreWs || (!diffPrefs.whole && diffPrefs.context === 0)
