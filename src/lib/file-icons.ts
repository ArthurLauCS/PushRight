// 文件列表里按类型区分的小图标：形状表示大类（代码、文档、图片…），颜色区分具体语言或格式。
// 24×24 描边路径，和 Icon.svelte 同一套画法。

export const GLYPHS = {
  file: 'M6 3h8l4 4v14H6zM14 3v4h4',
  code: 'm8 7-5 5 5 5M16 7l5 5-5 5',
  doc: 'M6 3h8l4 4v14H6zM14 3v4h4M9 12h6M9 16h6',
  braces: 'M9 4c-2 0-3 1-3 3v2c0 1-1 2-2 3 1 1 2 2 2 3v2c0 2 1 3 3 3M15 4c2 0 3 1 3 3v2c0 1 1 2 2 3-1 1-2 2-2 3v2c0 2-1 3-3 3',
  settings: 'M4 7h16M4 17h16M9 4v6M15 14v6',
  table: 'M4 5h16v14H4zM4 10h16M4 15h16M10 5v14',
  image: 'M4 5h16v14H4zM4 16l4-4 4 4 3-3 5 5M9 9h.01',
  media: 'M4 5h16v14H4zM10 9l5 3-5 3z',
  box: 'M4 8l8-4 8 4v8l-8 4-8-4zM4 8l8 4 8-4M12 12v8',
  font: 'M6 19 12 5l6 14M8.5 14h7',
  terminal: 'm5 8 4 4-4 4M12 17h7',
  lock: 'M7 11V8a5 5 0 0 1 10 0v3M6 11h12v9H6z',
  git: 'M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 9a9 9 0 0 1-9 9',
  roblox: 'M7 4l13 3-3 13-13-3zM11 11l3 .7-.7 3-3-.7z',
  folder: 'M3 7V5a2 2 0 0 1 2-2h4l2 3h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
} as const

export type Glyph = keyof typeof GLYPHS
export interface FileIcon { glyph: Glyph; color: string }

const GRAY = 'var(--muted)'
/** 「形状 颜色: 扩展名…」；颜色取在深浅两套主题上都看得清的中间调 */
const TABLE = `
code #00a2ff: luau
code #7986cb: lua
roblox #00a2ff: rbxl rbxlx rbxm rbxmx
code #3b82c4: ts mts cts tsx
code #d4a72c: js mjs cjs jsx
code #4b8bbe: py pyw
code #dea584: rs
code #00add8: go
code #659ad2: c h
code #f34b7d: cpp cc cxx hpp hh
code #9b4f96: cs
code #e76f00: java
code #a97bff: kt kts
code #f05138: swift
code #cc342d: rb
code #8892bf: php
code #ff3e00: svelte
code #41b883: vue
code #e44d26: html htm
code #8a63d2: css
code #cf649a: scss sass less
code #6e9f18: glsl hlsl shader
doc #519aba: md markdown mdx
doc ${GRAY}: txt log rst
doc #b7472a: pdf doc docx ppt pptx
braces #cbcb41: json jsonc json5
braces #e37933: xml plist
braces #6d8086: yml yaml
settings #9c8b6e: toml
settings #6d8086: ini cfg conf env editorconfig properties
table #4caf50: csv tsv xls xlsx
table #e38c00: sql sqlite db
image #a074c4: png jpg jpeg gif webp bmp ico tga psd
image #ffb13b: svg
media #d16d9e: mp3 wav ogg flac mp4 mov webm avi mkv
box #afb42b: zip rar 7z gz tar tgz
box ${GRAY}: exe dll so dylib bin dat wasm
font #ec5f67: ttf otf woff woff2
terminal #89e051: sh bash zsh fish
terminal #5391fe: ps1 bat cmd
lock ${GRAY}: lock pem key crt
`
const BY_EXT = new Map<string, FileIcon>()
for (const row of TABLE.trim().split('\n')) {
  const [head, exts] = row.split(': ')
  const [glyph, color] = head.split(' ')
  for (const ext of exts.split(' ')) BY_EXT.set(ext, { glyph: glyph as Glyph, color })
}
/** 没有扩展名或扩展名说明不了问题的文件，按整个文件名认 */
const BY_NAME: Record<string, FileIcon> = {
  '.gitignore': { glyph: 'git', color: '#f05033' },
  '.gitattributes': { glyph: 'git', color: '#f05033' },
  '.gitmodules': { glyph: 'git', color: '#f05033' },
  'dockerfile': { glyph: 'box', color: '#2496ed' },
  'makefile': { glyph: 'terminal', color: '#e37933' },
  'license': { glyph: 'doc', color: '#cbcb41' },
}
const DEFAULT: FileIcon = { glyph: 'file', color: GRAY }

export function fileIcon(path: string): FileIcon {
  const name = path.slice(path.lastIndexOf('/') + 1).toLowerCase()
  return BY_NAME[name] ?? BY_EXT.get(name.slice(name.lastIndexOf('.') + 1)) ?? DEFAULT
}
