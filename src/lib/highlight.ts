// diff 里的语法高亮：按文件扩展名选语言，区块的旧侧和新侧各自作为一段完整代码高亮，再拆回逐行。
// 整段高亮才能认出跨行的字符串和注释；区块以外的上文看不到，从区块中间开始的多行注释会着色不准（和 GitHub 一样）。

import hljs from 'highlight.js/lib/common'
import lua from 'highlight.js/lib/languages/lua'

// Luau 在 Lua 之上多几个关键字；`type` 不加，它同时是常用的内置函数名
hljs.registerLanguage('luau', (h) => {
  const def = lua(h)
  const keywords = def.keywords as { keyword: string }
  return { ...def, name: 'Luau', keywords: { ...keywords, keyword: `${keywords.keyword} continue export typeof` } }
})

/** highlight.js 的别名表里没有的扩展名 */
const EXTRA: Record<string, string> = { svelte: 'xml', vue: 'xml', toml: 'ini', jsonc: 'json', json5: 'json', tsx: 'typescript', jsx: 'javascript', mts: 'typescript', cts: 'typescript', rbxlx: 'xml', rbxmx: 'xml' }

/** 路径对应的语言名；认不出的返回 null，按纯文本显示 */
export function languageOf(path: string): string | null {
  const name = path.slice(path.lastIndexOf('/') + 1).toLowerCase()
  const ext = name.slice(name.lastIndexOf('.') + 1)
  const language = EXTRA[ext] ?? ext
  return hljs.getLanguage(language) ? language : null
}

/** 把一段高亮结果按行拆开：跨行的 span 在行尾补上闭合，下一行开头重新打开 */
export function splitHtml(html: string): string[] {
  const lines: string[] = []
  const open: string[] = []
  let line = ''
  for (const [token] of html.matchAll(/<span[^>]*>|<\/span>|\n|[^<\n]+/g)) {
    if (token === '\n') {
      lines.push(line + '</span>'.repeat(open.length))
      line = open.join('')
    } else {
      line += token
      if (token === '</span>') open.pop()
      else if (token.startsWith('<span')) open.push(token)
    }
  }
  lines.push(line)
  return lines
}

/** 超过这个长度的行多半是压缩过的代码，高亮既慢又没意义 */
const MAX_LINE = 2000

/**
 * 一个区块各行的高亮 HTML，下标与 `lines` 一一对应；不高亮时返回 null。
 * 删除行取自旧侧，新增行和上下文取自新侧。
 */
export function highlightHunk(lines: { kind: string; text: string }[], language: string | null): string[] | null {
  if (!language || lines.some((l) => l.text.length > MAX_LINE)) return null
  const side = (skip: string) => {
    const picked = lines.filter((l) => l.kind !== skip && l.kind !== '\\')
    return splitHtml(hljs.highlight(picked.map((l) => l.text).join('\n'), { language, ignoreIllegals: true }).value)
  }
  const [older, newer] = [side('+'), side('-')]
  let [o, n] = [0, 0]
  return lines.map((l) => {
    if (l.kind === '\\') return ''
    if (l.kind === '-') return older[o++]
    if (l.kind === ' ') o++
    return newer[n++]
  })
}
