<script lang="ts">
  import type { Detail, Diff } from './api'
  import DiffView from './DiffView.svelte'
  import FileIcon from './FileIcon.svelte'
  import FileListBar from './FileListBar.svelte'
  import FileTree from './FileTree.svelte'
  import { indent } from './file-tree'
  import { authorColor } from './author'
  import { layout } from './layout.svelte'
  import Splitter from './Splitter.svelte'
  import { fmtTime, t, errorText } from './i18n.svelte'

  let {
    detail,
    onjump,
    fetchDiff,
    fetchPage,
    reviewKey = '',
    comparison = false,
    onfilemenu,
  }: {
    detail: Detail | null; onjump: (id: string) => void; fetchDiff: (id: string, path: string) => Promise<Diff>; fetchPage?: (id: string, path: string, skip: number) => Promise<Diff>; reviewKey?: string; comparison?: boolean
    /** 文件行的右键菜单；参数是提交 id 和文件路径 */
    onfilemenu?: (e: MouseEvent, id: string, path: string) => void
  } = $props()

  // 提交信息第一行是标题，其余是正文
  const subject = $derived(detail?.message.split('\n')[0] ?? '')
  const body = $derived(detail?.message.split('\n').slice(1).join('\n').trim() ?? '')

  /** 正在看 diff 的文件；null 时右侧显示提交信息 */
  let file = $state<string | null>(null)
  let diff = $state.raw<Diff | null>(null)
  let error = $state('')
  let filter = $state('')
  let tree = $state<{ setAll(open: boolean): void }>()
  const files = $derived.by(() => {
    const query = filter.trim().toLowerCase()
    return (detail?.files ?? []).filter((f) => !query || f.path.toLowerCase().includes(query))
  })
  const base = (p: string) => p.slice(p.lastIndexOf('/') + 1)
  let reviewed = $state<string[]>([])
  $effect(() => { reviewed = JSON.parse(localStorage.getItem(reviewKey) ?? '[]') })
  function review(path: string) {
    reviewed = reviewed.includes(path) ? reviewed.filter((p) => p !== path) : [...reviewed, path]
    if (reviewKey) localStorage.setItem(reviewKey, JSON.stringify(reviewed))
  }

  $effect(() => {
    detail
    file = null
  })
  /** 已经显示着的文件；只换 diff 选项时留着旧内容等新结果，不先清空 */
  let shown = ''
  $effect(() => {
    const f = file
    const key = `${detail?.id}\n${f}`
    if (key !== shown) diff = null
    shown = key
    error = ''
    if (!f || !detail) return
    let stale = false
    fetchDiff(detail.id, f).then(
      (d) => stale || (diff = d),
      (e) => { if (!stale) error = errorText(e) },
    )
    return () => (stale = true)
  })
</script>

<section style:height="min({layout.detail}px, 62%)">
  {#if detail}
    <div class="files" style:width="min({layout.files}px, 45%)">
      <h3>{t.files(detail.files.length)}</h3>
      <FileListBar bind:filter onfold={(open) => tree?.setAll(open)} />
      <FileTree items={files} compact bind:this={tree}>
        {#snippet row(f, depth)}
          <div class="file-row" style:padding-left={indent(depth)}>
          <input type="checkbox" aria-label={`${t.tools.reviewed}: ${f.path}`} title={t.tools.reviewed} checked={reviewed.includes(f.path)} onchange={() => review(f.path)} />
          <button class="file" class:selected={file === f.path} title={f.old_path ? `${f.old_path} → ${f.path}` : f.path} onclick={() => (file = file === f.path ? null : f.path)} oncontextmenu={(e) => onfilemenu?.(e, detail!.id, f.path)}>
            <span class="status s{f.status}" title={t.status[f.status] ?? f.status}>{f.status}</span>
            <FileIcon path={f.path} />
            <span class="path">{#if f.old_path}<span class="muted">{depth < 0 ? f.old_path : base(f.old_path)} → </span>{/if}{depth < 0 ? f.path : base(f.path)}</span>
          </button>
          </div>
        {/snippet}
      </FileTree>
    </div>
    <Splitter key="files" min={200} max={640} />
    {#if file}
      {#if error}<p class="empty">{error}</p>{:else}<DiffView {diff} path={file} fetchPage={fetchPage ? (skip) => fetchPage!(detail!.id, file!, skip) : undefined} />{/if}
    {:else}
      <div class="info">
        <h2>{subject}</h2>
        {#if body}<pre class="message">{body}</pre>{/if}
        {#if !comparison}<dl>
          <dt>{t.colCommit}</dt>
          <dd class="mono">{detail.id}</dd>
          <dt>{t.author}</dt>
          <dd><strong style:color={authorColor(detail.author)}>{detail.author}</strong> <span class="muted">&lt;{detail.author_email}&gt; · {fmtTime(detail.author_time)}</span></dd>
          {#if detail.committer !== detail.author || detail.committer_time !== detail.author_time}
            <dt>{t.committer}</dt>
            <dd>{detail.committer} <span class="muted">· {fmtTime(detail.committer_time)}</span></dd>
          {/if}
          {#if detail.parents.length}
            <dt>{t.parents}</dt>
            <dd>
              {#each detail.parents as p (p)}
                <button class="mono" onclick={() => onjump(p)}>{p.slice(0, 8)}</button>
              {/each}
            </dd>
          {/if}
        </dl>{/if}
      </div>
    {/if}
  {:else}
    <p class="empty">{t.noSelection}</p>
  {/if}
</section>

<style>
  .file-row { display: flex; align-items: center; padding-left: 8px; }
  .file-row .file { min-width: 0; }
  section {
    flex: none;
    display: flex;
    background: var(--panel);
    user-select: text;
  }
  .info {
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding: 16px 24px;
  }
  .files {
    flex: none;
    overflow: auto;
    padding: 12px 0;
    user-select: none;
  }
  /* 提交说明是这块的主角：标题字族、最大行宽 72ch */
  h2 {
    max-width: 40em;
    margin: 0 0 12px;
    font-size: var(--fs-lg);
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .message {
    max-width: 72ch;
    margin: 0 0 16px;
    font-family: var(--prose);
    font-size: var(--fs-md);
    line-height: var(--lh-body);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  dl {
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 16px;
    margin: 0;
    font-size: var(--fs-sm);
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  dd button {
    margin-right: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }
  dd button:hover {
    text-decoration: underline;
  }
  h3 {
    margin: 0 12px 8px;
    font-family: var(--font);
    font-size: var(--fs-sm);
    font-weight: 400;
    color: var(--muted);
    letter-spacing: 0.04em;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 24px;
    padding: 0 12px;
    border: 0;
    background: none;
    font-size: var(--fs-sm);
    text-align: left;
  }
  .file:hover {
    background: var(--hover);
  }
  .file:active:enabled {
    transform: none;
  }
  .file.selected {
    background: var(--accent-soft);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status {
    flex: none;
    width: 16px;
    text-align: center;
    font: 600 var(--fs-sm) var(--mono);
    color: var(--yellow);
  }
  .sA {
    color: var(--diff-added);
  }
  .sD {
    color: var(--diff-deleted);
  }
  .sR,
  .sC {
    color: var(--accent);
  }
  .muted {
    color: var(--muted);
  }
  .mono {
    font-family: var(--mono);
  }
  .empty {
    margin: auto;
    color: var(--muted);
    font-family: var(--prose);
    font-size: var(--fs-lg);
  }
</style>
