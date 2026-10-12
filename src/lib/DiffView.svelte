<script lang="ts">
  import type { Diff, Line } from './api'
  import { CONTEXT_STEPS, diffPrefs, partialBlocked, setDiffPref } from './diff-prefs.svelte'
  import { splitRows, whitespaceMarks, type Cell } from './diff-rows'
  import Icon, { type IconName } from './Icon.svelte'
  import Label from './Label.svelte'
  import { t, errorText } from './i18n.svelte'

  // Large previews can switch to streaming pages; editing retains the original patch.
  const MAX_LINES = 5000

  let {
    diff: source,
    mode = 'readonly',
    onapply,
    ondiscard,
    fetchPage,
  }: {
    diff: Diff | null
    /** unstaged / staged 时可以按区块、按行暂存或取消暂存 */
    mode?: 'readonly' | 'unstaged' | 'staged'
    onapply?: (hunk: number, header: string, lines: number[]) => void
    /** 只在 unstaged 模式下出现「丢弃」按钮 */
    ondiscard?: (hunk: number, header: string, lines: number[]) => void
    fetchPage?: (skip: number) => Promise<Diff>
  } = $props()

  let page = $state<Diff | null>(null)
  let offset = $state(0)
  let loading = $state(false)
  let error = $state('')
  let request = 0
  const diff = $derived(page ?? source)
  $effect(() => { source; page = null; offset = 0; error = ''; loading = false; ++request })
  async function load(skip: number) {
    if (!fetchPage) return
    const id = ++request
    loading = true
    error = ''
    try { const result = await fetchPage(skip); if (id === request) { page = result; offset = skip } }
    catch (e) { if (id === request) error = errorText(e) }
    finally { if (id === request) loading = false }
  }

  /** 选中的行，键为「区块下标:行下标」 */
  let selected = $state(new Set<string>())
  $effect(() => {
    diff
    selected = new Set()
  })

  const hunks = $derived.by(() => {
    let budget = MAX_LINES
    return (diff?.hunks ?? []).map((h) => {
      const lines = h.lines.slice(0, Math.max(0, budget))
      budget -= h.lines.length
      return { ...h, lines }
    })
  })
  const truncated = $derived((diff?.hunks ?? []).reduce((n, h) => n + h.lines.length, 0) > MAX_LINES)

  /** 能不能按行 / 按区块暂存：只读、分页预览、过滤过的视图都不行 */
  const editable = $derived(mode !== 'readonly' && !page && !partialBlocked())
  const step = $derived(CONTEXT_STEPS.indexOf(diffPrefs.context))
  const setContext = (delta: number) => setDiffPref('context', CONTEXT_STEPS[step + delta])

  const changed = (kind: string) => kind === '+' || kind === '-'
  const picked = (h: number) => [...selected].filter((k) => k.startsWith(`${h}:`)).map((k) => +k.split(':')[1])

  function toggle(h: number, i: number, kind: string) {
    if (!editable || !changed(kind)) return
    const next = new Set(selected)
    const key = `${h}:${i}`
    if (!next.delete(key)) next.add(key)
    selected = next
  }

  function apply(h: number, action = onapply) {
    const hunk = diff!.hunks[h]
    const lines = picked(h)
    // 没选行就是整个区块
    action?.(h, hunk.header, lines.length ? lines : hunk.lines.flatMap((l, i) => (changed(l.kind) ? [i] : [])))
  }

  function label(h: number) {
    const n = picked(h).length
    if (mode === 'unstaged') return n ? t.stageLines(n) : t.stageHunk
    return n ? t.unstageLines(n) : t.unstageHunk
  }
</script>

{#snippet tool(icon: IconName, label: string, on: boolean | undefined, action: () => void, disabled = false)}
  <button class="btn small quiet icon" class:on title={label} aria-label={label} aria-pressed={on} {disabled} onclick={action}><Icon name={icon} size={14} /></button>
{/snippet}

{#snippet half(h: number, cell: Cell<Line> | undefined, old: boolean)}
  {#if cell}
    {@const line = cell.line}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="line"
      class:add={line.kind === '+'}
      class:del={line.kind === '-'}
      class:meta={line.kind === '\\'}
      class:pick={editable && changed(line.kind)}
      class:selected={selected.has(`${h}:${cell.index}`)}
      class:right={!old}
      onclick={() => toggle(h, cell.index, line.kind)}
    >
      <span class="no">{(old ? line.old_no : line.new_no) ?? ''}</span><span class="sign">{line.kind}</span><span class="text" data-marks={diffPrefs.hidden ? whitespaceMarks(line.text) : undefined}>{line.text}</span>
    </div>
  {:else}<div class="line gap" class:right={!old}></div>{/if}
{/snippet}

<div class="view">
<div class="bar" role="toolbar">
  {@render tool('space', t.diffTools.ignoreWs, diffPrefs.ignoreWs, () => setDiffPref('ignoreWs', !diffPrefs.ignoreWs))}
  {@render tool('pilcrow', t.diffTools.hidden, diffPrefs.hidden, () => setDiffPref('hidden', !diffPrefs.hidden))}
  {@render tool('wrap', t.diffTools.wrap, diffPrefs.wrap, () => setDiffPref('wrap', !diffPrefs.wrap))}
  <span class="sep"></span>
  {@render tool('contextLess', t.diffTools.less(diffPrefs.context), undefined, () => setContext(-1), diffPrefs.whole || step <= 0)}
  {@render tool('contextMore', t.diffTools.more(diffPrefs.context), undefined, () => setContext(1), diffPrefs.whole || step === CONTEXT_STEPS.length - 1)}
  {@render tool('wholeFile', t.diffTools.whole, diffPrefs.whole, () => setDiffPref('whole', !diffPrefs.whole))}
  <span class="sep"></span>
  {@render tool('split', t.diffTools.sideBySide, diffPrefs.sideBySide, () => setDiffPref('sideBySide', !diffPrefs.sideBySide))}
</div>
<div class="diff" class:wrap={diffPrefs.wrap}>
  {#if error}<p class="note">{error}</p>{/if}
  {#if page}
    <div class="hunk">
      <button class="btn small" disabled={loading || offset === 0} onclick={() => load(Math.max(0, offset - 1000))}>{t.tools.previousPage}</button>
      <span>{offset + 1} · {t.tools.readonlyPage}</span>
      <button class="btn small" disabled={loading || page.next == null} onclick={() => load(page!.next!)}>{t.tools.nextPage}</button>
    </div>
  {/if}
  {#if !diff}
    <p class="note">{t.pickFile}</p>
  {:else if diff.binary}
    {#if diff.images?.some(Boolean)}
      <div class="images">
        {#each diff.images as src, i}
          <figure><figcaption>{i ? t.tools.after : t.tools.before}</figcaption>{#if src}<img {src} alt={i ? t.tools.after : t.tools.before} />{:else}<p>{t.tools.absent}</p>{/if}</figure>
        {/each}
      </div>
    {:else}<p class="note">{t.binary}</p>{/if}
  {:else if diff.too_large}
    <p class="note">{t.tooLarge}</p>
    {#if fetchPage}<button class="btn" disabled={loading} onclick={() => load(0)}>{t.tools.pagedPreview}</button>{/if}
  {:else if !diff.hunks.length}
    <p class="note">{t.noDiff}</p>
  {:else}
    <div class="body" class:split={diffPrefs.sideBySide}>
      {#if mode !== 'readonly' && !page && partialBlocked()}<p class="hint">{t.diffTools.filtered}</p>{/if}
      {#each hunks as hunk, h (h)}
        <div class="hunk">
          <span>{hunk.header}</span>
          {#if editable && mode === 'unstaged' && ondiscard}
            <button class="btn small danger" title={picked(h).length ? t.discardLines(picked(h).length) : t.discardHunk} onclick={() => apply(h, ondiscard)}>
              <Label text={picked(h).length ? t.discardLines(picked(h).length) : t.discardHunk} />
            </button>
          {/if}
          {#if editable}<button class="btn small" title={label(h)} onclick={() => apply(h)}><Label text={label(h)} /></button>{/if}
        </div>
        {#if diffPrefs.sideBySide}
          {#each splitRows(hunk.lines) as row, r (r)}
            {@render half(h, row.left, true)}{@render half(h, row.right, false)}
          {/each}
        {:else}
          {#each hunk.lines as line, i (i)}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div
              class="line"
              class:add={line.kind === '+'}
              class:del={line.kind === '-'}
              class:meta={line.kind === '\\'}
              class:pick={editable && changed(line.kind)}
              class:selected={selected.has(`${h}:${i}`)}
              onclick={() => toggle(h, i, line.kind)}
            >
              <span class="no">{line.old_no ?? ''}</span><span class="no">{line.new_no ?? ''}</span><span class="sign">{line.kind}</span><span class="text" data-marks={diffPrefs.hidden ? whitespaceMarks(line.text) : undefined}>{line.text}</span>
            </div>
          {/each}
        {/if}
      {/each}
      {#if truncated}<p class="note">{t.truncated(MAX_LINES)}</p>{#if fetchPage}<button class="btn" disabled={loading} onclick={() => load(0)}>{t.tools.pagedPreview}</button>{/if}{/if}
      {#if diff.clipped}<p class="note">{t.tools.clippedLine}</p>{/if}
    </div>
  {/if}
</div>
</div>

<style>
  .view {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 2px;
    padding: 4px 8px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }
  .sep {
    width: 1px;
    height: 16px;
    margin: 0 4px;
    background: var(--border);
  }
  .images { display: flex; gap: 16px; padding: 16px; }
  figure { flex: 1; min-width: 0; margin: 0; }
  figcaption { margin-bottom: 12px; }
  img { max-width: 100%; background: repeating-conic-gradient(#bbb 0% 25%, #eee 0% 50%) 50% / 20px 20px; }
  .diff {
    flex: 1;
    min-height: 0;
    overflow: auto;
    /* 区块标题用 100cqw 取本容器宽度，横向滚动时保持不动 */
    container-type: inline-size;
    background: var(--bg);
    font: var(--fs-sm) / 20px var(--mono);
    user-select: text;
  }
  .body {
    width: max-content;
    min-width: 100%;
  }
  .note {
    margin: 0;
    padding: 48px 24px;
    text-align: center;
    color: var(--muted);
    font-family: var(--prose);
    font-size: var(--fs-lg);
  }
  .hint {
    position: sticky;
    left: 0;
    width: 100cqw;
    margin: 0;
    padding: 8px 12px;
    color: var(--yellow);
    font-family: var(--prose);
    font-size: var(--fs-md);
    white-space: normal;
  }
  .hunk {
    position: sticky;
    left: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100cqw;
    height: 32px;
    padding: 0 8px 0 12px;
    color: var(--muted);
    background: var(--panel);
    border-block: 1px solid var(--border);
  }
  .hunk span {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hunk .btn {
    flex: none;
    font-family: var(--font);
  }
  /* 差异栏窄时区块按钮只留名字，命令在悬停提示里 */
  @container (max-width: 640px) {
    .hunk :global(.cmd) {
      display: none;
    }
  }
  .line {
    border-left: 3px solid transparent;
    display: flex;
    white-space: pre;
    tab-size: 4;
  }
  /* 自动换行：一条源码行仍是一行，行号列不动，只是正文折返 */
  .wrap .body {
    width: 100%;
  }
  .wrap .line {
    min-width: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .wrap .text {
    flex: 1;
    min-width: 0;
  }
  /* 并排：整个 diff 共用一个两列网格。每栏至少占一半宽，再按自己最长的一行伸展，
     所以右栏不会被左栏的长行推出屏幕；同一网格行里两侧自动等高 */
  .split {
    display: grid;
    grid-template-columns: repeat(2, minmax(50cqw, max-content));
  }
  .wrap .split {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .split > :not(.line) {
    grid-column: 1 / -1;
  }
  /* 两栏之间的分隔线；悬停和选中的标记画在同一位置，会盖住它 */
  .right {
    box-shadow: -1px 0 var(--border);
  }
  .gap {
    background: var(--hover);
  }
  /* 隐藏字符：标记串叠在原文上，逐格对齐。画在伪元素里，复制出来的仍是真实的空格和制表符 */
  .text[data-marks] {
    position: relative;
  }
  .text[data-marks]::before {
    content: attr(data-marks);
    position: absolute;
    inset: 0;
    color: var(--muted);
    opacity: 0.8;
    pointer-events: none;
  }
  .no {
    flex: none;
    width: 48px;
    padding-right: 8px;
    text-align: right;
    color: var(--muted);
    opacity: 0.6;
    user-select: none;
  }
  .sign {
    flex: none;
    width: 24px;
    text-align: center;
    color: var(--muted);
    user-select: none;
  }
  .add {
    border-left-color: var(--diff-added);
    background: color-mix(in srgb, var(--diff-added) 22%, transparent);
  }
  .add .sign {
    color: var(--text);
    font-weight: 700;
  }
  .del {
    border-left-color: var(--diff-deleted);
    background: color-mix(in srgb, var(--diff-deleted) 22%, transparent);
  }
  .del .sign {
    color: var(--text);
    font-weight: 700;
  }
  .meta {
    color: var(--muted);
  }
  .pick {
    cursor: pointer;
  }
  .pick:hover {
    box-shadow: inset 3px 0 var(--border-strong);
  }
  .selected,
  .selected:hover {
    box-shadow: inset 3px 0 var(--accent);
    background: var(--accent-soft);
  }
</style>
