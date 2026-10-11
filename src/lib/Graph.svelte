<script lang="ts">
  import type { FileChange, Ref, Row } from './api'
  import { expansionRanges, graphIndex, graphPosition } from './graph-rows'
  import { authorColor, authorKey } from './author'
  import Dialog from './Dialog.svelte'
  import { layout } from './layout.svelte'
  import { prefs, setAuthorStyle } from './prefs.svelte'
  import Splitter from './Splitter.svelte'
  import { theme } from './theme.svelte'
  import { fmtTime, t } from './i18n.svelte'

  const CHUNK = 128
  // ponytail: 超过 MAX_LANES 的泳道不画（内核图宽 572）；需要时给图列加横向滚动
  const LANE_CAP = 24
  // 浏览器单个元素的高度上限约 3300 万像素，超出后把滚动位置按比例映射到行号
  const MAX_H = 16_000_000
  const LANES = 8

  let {
    fetchRows,
    count,
    version,
    badges,
    headId,
    selectedRow,
    onselect,
    onmenu,
    compact = false,
    fetchFiles,
    onfile,
  }: {
    fetchRows: (start: number, count: number) => Promise<Row[]>
    count: number
    /** 提交图每次重新加载后加一，用来作废已缓存的行 */
    version: number
    badges: Map<string, Ref[]>
    headId: string | null
    selectedRow: number | null
    onselect: (row: number, id: string) => void
    onmenu: (e: MouseEvent, id: string) => void
    compact?: boolean
    fetchFiles?: (id: string) => Promise<FileChange[]>
    onfile?: (id: string, file: FileChange, preview: boolean) => void
  } = $props()

  const ROW_H = $derived(compact ? 22 : 28)
  const LANE_W = $derived(compact ? 12 : 14)
  const PAD = $derived(compact ? 6 : 12)

  let viewport: HTMLDivElement
  let canvas = $state<HTMLCanvasElement>()
  /** 只高亮这个作者的提交，其余变淡 */
  let focusAuthor = $state<{ key: string; name: string } | null>(null)
  let authorDialog = $state<Dialog>()

  async function styleAuthor(row: Row) {
    const key = authorKey(row)
    const style = prefs.authorStyles[key]
    const values = await authorDialog?.ask({
      title: t.authorStyle(row.author),
      message: row.author_email,
      fields: [
        { key: 'border', label: t.authorBorder, type: 'checkbox', value: style?.border ?? false },
        { key: 'fill', label: t.authorFill, type: 'checkbox', value: style?.fill ?? false },
      ],
    })
    if (values) setAuthorStyle(key, { border: values.border as boolean, fill: values.fill as boolean })
  }
  const cols = $derived(compact ? 'minmax(0, 1fr)' : `minmax(0, 1fr) min(${layout.author}px, 18vw) min(${layout.date}px, 13vw) 72px`)
  let scrollTop = $state(0)
  let height = $state(0)
  let width = $state(0)
  /** 泳道列最多占提交图宽度的 28%，窄窗口下给说明列留地方 */
  const MAX_LANES = $derived(Math.max(4, Math.min(LANE_CAP, Math.floor((width * 0.28 - PAD * 2) / LANE_W))))
  let loaded = $state(0)

  const chunks = new Map<number, Row[]>()
  /** 重新加载后、新行到达前先显示的旧行，免得整屏空白闪一下 */
  let stale = new Map<number, Row[]>()
  const pending = new Set<number>()
  let gen = 0
  let seenVersion = -1
  type Expansion = { id: string; files: FileChange[] | null; error: string }
  let expanded = $state.raw(new Map<number, Expansion>())
  let selectedFile = $state<{ row: number; child: number } | null>(null)
  const ranges = $derived(expansionRanges(Array.from(expanded, ([row, e]) => [row, Math.max(1, e.files?.length ?? 0)])))
  const displayCount = $derived(count + (ranges.length ? ranges.at(-1)!.offset + ranges.at(-1)!.size : 0))

  const total = $derived(displayCount * ROW_H)
  const virt = $derived(Math.min(total, MAX_H))
  const scale = $derived(virt > height ? (total - height) / (virt - height) : 1)
  const top = $derived(scrollTop * scale)
  const first = $derived(Math.floor(top / ROW_H))
  const visible = $derived(Math.ceil(height / ROW_H) + 1)

  const rowAt = (r: number) => (chunks.get(Math.floor(r / CHUNK)) ?? stale.get(Math.floor(r / CHUNK)))?.[r % CHUNK]

  const windowRows = $derived.by(() => {
    loaded
    const out: { commit: number; child: number; row: Row | undefined }[] = []
    for (let r = first; r < Math.min(first + visible, displayCount); r++) {
      const position = graphPosition(r, ranges)
      out.push({ commit: position.row, child: position.child, row: rowAt(position.row) })
    }
    return out
  })

  const lanes = $derived(
    Math.min(MAX_LANES, 1 + Math.max(0, ...windowRows.map(({ row: r }) => (r ? Math.max(r.lane, ...r.through, ...r.out, ...r.incoming) : 0)))),
  )
  const graphW = $derived(PAD * 2 + lanes * LANE_W)

  $effect(() => {
    if (version !== seenVersion) {
      seenVersion = version
      gen++
      stale = new Map([...stale, ...chunks])
      chunks.clear()
      pending.clear()
      expanded = new Map()
      selectedFile = null
    }
    const g = gen
    const needed = new Set(windowRows.map(({ commit }) => Math.floor(commit / CHUNK)))
    for (const c of needed) {
      if (chunks.has(c) || pending.has(c)) continue
      pending.add(c)
      fetchRows(c * CHUNK, CHUNK).then((rows) => {
        if (g !== gen) return
        pending.delete(c)
        if (chunks.size > 256) chunks.clear()
        chunks.set(c, rows)
        if (!pending.size) stale.clear()
        loaded++
      })
    }
  })

  $effect(() => {
    if (!canvas) return
    // 泳道色跟主题走，切主题时重画
    theme.current
    const css = getComputedStyle(canvas)
    const COLORS = Array.from({ length: LANES }, (_, i) => css.getPropertyValue(`--lane-${i}`).trim())
    const dpr = window.devicePixelRatio || 1
    const h = windowRows.length * ROW_H
    canvas.width = graphW * dpr
    canvas.height = h * dpr
    const ctx = canvas.getContext('2d')!
    ctx.scale(dpr, dpr)
    ctx.lineWidth = 1.6
    const x = (lane: number) => PAD + lane * LANE_W + LANE_W / 2
    const link = (lane: number, x0: number, y0: number, x1: number, y1: number) => {
      ctx.strokeStyle = COLORS[lane % COLORS.length]
      ctx.beginPath()
      ctx.moveTo(x0, y0)
      // 竖直方向出入、中间平滑过渡
      ctx.bezierCurveTo(x0, (y0 + y1) / 2, x1, (y0 + y1) / 2, x1, y1)
      ctx.stroke()
    }
    windowRows.forEach(({ row, child }, i) => {
      if (!row) return
      const y = i * ROW_H
      if (child >= 0) {
        for (const l of [...row.through, ...row.out]) if (l < MAX_LANES) link(l, x(l), y, x(l), y + ROW_H)
        return
      }
      const cy = y + ROW_H / 2
      const cx = x(row.lane)
      for (const l of row.through) if (l < MAX_LANES) link(l, x(l), y, x(l), y + ROW_H)
      for (const l of row.incoming) if (l < MAX_LANES) link(l, x(l), y, cx, cy)
      for (const l of row.out) if (l < MAX_LANES) link(l, cx, cy, x(l), y + ROW_H)
      if (row.lane >= MAX_LANES) return
      const color = COLORS[row.lane % COLORS.length]
      ctx.beginPath()
      ctx.arc(cx, cy, compact ? 3 : 4, 0, Math.PI * 2)
      ctx.fillStyle = row.id === headId ? css.getPropertyValue('--bg') : color
      ctx.fill()
      ctx.strokeStyle = color
      ctx.lineWidth = 2
      ctx.stroke()
      ctx.lineWidth = 1.6
    })
  })

  export function scrollToRow(row: number, center = false) {
    scrollToIndex(graphIndex(row, ranges), center)
  }

  function scrollToIndex(index: number, center = false) {
    const y = index * ROW_H
    if (center) viewport.scrollTop = (y - height / 2 + ROW_H / 2) / scale
    else if (y < top) viewport.scrollTop = y / scale
    else if (y + ROW_H > top + height) viewport.scrollTop = (y + ROW_H - height) / scale
  }

  async function expand(index: number, id: string, retry = false) {
    if (!fetchFiles) return
    if (expanded.has(index) && !retry) {
      const next = new Map(expanded)
      next.delete(index)
      expanded = next
      return
    }
    const entry: Expansion = { id, files: null, error: '' }
    expanded = new Map(expanded).set(index, entry)
    try {
      const files = await fetchFiles(id)
      if (expanded.get(index) === entry) expanded = new Map(expanded).set(index, { ...entry, files })
    } catch (e) {
      if (expanded.get(index) === entry) expanded = new Map(expanded).set(index, { ...entry, error: String(e) })
    }
  }

  function select(index: number, child = -1, open = true, preview = true) {
    const row = rowAt(index)
    if (!row) return
    viewport.focus({ preventScroll: true })
    onselect(index, row.id)
    selectedFile = child < 0 ? null : { row: index, child }
    if (!open) return
    if (child < 0) void expand(index, row.id)
    else {
      const entry = expanded.get(index)
      const file = entry?.files?.[child]
      if (file) onfile?.(row.id, file, preview)
      else if (entry?.error) void expand(index, row.id, true)
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLButtonElement) return
    if (fetchFiles && selectedRow !== null) {
      const entry = expanded.get(selectedRow)
      if (['Enter', ' ', 'ArrowRight', 'ArrowLeft'].includes(e.key)) {
        e.preventDefault()
        if (e.key === 'ArrowLeft') {
          if (selectedFile) select(selectedRow, -1, false)
          else if (entry) void expand(selectedRow, entry.id)
        } else if (e.key === 'ArrowRight' && entry) {
          if (!selectedFile && entry.files?.length) select(selectedRow, 0, false)
        } else select(selectedRow, selectedFile?.child ?? -1)
        scrollToIndex(graphIndex(selectedRow, ranges) + (selectedFile ? selectedFile.child + 1 : 0))
        return
      }
    }
    const step = { ArrowDown: 1, ArrowUp: -1, PageDown: visible - 2, PageUp: 2 - visible }[e.key]
    if (!step || (!compact && selectedRow === null)) return
    e.preventDefault()
    const current = selectedRow === null ? -1 : graphIndex(selectedRow, ranges) + (selectedFile ? selectedFile.child + 1 : 0)
    const next = Math.max(0, Math.min(displayCount - 1, current + step))
    const position = graphPosition(next, ranges)
    select(position.row, position.child, !fetchFiles)
    scrollToIndex(next)
  }

  const kind = (name: string) => (name.startsWith('refs/heads/') ? 'branch' : name.startsWith('refs/tags/') ? 'tag' : 'remote')
  const short = (name: string) => name.replace(/^refs\/(heads|tags|remotes)\//, '')
</script>

<Dialog bind:this={authorDialog} />

{#if !compact}
<div class="head" style:padding-left="{graphW}px" style:grid-template-columns={cols}>
  <span>
    {t.colSubject}
    {#if focusAuthor}
      <button class="chip" style:color={authorColor(focusAuthor.name)} onclick={() => (focusAuthor = null)}>{t.onlyAuthor(focusAuthor.name)} ×</button>
    {/if}
  </span>
  <span class="cell">
    <span class="grip" title={t.dragHint}><Splitter key="author" min={80} max={360} invert /></span>
    <span title={t.authorHint}>{t.colAuthor}</span>
  </span>
  <span class="cell"><span class="grip" title={t.dragHint}><Splitter key="date" min={96} max={220} invert /></span>{t.colDate}</span>
  <span>{t.colCommit}</span>
</div>
{/if}
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="viewport" class:compact style:--row="{ROW_H}px" bind:this={viewport} bind:clientHeight={height} bind:clientWidth={width} onscroll={() => (scrollTop = viewport.scrollTop)} {onkeydown} tabindex="0" role={fetchFiles ? 'treegrid' : 'grid'} aria-rowcount={displayCount} aria-activedescendant={selectedRow === null ? undefined : `graph-${selectedRow}-${selectedFile?.child ?? -1}`}>
  <div style:height="{virt}px">
    <div class="window" style:transform="translateY({scrollTop - (top % ROW_H)}px)">
      <canvas bind:this={canvas} style:width="{graphW}px" style:height="{windowRows.length * ROW_H}px"></canvas>
      {#each windowRows as { row, commit, child }, i (first + i)}
        {@const entry = expanded.get(commit)}
        {@const file = child >= 0 ? entry?.files?.[child] : undefined}
        {@const selected = commit === selectedRow && child === (selectedFile?.child ?? -1)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="row"
          class:selected
          class:file-row={child >= 0}
          class:dim={focusAuthor !== null && (!row || authorKey(row) !== focusAuthor.key)}
          style:padding-left="{graphW}px"
          style:grid-template-columns={cols}
          role="row"
          id="graph-{commit}-{child}"
          aria-rowindex={first + i + 1}
          aria-level={fetchFiles ? (child < 0 ? 1 : 2) : undefined}
          aria-expanded={fetchFiles && child < 0 ? !!entry : undefined}
          aria-selected={selected}
          tabindex="-1"
          title={file ? `${file.old_path ? `${file.old_path} → ` : ''}${file.path}\n${t.status[file.status] ?? file.status}` : compact && row ? `${row.subject}\n${row.author} · ${fmtTime(row.time)} · ${row.id.slice(0, 7)}` : undefined}
          onclick={() => select(commit, child)}
          ondblclick={() => child >= 0 && select(commit, child, true, false)}
          oncontextmenu={(e) => row && (select(commit, child, false), onmenu(e, row.id))}
        >
          {#if child >= 0}
            <span class="file-content" role="gridcell">
              {#if file}
                <span class="file-status" class:added={file.status === 'A'} class:deleted={file.status === 'D'} aria-hidden="true">{file.status}</span>
                <span class="file-name">{file.path.split('/').at(-1)}</span>
                <span class="file-path">{file.path.includes('/') ? file.path.slice(0, file.path.lastIndexOf('/')) : ''}</span>
              {:else if entry?.error}
                <span role="alert">{entry.error}</span><button onclick={(e) => { e.stopPropagation(); if (row) void expand(commit, row.id, true) }}>{t.vscode.graphRetryFiles}</button>
              {:else}
                <span class="muted" role="status">{entry?.files ? t.vscode.graphNoFiles : t.vscode.graphLoadingFiles}</span>
              {/if}
            </span>
          {:else if row}
            <span class="subject">
              {#if fetchFiles}<span class="twistie" aria-hidden="true">{entry ? '⌄' : '›'}</span>{/if}
              {#if badges.has(row.id)}
                <span class="badges">
                  {#each badges.get(row.id) ?? [] as ref (ref.name)}
                    <span class="badge {kind(ref.name)}" title={short(ref.name)}>{short(ref.name)}</span>
                  {/each}
                </span>
              {/if}
              <span>{row.subject}</span>
              {#if compact}<span class="muted by">{row.author}</span>{/if}
            </span>
            {#if !compact}<button
              class="author"
              class:boxed={prefs.authorStyles[authorKey(row)]?.border}
              class:filled={prefs.authorStyles[authorKey(row)]?.fill}
              style:color={authorColor(row.author)}
              title={t.authorHint}
              onclick={(e) => (e.stopPropagation(), (focusAuthor = focusAuthor?.key === authorKey(row) ? null : { key: authorKey(row), name: row.author }))}
              oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); styleAuthor(row) }}
              onkeydown={(e) => {
                if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
                  e.preventDefault()
                  e.stopPropagation()
                  styleAuthor(row)
                }
              }}
            >
              {row.author}
            </button>
            <span class="muted">{fmtTime(row.time)}</span>
            <span class="muted mono">{row.id.slice(0, 7)}</span>
            {/if}
          {/if}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .head,
  .row {
    display: grid;
    gap: 12px;
    align-items: center;
    padding-right: 12px;
  }
  .head {
    position: relative;
    z-index: 3;
    flex: none;
    height: var(--row);
    color: var(--muted);
    font-size: var(--fs-sm);
    border-bottom: 1px solid var(--border);
    /* 多出的 12px 是下面列表滚动条的宽度，表头和各列才能对齐 */
    padding-right: 24px;
  }
  .cell {
    position: relative;
  }
  /* 列分隔条贴在列的左边缘 */
  .grip {
    position: absolute;
    left: -6px;
    top: 0;
    bottom: 0;
    display: flex;
  }
  .chip {
    margin-left: 8px;
    padding: 0 8px;
    border: 1px solid currentColor;
    border-radius: var(--r-lg);
    background: none;
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    outline: none;
  }
  .viewport:not(.compact) {
    scrollbar-gutter: stable;
  }
  .window {
    position: relative;
    will-change: transform;
  }
  canvas {
    position: absolute;
    left: 0;
    top: 0;
    pointer-events: none;
  }
  .row {
    height: var(--row);
    cursor: default;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.selected {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 var(--accent);
  }
  /* 高亮某个作者时，其他人的提交退后 */
  .row.dim > :global(*) {
    opacity: 0.3;
  }
  .row span,
  .author {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .author {
    padding: 0;
    border: 0;
    background: none;
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
  }
  .author:hover {
    text-decoration: underline;
  }
  /* 可选：给作者名加边框和（或）底色，颜色都跟着作者色走 */
  .author.boxed,
  .author.filled {
    justify-self: start;
    max-width: 100%;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: var(--r-lg);
    line-height: 20px;
  }
  .author.boxed {
    border-color: currentColor;
  }
  .author.filled {
    background: color-mix(in srgb, currentColor 18%, transparent);
  }
  .muted {
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .mono {
    font-family: var(--mono);
  }
  /* 引用标签最多占说明列的一半，剩下的留给提交标题 */
  .subject {
    display: flex;
    align-items: center;
  }
  .badges {
    flex: 0 1 auto;
    max-width: 50%;
  }
  .subject > span:last-child {
    flex: 1;
    min-width: 0;
  }
  /* 紧凑模式：标题按内容占宽，作者跟在后面用剩下的地方，窄的时候先省略作者 */
  .compact .subject > span:nth-last-child(2) {
    flex: 0 1 auto;
    min-width: 0;
  }
  .compact .by {
    margin-left: 4px;
  }
  .badge {
    margin-right: 8px;
    padding: 2px 8px;
    border-radius: var(--r-sm);
    font-size: var(--fs-sm);
    font-weight: 600;
    border: 1px solid;
  }
  .badge.branch {
    color: var(--text);
    background: var(--accent-soft);
    border-color: var(--border-strong);
    border-left: 3px solid var(--ref-branch, var(--accent));
  }
  .badge.remote {
    color: var(--text);
    border-color: var(--border-strong);
    font-weight: 400;
  }
  .badge.tag {
    color: var(--text);
    border-color: color-mix(in srgb, var(--yellow) 45%, transparent);
  }
  .compact .row {
    gap: 4px;
    padding-right: 4px;
  }
  .compact .subject { gap: 4px; }
  .compact .badges {
    display: flex;
    gap: 4px;
    min-width: 0;
  }
  .compact .badge {
    min-width: 0;
    margin: 0;
    padding: 0 4px;
    line-height: 16px;
    border-radius: 3px;
    font-weight: 500;
  }
  .twistie { flex: none; width: 10px; text-align: center; color: var(--muted); }
  .file-content { display: flex; align-items: center; gap: 5px; min-width: 0; padding-left: 14px; }
  .file-name { flex: 0 1 auto; }
  .file-path { flex: 1; min-width: 0; color: var(--muted); font-size: var(--fs-sm); }
  .file-status { flex: none; width: 12px; color: var(--yellow); font-size: var(--fs-sm); text-align: center; }
  .file-status.added { color: var(--diff-added); }
  .file-status.deleted { color: var(--diff-deleted); }
  .file-content button { flex: none; color: var(--text); background: var(--hover); border: 1px solid var(--border-strong); font: inherit; cursor: pointer; }
  .viewport:focus-visible .row.selected { outline: 1px solid var(--accent); outline-offset: -1px; }
</style>
