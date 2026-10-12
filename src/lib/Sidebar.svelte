<script lang="ts">
  import type { Refs, Stash, Track, Worktree } from './api'
  import { orderBranches } from './branch-order'
  import Icon from './Icon.svelte'
  import Label from './Label.svelte'
  import { layout } from './layout.svelte'
  import { t } from './i18n.svelte'

  type Kind = 'branch' | 'remote' | 'tag' | 'stash'
  type View = 'history' | 'changes' | 'worktrees' | 'tools'

  let {
    refs,
    stashes,
    worktrees,
    tracks,
    view,
    changes,
    onview,
    onjump,
    onactivate,
    onstash,
    onmenu,
    scope,
    onscope,
    autoFf,
    pinned,
    onpin,
    onclean,
    onworktree,
  }: {
    refs: Refs
    stashes: Stash[]
    worktrees: Worktree[]
    /** 各本地分支与上游的同步状态，键是完整引用名 */
    tracks: Map<string, Track>
    view: View
    /** 有改动的文件数 */
    changes: number
    onview: (view: View) => void
    onjump: (id: string) => void
    /** 双击：name 是引用全名或贮藏名 */
    onactivate: (kind: Kind, name: string) => void
    /** 单击贮藏：查看内容，不改变工作区。 */
    onstash: (name: string) => void
    onmenu: (e: MouseEvent, kind: Kind, name: string, id: string) => void
    /** 提交图只显示这些引用的提交；空表示全部 */
    scope: string[]
    /** 把一个引用加入或移出提交图筛选 */
    onscope: (name: string) => void
    /** 自动获取后会自动快进的本地分支（引用全名） */
    autoFf: string[]
    /** 固定在前列的分支（引用全名） */
    pinned: string[]
    onpin: (name: string) => void
    /** 打开「清理分支」 */
    onclean: () => void
    /** 双击工作树：在页签中打开 */
    onworktree: (path: string) => void
  } = $props()

  /** 在别的工作树里打开着的分支 */
  const elsewhere = $derived(new Set(worktrees.filter((w) => !w.current && w.branch).map((w) => w.branch)))

  let filter = $state('')
  /** 单击选中的引用或贮藏：只高亮，不改变列表顺序。 */
  let selected = $state('')

  const group = (prefix: string) =>
    refs.refs
      .filter((r) => r.name.startsWith(prefix) && r.name.toLowerCase().includes(filter.toLowerCase()))
      .map((r) => ({ ...r, label: r.name.slice(prefix.length) }))
  const branches = $derived(orderBranches(group('refs/heads/'), refs.head, pinned))
  const remotes = $derived(orderBranches(group('refs/remotes/'), null, pinned))
  /** 当前分支的上游短名，显示在它那一行里；没有上游时为空 */
  const upstream = $derived(refs.refs.find((r) => r.name === refs.head)?.upstream?.replace('refs/remotes/', ''))
  const tags = $derived(group('refs/tags/'))
  const basename = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p
</script>

<aside style:width="min({layout.sidebar}px, 36vw)">
  <nav>
    <button class:on={view === 'changes'} title={t.changes} onclick={() => onview('changes')}>
      <Icon name="changes" /><span><Label text={t.changes} /></span>{#if changes}<span class="pill">{changes}</span>{/if}
    </button>
    <button class:on={view === 'history'} title={t.history} onclick={() => onview('history')}><Icon name="history" /><span><Label text={t.history} /></span></button>
    <button class:on={view === 'worktrees'} title={t.worktreesNav} onclick={() => onview('worktrees')}>
      <Icon name="folders" /><span><Label text={t.worktreesNav} /></span>{#if worktrees.length > 1}<span class="pill quiet">{worktrees.length}</span>{/if}
    </button>
    <button class:on={view === 'tools'} onclick={() => onview('tools')}><Icon name="tools" /><span>{t.tools.nav}</span></button>
  </nav>
  <label class="search"><Icon name="search" size={14} /><input type="search" placeholder={t.filter} bind:value={filter} /></label>
  <div class="scroll">
    {#snippet section(kind: Kind, title: string, items: { name: string; id: string; label: string }[], open: boolean)}
      <details {open}>
        <summary>{title}<span class="count">{items.length}</span></summary>
        {#if kind === 'branch'}<button class="clean" title={t.branchList.clean} aria-label={t.branchList.clean} onclick={onclean}><Icon name="broom" size={12} /></button>{/if}
        {#each items as r (r.name)}
          <button
            class:head={r.name === refs.head}
            class:on={r.name === selected}
            title={r.label}
            onclick={() => { selected = r.name; onjump(r.id) }}
            ondblclick={() => onactivate(kind, r.name)}
            oncontextmenu={(e) => onmenu(e, kind, r.name, r.id)}
          >
            {#if r.name === refs.head}<span class="tick">✓</span>{/if}
            <span class="name">{r.label}</span>
            {#if r.name === refs.head && upstream}<span class="sub up" title={t.branchList.upstream(upstream)}>→ {upstream}</span>{/if}
            {#if elsewhere.has(r.name)}<span class="sub" title={t.inOtherWorktreeMark}><Icon name="windows" size={12} /></span>{/if}
            {#if autoFf.includes(r.name)}<span class="sub" title={t.autoFf.mark}><Icon name="pull" size={12} /></span>{/if}
            {#if tracks.get(r.name)}
              {@const tr = tracks.get(r.name)!}
              <span class="sub track" title={tr.gone ? t.gone : `${t.toPush(tr.ahead)} · ${t.toPull(tr.behind)}`}>
                {tr.gone ? t.gone : `${tr.ahead ? '↑' + tr.ahead : ''} ${tr.behind ? '↓' + tr.behind : ''}`}
              </span>
            {/if}
            <!-- 行本身是按钮，里面不能再套按钮；键盘从右键菜单里的同名项操作 -->
            {#if kind !== 'tag'}
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
              <span
                class="sub filter pin"
                class:active={pinned.includes(r.name)}
                title={pinned.includes(r.name) ? t.branchList.unpin : t.branchList.pin}
                onclick={(e) => { e.stopPropagation(); onpin(r.name) }}
                ondblclick={(e) => e.stopPropagation()}
              ><Icon name="pin" size={12} /></span>
            {/if}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <span
              class="sub filter"
              class:active={scope.includes(r.name)}
              title={scope.includes(r.name) ? t.graphFilter.remove : t.graphFilter.add}
              onclick={(e) => { e.stopPropagation(); onscope(r.name) }}
              ondblclick={(e) => e.stopPropagation()}
            ><Icon name="filter" size={12} /></span>
          </button>
        {/each}
      </details>
    {/snippet}
    {@render section('branch', t.branches, branches, true)}
    {@render section('remote', t.remotes, remotes, filter !== '')}
    {@render section('tag', t.tags, tags, filter !== '')}
    <details>
      <summary>{t.stashes}<span class="count">{stashes.length}</span></summary>
      {#each stashes as s (s.name)}
        <button class:on={s.name === selected} title={s.subject} onclick={() => { selected = s.name; onstash(s.name) }} ondblclick={() => onactivate('stash', s.name)} oncontextmenu={(e) => onmenu(e, 'stash', s.name, s.id)}>
          <span class="name">{s.subject}</span>
        </button>
      {/each}
    </details>
    <details open>
      <summary>{t.worktrees}<span class="count">{worktrees.length}</span></summary>
      {#each worktrees as w (w.path)}
        <button class:head={w.current} title={w.path} onclick={() => onjump(w.head)} ondblclick={() => w.current || onworktree(w.path)}>
          <span class="name">{basename(w.path)}</span>
          <span class="sub">{w.branch ? w.branch.replace('refs/heads/', '') : t.detached}{w.changes ? ` · ${t.wtDirty(w.changes)}` : ''}</span>
        </button>
      {/each}
    </details>
  </div>
</aside>

<style>
  aside {
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--panel);
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px;
  }
  nav button {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-md);
    background: none;
    cursor: pointer;
  }
  nav button span:first-of-type {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  nav button:hover {
    background: var(--hover);
  }
  nav button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .pill {
    min-width: 20px;
    padding: 0 4px;
    border-radius: var(--r-lg);
    background: var(--accent);
    color: var(--on-accent);
    font-size: var(--fs-sm);
    font-weight: 600;
    line-height: 20px;
    text-align: center;
  }
  .pill.quiet {
    background: var(--raised);
    color: var(--muted);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    margin: 0 8px 8px;
    padding: 0 8px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    color: var(--muted);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  input {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font-size: var(--fs-sm);
    outline: none;
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    padding-bottom: 12px;
  }
  summary {
    padding: 8px 12px 4px;
    font-size: var(--fs-sm);
    color: var(--muted);
    letter-spacing: 0.04em;
    cursor: default;
  }
  summary:hover {
    color: var(--text);
  }
  .count {
    margin-left: 8px;
    opacity: 0.7;
  }
  details button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 26px;
    padding: 0 12px 0 24px;
    border: 0;
    background: none;
    text-align: left;
    cursor: default;
  }
  details button:hover {
    background: var(--hover);
  }
  details button.on {
    background: var(--accent-soft);
  }
  .tick {
    width: 16px;
    margin: 0 -8px 0 -16px;
  }
  details button:active:enabled {
    transform: none;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  button.head {
    color: var(--accent);
    font-weight: 600;
  }
  /* 上游名字可以被挤短，分支名优先 */
  .up {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  details {
    position: relative;
  }
  /* 分组标题右端的小按钮；选择器要压过下面 details button 的整行样式 */
  details .clean {
    position: absolute;
    right: 8px;
    top: 6px;
    width: auto;
    height: auto;
    padding: 2px 4px;
    border: 0;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }
  .clean:hover {
    color: var(--text);
  }
  .pin + .filter {
    margin-left: 0;
  }
  .filter {
    margin-left: auto;
    padding: 2px;
    border-radius: var(--r-sm);
    opacity: 0;
    cursor: pointer;
  }
  details button:hover .filter,
  .filter.active {
    opacity: 1;
  }
  .filter:hover {
    background: var(--hover);
    color: var(--text);
  }
  .filter.active {
    color: var(--accent);
  }
  .sub {
    flex: none;
    display: inline-flex;
    color: var(--muted);
    font-size: var(--fs-sm);
    white-space: nowrap;
  }
  /* 界面字体里 ↑ 和数字 1 太像，「↑2」会被读成「12」 */
  .track {
    font-family: var(--mono);
  }
</style>
