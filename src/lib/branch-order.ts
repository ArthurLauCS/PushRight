/** 侧栏分支的排列：当前分支永远在第一行，其次是固定的分支（按固定的先后），其余保持原来的顺序。 */
export function orderBranches<T extends { name: string }>(items: T[], head: string | null, pinned: readonly string[]): T[] {
  const rank = (name: string) => (name === head ? -1 : pinned.includes(name) ? pinned.indexOf(name) : pinned.length)
  // sort 是稳定的，同一档里的顺序不变
  return [...items].sort((a, b) => rank(a.name) - rank(b.name))
}
