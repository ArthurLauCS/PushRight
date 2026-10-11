<script lang="ts">
  import type { Diff } from './api'
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

  const changed = (kind: string) => kind === '+' || kind === '-'
  const picked = (h: number) => [...selected].filter((k) => k.startsWith(`${h}:`)).map((k) => +k.split(':')[1])

  function toggle(h: number, i: number, kind: string) {
    if (page || mode === 'readonly' || !changed(kind)) return
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

<div class="diff">
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
    <div class="body">
      {#each hunks as hunk, h (h)}
        <div class="hunk">
          <span>{hunk.header}</span>
          {#if !page && mode === 'unstaged' && ondiscard}
            <button class="btn small danger" title={picked(h).length ? t.discardLines(picked(h).length) : t.discardHunk} onclick={() => apply(h, ondiscard)}>
              <Label text={picked(h).length ? t.discardLines(picked(h).length) : t.discardHunk} />
            </button>
          {/if}
          {#if !page && mode !== 'readonly'}<button class="btn small" title={label(h)} onclick={() => apply(h)}><Label text={label(h)} /></button>{/if}
        </div>
        {#each hunk.lines as line, i (i)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div
            class="line"
            class:add={line.kind === '+'}
            class:del={line.kind === '-'}
            class:meta={line.kind === '\\'}
            class:pick={!page && mode !== 'readonly' && changed(line.kind)}
            class:selected={selected.has(`${h}:${i}`)}
            onclick={() => toggle(h, i, line.kind)}
          >
            <span class="no">{line.old_no ?? ''}</span><span class="no">{line.new_no ?? ''}</span><span class="sign">{line.kind}</span><span>{line.text}</span>
          </div>
        {/each}
      {/each}
      {#if truncated}<p class="note">{t.truncated(MAX_LINES)}</p>{#if fetchPage}<button class="btn" disabled={loading} onclick={() => load(0)}>{t.tools.pagedPreview}</button>{/if}{/if}
      {#if diff.clipped}<p class="note">{t.tools.clippedLine}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .images { display: flex; gap: 16px; padding: 16px; }
  figure { flex: 1; min-width: 0; margin: 0; }
  figcaption { margin-bottom: 12px; }
  img { max-width: 100%; background: repeating-conic-gradient(#bbb 0% 25%, #eee 0% 50%) 50% / 20px 20px; }
  .diff {
    flex: 1;
    min-width: 0;
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
