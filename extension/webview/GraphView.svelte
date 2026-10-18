<script lang="ts">
  import { onMount } from 'svelte'
  import type { Detail, FileChange, Ref, Refs, Row } from '../../src/lib/api'
  import Graph from '../../src/lib/Graph.svelte'
  import Icon from '../../src/lib/Icon.svelte'
  import { graphKey } from '../../src/lib/graph-key'
  import { errorText, t } from '../../src/lib/i18n.svelte'
  import { invoke } from './host'

  type Repository = { tab: number; root: string; name: string; refs: Refs; scope: string }
  let repos = $state.raw<Repository[]>([])
  let tab = $state<number>()
  let repo = $state.raw<Repository>()
  let count = $state(0)
  let version = $state(0)
  let selectedRow = $state<number | null>(null)
  let loading = $state(true)
  let error = $state('')
  let scope = $state('auto')
  let busy = $state(false)
  const head = $derived(repo?.refs.head?.replace(/^refs\/heads\//, '') ?? repo?.refs.head_id?.slice(0, 7) ?? '')
  const short = (name: string) => name.replace(/^refs\/(heads|remotes)\//, '')
  /** 多选时范围是换行分隔的引用名 */
  const scoped = $derived(scope.split('\n'))
  /** 「相比」的基准在范围里是以 ^ 开头的一行 */
  const versus = $derived(scoped.find((name) => name.startsWith('^'))?.slice(1))
  const scopeLabel = $derived(scope === 'auto' ? t.vscode.graphAuto : scope === 'all' ? t.vscode.graphAll : versus ? t.compareBranch.label(short(scoped[0]), short(versus)) : scoped.length > 1 ? t.vscode.graphBranches(scoped.length) : short(scope))
  /** 和完整提交图共用的显示开关：隐藏标签只影响标记；只看主线要重新加载图 */
  const view = $state<{ hideTags: boolean; firstParent: boolean }>({ hideTags: false, firstParent: false, ...(() => { try { return JSON.parse(localStorage.getItem('graphView') ?? '{}') } catch { return {} } })() })
  function toggle(key: 'hideTags' | 'firstParent') {
    view[key] = !view[key]
    localStorage.setItem('graphView', JSON.stringify(view))
    if (key === 'firstParent') refresh()
  }
  let loadedKey = ''
  let running = false
  let again = false
  let disposed = false
  const badges = $derived.by(() => {
    const result = new Map<string, Ref[]>()
    for (const ref of repo?.refs.refs ?? []) if (!view.hideTags || !ref.name.startsWith('refs/tags/')) result.set(ref.id, [...(result.get(ref.id) ?? []), ref])
    return result
  })

  // 一次只加载一份图；切仓库或刷新时合并请求，避免旧加载覆盖引擎里的新图。
  async function refresh() {
    again = true
    if (running) return
    running = true
    do {
      again = false
      try {
        repos = await invoke<Repository[]>('graph_repos')
        const next = repos.find((r) => r.tab === tab) ?? repos[0]
        if (!next || disposed) { repo = undefined; count = 0; break }
        tab = next.tab
        scope = next.scope
        // Auto 随 HEAD/上游切换；所有引用的提交集合没变，也可能需要换一份子图。
        const current = scope === 'auto' ? `${next.refs.head}:${next.refs.refs.find((r) => r.name === next.refs.head)?.upstream}` : ''
        const key = `${next.tab}:${scope}:${view.firstParent}:${current}:${graphKey(next.refs)}`
        // 同一个仓库重载时留着旧图直接换成完整的新图：先清空或先退回首屏都会闪一下、丢滚动位置。
        const first = next.tab !== repo?.tab || !count
        repo = next
        if (key === loadedKey) continue
        loading = true
        error = ''
        selectedRow = null
        if (first) {
          count = 0
          count = await invoke<number>('load_graph', { tab: next.tab, full: false, firstParent: view.firstParent })
          version++
        }
        if (!first || count >= 2000) {
          count = await invoke<number>('load_graph', { tab: next.tab, full: true, firstParent: view.firstParent })
          version++
        }
        loadedKey = key
      } catch (e) {
        error = errorText(String(e))
      } finally {
        loading = false
      }
    } while (again && !disposed)
    running = false
  }

  async function changeScope() {
    busy = true
    try { await invoke('graph_scope', { tab: repo!.tab }); await refresh() }
    catch (e) { error = errorText(String(e)) }
    finally { busy = false }
  }
  async function sync(action: 'fetch' | 'pull' | 'push') {
    busy = true
    try { await invoke('graph_sync', { tab: repo!.tab, action }); await refresh() }
    catch (e) { error = errorText(String(e)) }
    finally { busy = false }
  }

  async function fetchFiles(id: string) {
    try { return (await invoke<Detail>('detail', { tab: repo!.tab, id })).files }
    catch (e) { throw errorText(String(e)) }
  }
  async function openFile(id: string, file: FileChange, preview: boolean) {
    try { await invoke('open_commit_file', { tab: repo!.tab, id, path: file.path, preview }) }
    catch (e) { error = errorText(String(e)) }
  }
  async function fetchRows(start: number, count: number) {
    try { return await invoke<Row[]>('rows', { tab: repo!.tab, start, count }) }
    catch (e) { loadedKey = ''; error = errorText(String(e)); return [] }
  }
  onMount(() => {
    refresh()
    addEventListener('focus', refresh)
    return () => { disposed = true; removeEventListener('focus', refresh) }
  })
</script>

<main>
  <div class="toolbar">
    <select aria-label={t.vscode.graphRepository} bind:value={tab} onchange={refresh} title={repo?.root} disabled={busy}>
      {#each repos as r (r.tab)}<option value={r.tab}>{r.name}</option>{/each}
    </select>
    <span title={t.vscode.graphCurrentBranch(head)}>{head}</span>
    <button class="action" class:on={!view.hideTags} aria-pressed={!view.hideTags} title={t.graphView.tagsHint} onclick={() => toggle('hideTags')}>{t.graphView.tags}</button>
    <button class="action" class:on={view.firstParent} aria-pressed={view.firstParent} disabled={loading} title={t.graphView.firstParentHint} onclick={() => toggle('firstParent')}>{t.graphView.firstParent}</button>
  </div>
  <div class="toolbar">
    <button class="scope" aria-label={t.vscode.graphScope} onclick={changeScope} disabled={!repo || busy || loading} title={scope === 'auto' ? t.vscode.graphAutoHint : scoped.map((name) => (name.startsWith('^') ? t.compareBranch.against(short(name.slice(1))) : short(name))).join('\n')}><span>{scopeLabel}</span><Icon name="down" size={12} /></button>
    <button class="action" disabled={!repo || busy} title={`${t.fetch} · ${repo?.name ?? ''}`} aria-label={t.fetch} onclick={() => sync('fetch')}><Icon name="fetch" size={14} /></button>
    <button class="action" disabled={!repo || busy || !repo.refs.head} title={`${t.pull} · ${head}`} aria-label={t.pull} onclick={() => sync('pull')}><Icon name="pull" size={14} /><small>{repo?.refs.ahead_behind?.[1] || ''}</small></button>
    <button class="action" disabled={!repo || busy || !repo.refs.head} title={t.vscode.graphPushSame(head)} aria-label={t.vscode.graphPushSame(head)} onclick={() => sync('push')}><Icon name="push" size={14} /><small>{repo?.refs.ahead_behind?.[0] || ''}</small></button>
  </div>
  {#if error}<p role="alert">{error} <button onclick={refresh}>{t.vscode.graphRetry}</button></p>{/if}
  {#if loading && !count}<p role="status">{t.loadingHistory}</p>{/if}
  {#if repo && count}
    {#key repo.tab}
      <Graph compact {fetchRows} {fetchFiles} onfile={openFile} {count} {version} {badges} headId={repo.refs.head_id} {selectedRow} onselect={(row) => selectedRow = row} onmenu={(e) => e.preventDefault()} />
    {/key}
  {:else if !loading && !error}
    <p>{repo ? t.noCommits : t.vscode.noRepo}</p>
  {/if}
</main>

<style>
  main {
    height: 100vh;
    display: flex;
    flex-direction: column;
    --bg: var(--vscode-sideBar-background);
    --text: var(--vscode-foreground);
    --muted: var(--vscode-descriptionForeground);
    --hover: var(--vscode-list-hoverBackground);
    --accent: var(--vscode-focusBorder);
    --accent-soft: var(--vscode-list-inactiveSelectionBackground);
    --ref-branch: var(--vscode-textLink-foreground);
    --ref-bg: color-mix(in srgb, var(--vscode-textLink-foreground) 28%, transparent);
    --border-strong: var(--vscode-contrastBorder, var(--vscode-widget-border, var(--vscode-descriptionForeground)));
    --fs-md: 12px;
    --fs-sm: 11px;
    --font: var(--vscode-font-family);
    background: var(--bg);
    color: var(--text);
    font-family: var(--font);
    font-size: var(--fs-md);
  }
  .toolbar { display: flex; gap: 4px; align-items: center; padding: 2px 4px; }
  select, .scope { min-width: 0; flex: 1; height: 22px; padding: 0 2px; font: inherit; background: var(--vscode-dropdown-background); color: var(--vscode-dropdown-foreground); border: 1px solid var(--vscode-dropdown-border); }
  .scope { display: flex; align-items: center; gap: 4px; padding: 0 6px; text-align: left; cursor: pointer; }
  .toolbar .scope span { flex: 1; min-width: 0; max-width: none; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: inherit; color: inherit; }
  .scope:disabled { opacity: .45; cursor: default; }
  .toolbar span { max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-sm); color: var(--muted); }
  .action { display: inline-flex; align-items: center; justify-content: center; height: 22px; min-width: 24px; padding: 0 4px; border: 0; border-radius: 3px; color: var(--text); background: transparent; cursor: pointer; }
  .action:hover { background: var(--hover); }
  .action.on { background: var(--accent-soft); }
  .action { flex: none; font: inherit; font-size: var(--fs-sm); white-space: nowrap; }
  .action:disabled { opacity: .45; cursor: default; }
  .action small { font-size: 10px; margin-left: 2px; }
  select:focus-visible, button:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }
  p { margin: 8px; font-size: 12px; }
</style>
