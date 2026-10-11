<script lang="ts">
  import { tick } from 'svelte'
  import { isTauri } from '@tauri-apps/api/core'
  import { loadAutoSync } from './lib/auto-sync'
  import { open } from '@tauri-apps/plugin-dialog'
  import * as api from './lib/api'
  import Icon from './lib/Icon.svelte'
  import Label from './lib/Label.svelte'
  import RepoView from './lib/RepoView.svelte'
  import Dialog from './lib/Dialog.svelte'
  import DiffColorSettings from './lib/DiffColorSettings.svelte'
  import { setDiffColors } from './lib/diff-colors.svelte'
  import { theme, toggleTheme } from './lib/theme.svelte'
  import { t, locale, setLocale, errorText, type Locale } from './lib/i18n.svelte'

  interface Tab {
    id: number
    path: string
    count: number
  }

  const load = (key: string): string[] => JSON.parse(localStorage.getItem(key) ?? '[]')

  let recent = $state(load('recent'))
  let tabs = $state<Tab[]>([])
  /** 当前页签 id；null 时显示打开仓库页 */
  let active = $state<number | null>(null)
  let error = $state('')
  let dialog = $state<Dialog>()
  let creating = $state(false)
  const desktop = isTauri()
  let autoSync = $state(loadAutoSync(localStorage))

  async function autoSyncSettings() {
    const values = await dialog!.ask({ title: t.autoSyncSettings, message: t.autoSyncHint, fields: [
      { key: 'refresh', label: t.autoRefresh, type: 'checkbox', value: autoSync.refresh },
      { key: 'fetch', label: t.autoFetch, type: 'checkbox', value: autoSync.fetch },
    ] })
    if (!values) return
    autoSync = { refresh: !!values.refresh, fetch: !!values.fetch }
    localStorage.setItem('autoSync', JSON.stringify(autoSync))
  }

  async function create(clone: boolean) {
    const dir = await open({ directory: true, title: clone ? t.tools.cloneParent : t.tools.initRepo })
    if (!dir) return
    const values = await dialog!.ask({ title: clone ? t.tools.cloneRepo : t.tools.initRepo, message: dir, fields: clone ? [
      { key: 'url', label: t.tools.url, type: 'text' },
      { key: 'name', label: t.tools.folderName, type: 'text' },
    ] : [] })
    if (!values) return
    if (clone && /[\\/:*?"<>|]|^\.{1,2}$/.test(String(values.name))) { error = t.errors.PR_INVALID_NAME; return }
    creating = true
    error = ''
    try { await openRepo(await api.createRepo(clone ? `${dir}/${values.name}` : dir, clone ? String(values.url) : null)) }
    catch (e) { error = String(e) }
    finally { creating = false }
  }

  const name = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path
  const save = () => localStorage.setItem('tabs', JSON.stringify(tabs.map((x) => x.path)))

  async function openRepo(path: string) {
    error = ''
    try {
      const [id, real, count] = await api.openRepo(path)
      const existing = tabs.find((x) => x.path === real)
      if (existing) {
        api.closeRepo(id)
        active = existing.id
        return
      }
      tabs.push({ id, path: real, count })
      active = id
      save()
      recent = [real, ...recent.filter((p) => p !== real)].slice(0, 10)
      localStorage.setItem('recent', JSON.stringify(recent))
    } catch (e) {
      error = String(e)
    }
  }

  async function pick() {
    const dir = await open({ directory: true, title: t.openRepo })
    if (dir) openRepo(dir)
  }

  function close(id: number) {
    const i = tabs.findIndex((x) => x.id === id)
    tabs.splice(i, 1)
    api.closeRepo(id)
    if (active === id) active = (tabs[i] ?? tabs[i - 1])?.id ?? null
    save()
  }

  function onkeydown(e: KeyboardEvent) {
    if (!e.ctrlKey) return
    if (e.key === 'Tab' && tabs.length) {
      e.preventDefault()
      const i = tabs.findIndex((x) => x.id === active)
      active = tabs[(i + (e.shiftKey ? tabs.length - 1 : 1)) % tabs.length].id
    } else if (e.key === 'w' && active !== null) {
      e.preventDefault()
      close(active)
    } else if (e.key === 't') {
      e.preventDefault()
      active = null
    }
  }

  // 恢复上次的页签，再打开命令行传入的仓库
  ;(async () => {
    for (const path of [...load('tabs'), ...(await api.initialRepos())]) await openRepo(path)
  })()

  $effect(() => {
    const reveal = async (event: Event) => {
      const { id, root } = (event as CustomEvent<{ id: string; root?: string }>).detail
      if (root) {
        const normalize = (p: string) => p.replaceAll('\\', '/').toLowerCase()
        const tab = tabs.find((tab) => normalize(tab.path) === normalize(root))
        if (tab) active = tab.id
        else await openRepo(root)
      }
      await tick()
      dispatchEvent(new CustomEvent('pushright:jump', { detail: id }))
    }
    addEventListener('pushright:reveal', reveal)
    return () => removeEventListener('pushright:reveal', reveal)
  })
</script>

<svelte:window {onkeydown} />

  <nav>
    {#each tabs as tab (tab.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div
        class="tab"
        class:active={tab.id === active}
        title={tab.path}
        onclick={() => (active = tab.id)}
        onauxclick={(e) => e.button === 1 && close(tab.id)}
      >
        <span>{name(tab.path)}</span>
        <button title={t.closeTab} onclick={(e) => (e.stopPropagation(), close(tab.id))}><Icon name="close" size={12} /></button>
      </div>
    {/each}
    <button class="btn quiet icon" class:on={active === null} title={t.newTab} onclick={() => (active = null)}><Icon name="plus" /></button>
    <span class="spacer"></span>
    <DiffColorSettings editor={!desktop} onsave={async (colors) => {
      if (!desktop) await api.saveDiffColors(colors)
      setDiffColors(colors)
    }} />
    {#if desktop}<button class="btn quiet icon" title={t.autoSyncSettings} onclick={autoSyncSettings}><Icon name="sync" /></button>{/if}
    <select class="field language" aria-label={t.language} title={t.language} value={locale.current} onchange={(e) => setLocale(e.currentTarget.value as Locale)}>
      <option value="en">English</option>
      <option value="zh-CN">简体中文</option>
    </select>
    <button class="btn quiet icon" title={theme.current === 'dark' ? t.themeToLight : t.themeToDark} onclick={toggleTheme}>
      <Icon name={theme.current === 'dark' ? 'sun' : 'moon'} />
    </button>
  </nav>

<div class="body">
  {#each tabs as tab (tab.id)}
    <RepoView tab={tab.id} path={tab.path} initialCount={tab.count} active={tab.id === active} onopen={openRepo} autoSync={desktop ? autoSync : undefined} />
  {/each}

  {#if active === null}
    <div class="welcome">
      <h1>{t.appName}</h1>
      <p class="lead">{t.welcomeLead}</p>
      <p class="muted">{t.openRepoHint}</p>
      <div class="start">
        <button class="btn primary" disabled={creating} onclick={pick}>{t.openRepo}</button>
        <button class="btn" disabled={creating} onclick={() => create(true)}><Label text={t.tools.cloneRepo} /></button>
        <button class="btn" disabled={creating} onclick={() => create(false)}><Label text={t.tools.initRepo} /></button>
      </div>
      {#if creating}<p>{t.working}</p>{/if}
      {#if error}<p class="error">{errorText(error)}</p>{/if}
      {#if recent.length}
        <h2>{t.recent}</h2>
        {#each recent as path (path)}
          <button class="recent" onclick={() => openRepo(path)}>{path}</button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<Dialog bind:this={dialog} />

<style>
  :global(#app) {
    display: flex;
    flex-direction: column;
  }
  nav {
    flex: none;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 40px;
    padding: 0 8px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }
  /* :global：差异配色的按钮在子组件里 */
  nav > :global(.btn) {
    margin-bottom: 4px;
  }
  .language {
    width: auto;
    height: 28px;
    margin: 0 8px 4px;
    padding: 0 6px;
  }
  .spacer {
    flex: 1;
  }
  .body {
    flex: 1;
    min-height: 0;
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    min-width: 0;
    max-width: 220px;
    padding: 0 8px 0 16px;
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: var(--r-md) var(--r-md) 0 0;
    margin-bottom: -1px;
    color: var(--muted);
    font-family: var(--display);
    cursor: default;
  }
  .tab:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tab.active {
    background: var(--panel);
    border-color: var(--border);
    color: var(--text);
    font-weight: 700;
  }
  .tab span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tab button {
    flex: none;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    color: var(--muted);
    visibility: hidden;
    cursor: pointer;
  }
  .tab:hover button,
  .tab.active button {
    visibility: visible;
  }
  .tab button:hover {
    background: var(--raised);
    color: var(--text);
  }
  .welcome {
    margin: auto;
    width: 520px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
  h1 {
    margin: 0;
    color: var(--accent);
    font-size: 48px;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .lead {
    margin: 0 0 16px;
    font-family: var(--prose);
    font-size: var(--fs-xl);
  }
  .start {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .start .btn {
    height: 40px;
    padding: 0 16px;
  }
  .start .btn.primary {
    padding: 0 24px;
  }
  h2 {
    margin: 32px 0 4px;
    font-size: var(--fs-sm);
    font-weight: 400;
    color: var(--muted);
    letter-spacing: 0.04em;
  }
  .muted {
    margin: 0 0 16px;
    color: var(--muted);
  }
  .recent {
    width: 100%;
    padding: 8px 12px;
    margin-left: -12px;
    border: 0;
    border-radius: var(--r-md);
    background: none;
    font-family: var(--mono);
    font-size: var(--fs-sm);
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .recent:hover {
    background: var(--hover);
  }
  .error {
    margin: 0;
    color: var(--red);
    user-select: text;
  }
</style>
