<script lang="ts">
  import * as api from './api'
  import Label from './Label.svelte'
  import RevisionPicker from './RevisionPicker.svelte'
  import { t, errorText } from './i18n.svelte'

  let { tab, path, refs, disabled, section, run, onopen }: {
    tab: number; path: string; refs: api.Refs; disabled: boolean
    section: 'bisect' | 'submodules' | 'lfs' | 'patch' | 'stats'
    /** 执行一条写操作并刷新界面，返回它的命令日志 */
    run: (op: api.Op) => Promise<api.Log | undefined>
    onopen: (path: string) => void
  } = $props()

  const r = $derived(t.repoTools)
  let error = $state('')
  /** 最近一次操作的输出：二分查找每一步的结论、补丁失败的原因都在里面 */
  let last = $state.raw<api.Log | null>(null)

  let bisecting = $state<string | null>(null)
  /** 查找结束后 Git 在记录里写下的结论行 */
  const found = $derived(bisecting?.match(/^# first '?bad'? commit: \[(\w+)\] (.*)$/m))
  let bad = $state('HEAD')
  let good = $state('')
  let modules = $state.raw<api.Submodule[]>([])
  let lfs = $state.raw<api.Lfs | null>(null)
  let stats = $state.raw<api.Stats | null>(null)
  const peak = $derived(stats?.months.reduce((a, b) => (b.count > a.count ? b : a)))
  /** null 表示还没查过 */
  let locks = $state<string | null>(null)
  let pattern = $state('')
  let lockPath = $state('')
  let patch = $state('')
  let index = $state(false)

  async function load() {
    error = ''
    try {
      if (section === 'bisect') bisecting = await api.bisectLog(tab)
      if (section === 'submodules') modules = await api.submodules(tab)
      if (section === 'lfs') lfs = await api.lfs(tab)
      if (section === 'stats') stats = await api.stats(tab)
    } catch (e) { error = errorText(e) }
  }
  $effect(() => { section; last = null; load() })

  async function exec(op: api.Op) {
    last = (await run(op)) ?? null
    await load()
    return !!last?.ok
  }
  async function showLocks() {
    error = ''
    try { locks = await api.lfsLocks(tab) } catch (e) { error = errorText(e) }
  }
</script>

{#if error}<p class="error">{error}</p>{/if}
{#if section === 'patch'}
  <p>{r.patchHint}</p>
  <label>{r.patchText}<textarea class="field" spellcheck="false" bind:value={patch} {disabled}></textarea></label>
  <label class="check"><input type="checkbox" bind:checked={index} /><span><Label text={r.patchIndex} /></span></label>
  <button class="btn primary" disabled={disabled || !patch.trim()} onclick={async () => { if (await exec({ op: 'apply_patch', text: patch, index })) patch = '' }}><Label text={r.patchApply} /></button>
  {#if last?.ok}<p class="done">{r.patchDone}</p>{/if}
{:else if section === 'bisect'}
  <p>{r.bisectHint}</p>
  {#if bisecting === null}
    <form onsubmit={(e) => { e.preventDefault(); exec({ op: 'bisect_start', bad, good }) }}>
      <RevisionPicker {refs} label={r.bisectBad} bind:value={bad} />
      <RevisionPicker {refs} label={r.bisectGood} bind:value={good} />
      <button class="btn primary" disabled={disabled || !bad || !good || !!refs.in_progress}><Label text={r.bisectStart} /></button>
    </form>
  {:else if found}
    <p class="done">{r.bisectFound(found[1].slice(0, 12), found[2])}</p>
    <button class="btn primary" {disabled} onclick={() => exec({ op: 'bisect_reset' })}><Label text={r.bisectReset} /></button>
  {:else}
    <p class="now">{r.bisectNow((refs.head_id ?? '').slice(0, 12))}</p>
    <div class="actions">
      <button class="btn" {disabled} onclick={() => exec({ op: 'bisect_mark', verdict: 'good' })}><Label text={r.bisectMarkGood} /></button>
      <button class="btn" {disabled} onclick={() => exec({ op: 'bisect_mark', verdict: 'bad' })}><Label text={r.bisectMarkBad} /></button>
      <button class="btn quiet" {disabled} onclick={() => exec({ op: 'bisect_mark', verdict: 'skip' })}><Label text={r.bisectSkip} /></button>
      <button class="btn danger" {disabled} onclick={() => exec({ op: 'bisect_reset' })}><Label text={r.bisectReset} /></button>
    </div>
  {/if}
  {#if bisecting}<details><summary>{r.bisectLog}</summary><pre>{bisecting}</pre></details>{/if}
{:else if section === 'submodules'}
  <p>{r.submoduleHint}</p>
  <div class="actions">
    <button class="btn" disabled={disabled || !modules.length} onclick={() => exec({ op: 'submodule_update' })}><Label text={r.submoduleUpdate} /></button>
    <button class="btn" disabled={disabled || !modules.length} onclick={() => exec({ op: 'submodule_sync' })}><Label text={r.submoduleSync} /></button>
  </div>
  <div class="table">
    {#each modules as m (m.path)}
      <div class="row"><b>{m.path}</b><code title={m.id}>{m.id.slice(0, 12)}</code><span class:warn={m.state !== 'ok'}>{r.submoduleStates[m.state]}</span>
        {#if m.state !== 'missing'}<button class="btn small" onclick={() => onopen(`${path}/${m.path}`)}>{r.submoduleOpen}</button>{/if}
      </div>
    {:else}<p class="none">{r.submoduleNone}</p>{/each}
  </div>
{:else if section === 'stats'}
  <p>{r.statsHint}</p>
  {#if stats && peak}
    <div class="tiles">
      <div><b>{stats.commits.toLocaleString()}</b>{r.statsCommits}</div>
      <div><b>{stats.authors.length}</b>{r.statsAuthors}</div>
      <div><b>{stats.first}</b>{r.statsFirst}</div>
      <div><b>{stats.last}</b>{r.statsLast}</div>
    </div>
    <h3>{r.statsMonths}</h3>
    <div class="months">{#each stats.months as m (m.name)}<i style:height="{(m.count / peak.count) * 100}%" title={r.statsMonth(m.name, m.count)}></i>{/each}</div>
    <div class="axis"><span>{stats.months[0].name}</span><span>{r.statsPeak(peak.name, peak.count)}</span><span>{stats.months.at(-1)!.name}</span></div>
    {#each [[r.statsByAuthor, stats.authors], [r.statsByFile, stats.files]] as const as [title, list]}
      <h3>{title}</h3>
      <div class="table ranked">
        {#each list as c (c.name)}
          <div class="row" style:--w="{(c.count / list[0].count) * 100}%"><b title={c.name}>{c.name}</b><code>{c.count.toLocaleString()}</code></div>
        {/each}
      </div>
    {/each}
  {:else if stats}<p class="none">{r.statsNone}</p>{/if}
{:else if lfs}
  <p>{r.lfsHint}</p>
  {#if !lfs.version}
    <p class="error">{r.lfsMissing}</p>
  {:else}
    <div class="actions">
      <span class:warn={!lfs.installed}>{lfs.installed ? r.lfsInstalled : r.lfsNotInstalled}</span><small>{lfs.version}</small>
      {#if !lfs.installed}<button class="btn primary" {disabled} onclick={() => exec({ op: 'lfs_install' })}><Label text={r.lfsInstall} /></button>{/if}
      <button class="btn" {disabled} onclick={() => exec({ op: 'lfs_pull' })}><Label text={r.lfsPull} /></button>
    </div>
    <h3>{r.lfsPatterns}</h3>
    <div class="table">
      {#each lfs.patterns as p (p)}
        <div class="row"><b>{p}</b><span></span><button class="btn small" {disabled} onclick={() => exec({ op: 'lfs_untrack', pattern: p })}><Label text={r.lfsUntrack} /></button></div>
      {:else}<p class="none">{r.lfsNoPatterns}</p>{/each}
    </div>
    <form onsubmit={async (e) => { e.preventDefault(); if (await exec({ op: 'lfs_track', pattern })) pattern = '' }}>
      <label>{r.lfsPattern}<input class="field" bind:value={pattern} /></label>
      <button class="btn" disabled={disabled || !pattern.trim()}><Label text={r.lfsTrack} /></button>
    </form>
    <p>{r.lfsTrackNote}</p>
    <form onsubmit={(e) => e.preventDefault()}>
      <label>{r.lfsLockPath}<input class="field" bind:value={lockPath} /></label>
      <button class="btn" disabled={disabled || !lockPath.trim()} onclick={() => exec({ op: 'lfs_lock', path: lockPath })}><Label text={r.lfsLock} /></button>
      <button class="btn" disabled={disabled || !lockPath.trim()} onclick={() => exec({ op: 'lfs_unlock', path: lockPath })}><Label text={r.lfsUnlock} /></button>
      <button class="btn" {disabled} onclick={showLocks}><Label text={r.lfsLocks} /></button>
    </form>
    {#if locks !== null}<pre>{locks || r.lfsNoLocks}</pre>{/if}
  {/if}
{/if}
{#if last && (section !== 'patch' || !last.ok) && last.output}<pre class:error={!last.ok}>{last.output}</pre>{/if}

<style>
  /* 页面留白和纵向排布由外层 .toolbox 负责，这里只管各段内部 */
  :global(.toolbox) > :not(.btn) { align-self: stretch; }
  .actions, form { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  form { align-items: flex-end; }
  form label { flex: 1; min-width: 200px; }
  label { display: flex; flex-direction: column; gap: 4px; color: var(--muted); }
  .check { flex: none; flex-direction: row; align-items: center; gap: 8px; height: 28px; }
  p, h3 { margin: 0; }
  p { max-width: 72ch; color: var(--muted); line-height: var(--lh-body); }
  h3 { font-size: var(--fs-md); }
  p.now { color: var(--fg); }
  p.done { color: var(--green); }
  .error { color: var(--red); white-space: pre-wrap; }
  .warn { color: var(--yellow, var(--accent)); }
  .table { background: var(--panel); border: 1px solid var(--border); border-radius: var(--r-md); overflow: hidden; }
  .row { display: flex; gap: 12px; align-items: center; min-height: 36px; padding: 4px 8px 4px 16px; border-top: 1px solid var(--border); font-size: var(--fs-sm); }
  .row:first-child { border-top: 0; }
  .row b { font-family: var(--mono); font-weight: 400; }
  .row code { color: var(--muted); }
  .row span { flex: 1; min-width: 0; color: var(--muted); }
  .row span.warn { color: var(--yellow, var(--accent)); }
  .none { max-width: none; padding: 24px; text-align: center; }
  textarea { min-height: 220px; font-family: var(--mono); font-size: var(--fs-sm); white-space: pre; }
  pre { margin: 0; padding: 12px 16px; max-height: 320px; overflow: auto; background: var(--panel); border: 1px solid var(--border); border-radius: var(--r-md); font-size: var(--fs-sm); white-space: pre-wrap; user-select: text; }
  .tiles { display: flex; flex-wrap: wrap; gap: 8px; }
  .tiles div { display: flex; flex-direction: column; gap: 4px; min-width: 140px; padding: 12px 16px; background: var(--panel); border: 1px solid var(--border); border-radius: var(--r-md); color: var(--muted); font-size: var(--fs-sm); }
  .tiles b { font-size: var(--fs-lg); font-weight: 600; color: var(--fg); font-variant-numeric: tabular-nums; }
  /* 每月一根柱子：贴着基线、顶端圆角、柱间留 2px */
  .months { display: flex; align-items: flex-end; gap: 2px; height: 120px; border-bottom: 1px solid var(--border); }
  .months i { flex: 1; min-width: 1px; min-height: 1px; background: var(--accent); border-radius: 4px 4px 0 0; }
  .months i:hover { filter: brightness(1.25); }
  .axis { display: flex; justify-content: space-between; margin-top: -8px; color: var(--muted); font-size: var(--fs-sm); }
  /* 外层是会收缩子项的纵向 flex：图和表不让它压扁，由整页滚动 */
  .months, .ranked { flex: none; }
  .ranked { max-height: 420px; overflow: auto; }
  .ranked .row { min-height: 28px; background: linear-gradient(90deg, var(--accent-soft) var(--w), transparent var(--w)); }
  .ranked b { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ranked code { color: var(--fg); font-variant-numeric: tabular-nums; }
  summary { cursor: pointer; color: var(--muted); margin-bottom: 8px; }
</style>
