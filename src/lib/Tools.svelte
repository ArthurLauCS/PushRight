<script lang="ts">
  import { untrack } from 'svelte'
  import * as api from './api'
  import Detail from './Detail.svelte'
  import Icon from './Icon.svelte'
  import Label from './Label.svelte'
  import RevisionPicker from './RevisionPicker.svelte'
  import type { Spec, Values } from './Dialog.svelte'
  import { t, errorText } from './i18n.svelte'

  let { tab, path, refs, busy, left: initialLeft = 'HEAD~1', right: initialRight = 'HEAD', exec, rebase, ask }: {
    tab: number; path: string; refs: api.Refs; busy: boolean; left?: string; right?: string
    exec: (op: api.Op) => Promise<boolean>; rebase: (plan: api.RebasePlan) => Promise<boolean>
    ask: (spec: Spec) => Promise<Values | null>
  } = $props()
  let section = $state<'compare' | 'remotes' | 'rebase' | 'reflog'>('compare')
  let left = $state(untrack(() => initialLeft))
  let right = $state(untrack(() => initialRight))
  let commonBase = $state(false)
  let comparison = $state<api.Comparison | null>(null)
  let error = $state('')
  let loading = $state(false)
  let remotes = $state<api.Remote[]>([])
  let logs = $state<api.Reflog[]>([])
  let moreLogs = $state(false)
  let base = $state('HEAD~1')
  let plan = $state<api.RebasePlan | null>(null)
  const disabled = $derived(busy || loading)
  const compareDetail = $derived<api.Detail | null>(comparison ? {
    id: comparison.right, parents: [comparison.left], author: '', author_email: '', author_time: 0,
    committer: '', committer_time: 0, message: `${comparison.left.slice(0, 12)} → ${comparison.right.slice(0, 12)}`, files: comparison.files,
  } : null)

  async function run(action: () => Promise<unknown>) {
    loading = true
    error = ''
    try { await action() } catch (e) { error = errorText(e) } finally { loading = false }
  }
  async function choose(value: typeof section) {
    section = value
    if (value === 'remotes') await run(async () => { remotes = await api.remoteDetails(tab) })
    if (value === 'reflog') await loadLogs(false)
  }
  async function loadLogs(append: boolean) {
    await run(async () => {
      const next = await api.reflog(tab, append ? logs.length : 0)
      moreLogs = next.length > 200
      logs = [...(append ? logs : []), ...next.slice(0, 200)]
    })
  }
  async function remote(action: 'add' | 'fetch' | 'push' | 'rename' | 'remove', item?: api.Remote) {
    const title = action === 'add' ? t.tools.addRemote : action === 'remove' ? t.tools.deleteRemote : action === 'rename' ? t.tools.renameRemote : action === 'push' ? t.tools.editPush : t.tools.editFetch
    const values = await ask({ title, message: item?.name, warning: action === 'remove' ? t.tools.removeRemote : undefined, danger: action === 'remove', fields:
      action === 'remove' ? [] : action === 'rename' ? [{ key: 'name', label: t.tools.remoteName, type: 'text', value: item!.name }]
      : [...(action === 'add' ? [{ key: 'name', label: t.tools.remoteName, type: 'text' as const, value: 'origin' }] : []), { key: 'url', label: t.tools.url, type: 'text', value: item?.[action === 'push' ? 'push' : 'fetch'] ?? '' }],
    })
    if (!values) return
    await run(async () => {
      const op: api.Op = action === 'add' ? { op: 'remote_add', name: String(values.name), url: String(values.url) }
        : action === 'remove' ? { op: 'remote_remove', name: item!.name }
        : action === 'rename' ? { op: 'remote_rename', old: item!.name, new: String(values.name) }
        : { op: 'remote_url', name: item!.name, url: String(values.url), push: action === 'push' }
      await exec(op)
      remotes = await api.remoteDetails(tab)
    })
  }
  async function recover(entry: api.Reflog) {
    const values = await ask({ title: t.tools.recover, message: `${entry.id}\n${entry.subject}`, fields: [{ key: 'name', label: t.newName, type: 'text', value: `recovered/${entry.id.slice(0, 8)}` }] })
    if (values) await run(() => exec({ op: 'create_branch', name: String(values.name), start: entry.id, checkout: false }))
  }
  function move(index: number, delta: number) {
    if (!plan) return
    const next = [...plan.steps]
    ;[next[index], next[index + delta]] = [next[index + delta], next[index]]
    plan.steps = next
  }
  async function apply() {
    if (!plan || !await ask({ title: t.tools.applyPlan, warning: t.tools.rebaseWarning, danger: true })) return
    await run(async () => { if (await rebase($state.snapshot(plan!))) plan = null })
  }
</script>

<div class="toolbox">
  <nav aria-label={t.tools.nav}>
    {#each ['compare', 'remotes', 'rebase', 'reflog'] as key}
      <button class="btn" class:on={section === key} disabled={disabled} onclick={() => choose(key as typeof section)}><Label text={t.tools[key as typeof section]} /></button>
    {/each}
  </nav>
  {#if error}<p class="error">{error}</p>{/if}
  {#if section === 'compare'}
    <form onsubmit={(e) => { e.preventDefault(); run(async () => { comparison = await api.compare(tab, left, right, commonBase) }) }}>
      <RevisionPicker {refs} label={t.tools.left} bind:value={left} />
      <RevisionPicker {refs} label={t.tools.right} bind:value={right} />
      <label class="check"><input type="checkbox" bind:checked={commonBase} /><span><Label text={t.tools.commonBase} /></span></label>
      <button class="btn primary" disabled={disabled}><Label text={t.tools.compare} /></button>
    </form>
    {#if comparison}
      <div class="comparison">
        <Detail detail={compareDetail} comparison onjump={() => {}}
          reviewKey={`review:${JSON.stringify([path, comparison.left, comparison.right])}`}
          fetchDiff={(_, file) => api.diffBetween(tab, comparison!.left, comparison!.right, file, comparison!.files.find((f) => f.path === file)?.old_path ?? null)}
          fetchPage={(_, file, skip) => api.diffPage(tab, 'compare', comparison!.left, comparison!.right, file, skip, comparison!.files.find((f) => f.path === file)?.old_path ?? null)} />
      </div>
    {/if}
  {:else if section === 'remotes'}
    <button class="btn" disabled={disabled} onclick={() => remote('add')}><Label text={t.tools.addRemote} /></button>
    {#each remotes as item (item.name)}
      <article><h3>{item.name}</h3>
        <dl><dt>{t.tools.fetchUrl}</dt><dd>{item.fetch}</dd><dt>{t.tools.pushUrl}</dt><dd>{item.push}</dd></dl>
        <div class="actions">{#each ['fetch', 'push', 'rename', 'remove'] as action}
          <button class="btn small" class:danger={action === 'remove'} disabled={disabled} onclick={() => remote(action as 'fetch' | 'push' | 'rename' | 'remove', item)}><Label text={action === 'fetch' ? t.tools.editFetch : action === 'push' ? t.tools.editPush : action === 'rename' ? t.tools.renameRemote : t.tools.deleteRemote} /></button>
        {/each}</div>
      </article>
    {/each}
  {:else if section === 'reflog'}
    <button class="btn" disabled={disabled} onclick={() => loadLogs(false)}>{t.tools.refresh}</button>
    <div class="table">
      {#each logs as entry, i (i)}
        <div class="reflog"><code title={entry.id}>{entry.id.slice(0, 12)}</code><span title={entry.subject}><b>{entry.selector}</b>{entry.subject}</span><button class="btn small" disabled={disabled} onclick={() => recover(entry)}><Label text={t.tools.recover} /></button></div>
      {:else}<p class="none">{t.tools.noReflog}</p>{/each}
    </div>
    {#if moreLogs}<button class="btn" disabled={disabled} onclick={() => loadLogs(true)}>{t.tools.loadMore}</button>{/if}
  {:else}
    <p>{t.tools.rebaseWarning}</p>
    <form onsubmit={(e) => { e.preventDefault(); run(async () => { plan = await api.rebasePlan(tab, base) }) }}>
      <RevisionPicker {refs} label={t.tools.base} bind:value={base} />
      <button class="btn" disabled={disabled || !!refs.in_progress}><Label text={t.tools.loadPlan} /></button>
    </form>
    {#if plan}
      <p><code>{plan.base.slice(0, 12)} → {plan.head.slice(0, 12)}</code></p>
      {#each plan.steps as step, index (step.id)}
        <article>
          <div class="actions">
            <button class="btn small icon" title={t.tools.up} aria-label={t.tools.up} disabled={disabled || index === 0} onclick={() => move(index, -1)}><Icon name="up" size={14} /></button>
            <button class="btn small icon" title={t.tools.down} aria-label={t.tools.down} disabled={disabled || index === plan!.steps.length - 1} onclick={() => move(index, 1)}><Icon name="down" size={14} /></button>
            <select class="field" aria-label={t.tools.action} bind:value={step.action} disabled={disabled}>{#each ['pick', 'reword', 'edit', 'squash', 'fixup', 'drop'] as action}<option value={action}>{t.tools.rebaseActions[action]}</option>{/each}</select>
            <code>{step.id.slice(0, 8)}</code><span>{step.subject}</span>
          </div>
          {#if step.action === 'reword' || step.action === 'squash'}<label>{t.tools.rebaseMessage}<textarea class="field" bind:value={step.message} disabled={disabled}></textarea></label>{/if}
        </article>
      {/each}
      <button class="btn primary" disabled={disabled || !plan.steps.length || !!refs.in_progress || plan.head !== refs.head_id} onclick={apply}><Label text={t.tools.applyPlan} /></button>
    {/if}
  {/if}
</div>

<style>
  /* 和工作树总览同一套页面留白；按钮按内容定宽，不被纵向布局拉成整行 */
  .toolbox { flex: 1; min-height: 0; overflow: auto; padding: 24px 32px; display: flex; flex-direction: column; align-items: flex-start; gap: 16px; }
  .toolbox > :not(.btn, nav) { align-self: stretch; }
  nav, .actions, form { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  form { align-items: flex-end; }
  form label { flex: 1; min-width: 200px; }
  label { display: flex; flex-direction: column; gap: 4px; color: var(--muted); }
  .check { flex: none; flex-direction: row; align-items: center; gap: 8px; height: 28px; }
  article, .table { background: var(--panel); border: 1px solid var(--border); border-radius: var(--r-md); }
  article { padding: 12px 16px; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 4px 16px; margin: 0 0 12px; font-size: var(--fs-sm); }
  dt { color: var(--muted); }
  dd { margin: 0; font-family: var(--mono); overflow-wrap: anywhere; user-select: text; }
  /* Reflog 是一张表：行高和提交图一致，不再一条记录一张卡片 */
  .table { overflow: hidden; }
  .reflog { display: flex; gap: 12px; align-items: center; min-height: 36px; padding: 4px 8px 4px 16px; border-top: 1px solid var(--border); font-size: var(--fs-sm); }
  .reflog:first-child { border-top: 0; }
  .reflog:hover { background: var(--hover); }
  .reflog code { flex: none; color: var(--muted); }
  .reflog span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .reflog b { margin-right: 12px; font-family: var(--mono); font-weight: 400; color: var(--accent); }
  .none { padding: 24px; text-align: center; color: var(--muted); }
  h3, p { margin: 0 0 8px; }
  h3 { font-size: var(--fs-lg); }
  .toolbox > p:not(.error) { max-width: 72ch; color: var(--muted); line-height: var(--lh-body); }
  .error { color: var(--red); white-space: pre-wrap; }
  .comparison { flex: 1; min-height: 300px; display: flex; flex-direction: column; }
  .comparison :global(section) { height: 100% !important; flex: 1; min-height: 0; }
  .actions select { width: auto; }
  textarea { min-height: 64px; }
  code { user-select: text; }
</style>
