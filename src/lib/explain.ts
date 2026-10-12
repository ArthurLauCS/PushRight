// 每个操作的白话说明和操作前后的示意图。菜单提示、确认对话框、「操作说明」页都从这里取。
// 新增操作入口时在这里补一条。

export interface Node {
  id: string
  x: number
  y: number
  /** 0 蓝（当前这条线）1 绿（另一条线）2 黄（远程） */
  color: number
  /** new：这次操作新产生的；ghost：操作后从分支上消失的 */
  kind?: 'new' | 'ghost'
}
export interface Label {
  at: string
  text: string
  color: number
  /** 你当前所在的分支，画成实心 */
  here: boolean
}
export interface Graph {
  nodes: Node[]
  edges: [string, string][]
  labels: Label[]
}
export interface Explain {
  name: string
  /** 一行以内，用在菜单和按钮提示里 */
  short: string
  what: string
  result: string[]
  /** 做错了怎么办 */
  undo?: string
  before?: Graph
  after?: Graph
}

/**
 * 示意图的紧凑写法：
 * 节点 `id:x,y[:颜色[:new|ghost]]`，连线 `父>子`，标签 `节点=文字[:颜色][*]`（* 表示当前分支）
 */
function g(nodes: string, edges: string, labels: string): Graph {
  const words = (s: string) => s.split(' ').filter(Boolean)
  return {
    nodes: words(nodes).map((n) => {
      const [id, pos, color, kind] = n.split(':')
      const [x, y] = pos.split(',').map(Number)
      return { id, x, y, color: +(color || 0), kind: kind as Node['kind'] }
    }),
    edges: words(edges).map((e) => e.split('>') as [string, string]),
    labels: words(labels).map((l) => {
      const here = l.endsWith('*')
      const [at, rest] = l.replace('*', '').split('=')
      const [text, color] = rest.split(':')
      return { at, text, color: +(color || 0), here }
    }),
  }
}

// 两条分支的公共起点：main 上 A-B-C，feat 从 B 分出 D-E
const FORK = 'A:0,0 B:1,0 C:2,0 D:2,1:1 E:3,1:1'
const FORK_EDGES = 'A>B B>C B>D D>E'
const LINE = 'A:0,0 B:1,0 C:2,0'

export const diagrams = {
  stage: {
  },
  commit: {
    before: g(LINE, 'A>B B>C', 'C=main*'),
    after: g(LINE + ' D:3,0::new', 'A>B B>C C>D', 'D=main*'),
  },
  checkout: {
    before: g(FORK, FORK_EDGES, 'C=main* E=feat:1'),
    after: g(FORK, FORK_EDGES, 'C=main E=feat:1*'),
  },
  checkout_commit: {
  },
  track: {
  },
  create_branch: {
    before: g(LINE, 'A>B B>C', 'C=main*'),
    after: g(LINE, 'A>B B>C', 'C=main C=new-branch:1*'),
  },
  delete_branch: {
  },
  merge: {
    before: g(FORK, FORK_EDGES, 'C=main* E=feat:1'),
    after: g(FORK + ' M:4,0::new', FORK_EDGES + ' C>M E>M', 'M=main* E=feat:1'),
  },
  rebase: {
    before: g(FORK, FORK_EDGES, 'C=main E=feat:1*'),
    after: g("A:0,0 B:1,0 C:2,0 D:2,1:1:ghost E:3,1:1:ghost D':3,0:1:new E':4,0:1:new", "A>B B>C B>D D>E C>D' D'>E'", "C=main E'=feat:1*"),
  },
  cherry_pick: {
    before: g(FORK, FORK_EDGES, 'C=main* E=feat:1'),
    after: g(FORK + " E':3,0:1:new", FORK_EDGES + " C>E'", "E'=main* E=feat:1"),
  },
  revert: {
    before: g(LINE, 'A>B B>C', 'C=main*'),
    after: g(LINE + ' -C:3,0::new', 'A>B B>C C>-C', '-C=main*'),
  },
  reset: {
    before: g(LINE, 'A>B B>C', 'C=main*'),
    after: g('A:0,0 B:1,0 C:2,0::ghost', 'A>B B>C', 'B=main*'),
  },
  fetch: {
    before: g('A:0,0 B:1,0', 'A>B', 'B=main* B=origin/main:2'),
    after: g('A:0,0 B:1,0 C:2,0:2:new', 'A>B B>C', 'B=main* C=origin/main:2'),
  },
  pull: {
    before: g('A:0,0 B:1,0', 'A>B', 'B=main* B=origin/main:2'),
    after: g('A:0,0 B:1,0 C:2,0::new', 'A>B B>C', 'C=main* C=origin/main:2'),
  },
  fast_forward: {
    // B 和 C 隔开两格：相邻节点上的两个标签会叠在一起
    before: g('A:0,0 B:1,0 C:3,0:2', 'A>B B>C', 'B=main* C=origin/main:2'),
    after: g('A:0,0 B:1,0 C:3,0', 'A>B B>C', 'C=main* C=origin/main:2'),
  },
  push: {
    before: g(LINE, 'A>B B>C', 'C=main* B=origin/main:2'),
    after: g(LINE, 'A>B B>C', 'C=main* C=origin/main:2'),
  },
  stash: {
  },
  stash_drop: {
  },
  worktree: {
  },
  switch_dirty: {
  },
  discard: {
  },
  undo: {
  },
  conflict: {
  },
  create_tag: {
  },
} satisfies Record<string, Pick<Explain, 'before' | 'after'>>

export type ExplainKey = keyof typeof diagrams
