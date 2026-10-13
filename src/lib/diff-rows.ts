/** 并排 diff 的一格；`index` 是这一行在区块里的下标，按行暂存仍用它。 */
export interface Cell<L> { line: L; index: number }
export interface SplitRow<L> { left?: Cell<L>; right?: Cell<L> }

/** 把一个区块的行配成左右两栏：连续的删除和紧跟的新增逐行对齐，上下文两边各放一份。 */
export function splitRows<L extends { kind: string }>(lines: L[]): SplitRow<L>[] {
  const rows: SplitRow<L>[] = []
  let dels: Cell<L>[] = []
  let adds: Cell<L>[] = []
  /** 「文件末尾无换行」的标记跟在它前一行所在的那一栏 */
  let last: Cell<L>[] | null = null
  const flush = () => {
    for (let i = 0; i < Math.max(dels.length, adds.length); i++) rows.push({ left: dels[i], right: adds[i] })
    dels = []
    adds = []
  }
  lines.forEach((line, index) => {
    const cell = { line, index }
    if (line.kind === '-') {
      if (adds.length) flush()
      dels.push(cell)
      last = dels
    } else if (line.kind === '+') {
      adds.push(cell)
      last = adds
    } else if (line.kind === '\\' && last) last.push(cell)
    else {
      flush()
      last = null
      rows.push({ left: cell, right: cell })
    }
  })
  flush()
  return rows
}

/**
 * 「显示隐藏字符」的标记层：和原文逐格对齐的一串字符，空格处是点，制表符处是箭头，其余位置留白。
 * 界面用伪元素把它叠在原文上，不进文本、不增加节点（按空白段逐个包元素，几千行的 diff 会让界面卡死）。
 * 换行机会和原文保持一致：留白用不换行空格，点和制表符之后补零宽空格。
 */
export function whitespaceMarks(text: string, tab = 4): string {
  let out = ''
  let column = 0
  for (const ch of text) {
    if (ch === ' ') {
      out += '·\u200b'
      column++
    } else if (ch === '\t') {
      const width = tab - (column % tab)
      out += '→' + '\u00a0'.repeat(width - 1) + '\u200b'
      column += width
    } else {
      // 中日韩等宽字符占两格，用全角空格留白
      const wide = ch.codePointAt(0)! >= 0x2e80
      out += wide ? '\u3000' : '\u00a0'
      column += wide ? 2 : 1
    }
  }
  return out
}
