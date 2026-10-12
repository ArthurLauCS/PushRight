/** 变更文件按目录层级显示时的一行：文件夹或文件。 */
export interface TreeRow<T> {
  /** 文件是它的路径；文件夹是目录路径加 `/`，折叠状态按这个键记 */
  key: string
  depth: number
  name: string
  /** 文件行 */
  item?: T
  /** 文件夹行：其下所有文件，批量暂存 / 丢弃用 */
  items?: T[]
  open?: boolean
}

/** 树里文件行的左缩进，和同层文件夹的名字对齐；平铺显示（depth 为 -1）时不缩进 */
export const indent = (depth: number) => (depth < 0 ? undefined : `${28 + depth * 16}px`)

interface Dir<T> { dirs: Map<string, Dir<T>>; files: T[] }

/** 把路径列表摊成带深度的行：文件夹在前，只有一个子目录的目录并成一行，`closed` 里的文件夹不展开。 */
export function treeRows<T extends { path: string }>(items: T[], closed: ReadonlySet<string> = new Set()): TreeRow<T>[] {
  const root: Dir<T> = { dirs: new Map(), files: [] }
  // 未跟踪的嵌套仓库在状态里是以 / 结尾的一条
  const parts = (item: T) => item.path.replace(/\/$/, '').split('/')
  for (const item of items) {
    let dir = root
    for (const part of parts(item).slice(0, -1)) {
      let next = dir.dirs.get(part)
      if (!next) dir.dirs.set(part, (next = { dirs: new Map(), files: [] }))
      dir = next
    }
    dir.files.push(item)
  }
  const rows: TreeRow<T>[] = []
  const all = (dir: Dir<T>): T[] => [...[...dir.dirs.values()].flatMap(all), ...dir.files]
  const walk = (dir: Dir<T>, prefix: string, depth: number) => {
    for (let [name, child] of [...dir.dirs].sort(([a], [b]) => (a < b ? -1 : 1))) {
      while (!child.files.length && child.dirs.size === 1) {
        const [[next, only]] = child.dirs
        name += '/' + next
        child = only
      }
      const key = `${prefix}${name}/`
      const open = !closed.has(key)
      rows.push({ key, depth, name, items: all(child), open })
      if (open) walk(child, key, depth + 1)
    }
    for (const item of dir.files) rows.push({ key: item.path, depth, name: parts(item).at(-1)!, item })
  }
  walk(root, '', 0)
  return rows
}
