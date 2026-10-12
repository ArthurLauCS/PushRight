<script lang="ts">
  import type { Snippet } from 'svelte'
  import * as api from './api'
  import { authorColor } from './author'
  import Icon from './Icon.svelte'
  import { errorText, fmtTime, t } from './i18n.svelte'

  /** 提交图上方的搜索条：按说明、作者、文件、改动内容或提交号找提交，也用来显示单个文件的历史。 */
  let { tab, onjump, children }: { tab: number; onjump: (id: string) => void; /** 搜索栏右端的附加按钮 */ children?: Snippet } = $props()

  const KINDS: api.SearchKind[] = ['message', 'author', 'file', 'content', 'id']
  const PAGE = 200
  let kind = $state<api.SearchKind>('message')
  let query = $state('')
  /** null 表示没有在显示结果 */
  let results = $state.raw<api.HistoryCommit[] | null>(null)
  let more = $state(false)
  let loading = $state(false)
  let error = $state('')
  /** 结果是某个文件的历史（跟随重命名）时，这里是它的路径 */
  let history = $state('')
  let request = 0

  async function load(append: boolean) {
    const id = ++request
    const skip = append ? results!.length : 0
    loading = true
    error = ''
    try {
      // 多取一条用来判断还有没有下一页
      const next = history ? await api.fileHistory(tab, history, PAGE + 1, skip) : await api.search(tab, kind, query.trim(), PAGE + 1, skip)
      if (id !== request) return
      more = next.length > PAGE
      results = [...(append ? results! : []), ...next.slice(0, PAGE)]
    } catch (e) {
      if (id === request) error = errorText(e)
    } finally {
      if (id === request) loading = false
    }
  }

  export function fileHistory(path: string) {
    history = path
    query = path
    load(false)
  }

  function submit(e: Event) {
    e.preventDefault()
    history = ''
    if (query.trim()) load(false)
  }

  function close() {
    ++request
    results = null
    more = false
    loading = false
    error = ''
    history = ''
    query = ''
  }
</script>

<div class="search">
  <form onsubmit={submit}>
    <Icon name="search" size={14} />
    <select class="field" aria-label={t.vscode.searchTitle} bind:value={kind}>
      {#each KINDS as k (k)}<option value={k}>{t.vscode.searchKinds[k]}</option>{/each}
    </select>
    <input class="field" type="search" placeholder={t.search.placeholder} aria-label={t.search.placeholder} bind:value={query} />
    {#if results || error}<button type="button" class="btn small quiet icon" title={t.search.close} aria-label={t.search.close} onclick={close}><Icon name="close" size={14} /></button>{/if}
    {@render children?.()}
  </form>
  {#if error}<p class="note">{error}</p>{/if}
  {#if results}
    <div class="results">
      <p class="note">{history ? t.search.fileHistory(history) + ' · ' : ''}{results.length ? t.search.results(results.length) + (more ? '+' : '') : loading ? t.working : t.search.none}</p>
      {#each results as c (c.id)}
        <button class="result" title={c.subject} onclick={() => onjump(c.id)}>
          <code>{c.id.slice(0, 8)}</code>
          <span class="subject">{c.subject}</span>
          <span class="author" style:color={authorColor(c.author)}>{c.author}</span>
          <span class="time">{fmtTime(c.time)}</span>
        </button>
      {/each}
      {#if more}<button class="btn small" disabled={loading} onclick={() => load(true)}>{t.tools.loadMore}</button>{/if}
    </div>
  {/if}
</div>

<style>
  .search {
    flex: none;
    display: flex;
    flex-direction: column;
    max-height: 45%;
    border-bottom: 1px solid var(--border);
    font-size: var(--fs-sm);
  }
  form {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    color: var(--muted);
  }
  select {
    flex: none;
    width: auto;
    max-width: 40%;
    height: 24px;
  }
  input {
    flex: 1;
    min-width: 0;
    height: 24px;
    user-select: text;
  }
  .results {
    min-height: 0;
    overflow-y: auto;
    border-top: 1px solid var(--border);
    background: var(--panel);
  }
  .note {
    margin: 0;
    padding: 4px 12px;
    color: var(--muted);
  }
  .result {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    height: 24px;
    padding: 0 12px;
    border: 0;
    background: none;
    font-size: var(--fs-sm);
    text-align: left;
    cursor: default;
  }
  .result:hover {
    background: var(--hover);
  }
  .result code {
    flex: none;
    color: var(--muted);
  }
  .subject {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .author,
  .time {
    flex: none;
    white-space: nowrap;
  }
  .time {
    color: var(--muted);
  }
  .results > .btn {
    margin: 4px 12px 8px;
  }
</style>
