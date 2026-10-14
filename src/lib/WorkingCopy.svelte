<script lang="ts">
  import { untrack } from 'svelte'
  import * as api from './api'
  import ConflictView from './ConflictView.svelte'
  import { diffOptions } from './diff-prefs.svelte'
  import DiffView from './DiffView.svelte'
  import FileIcon from './FileIcon.svelte'
  import FileListBar from './FileListBar.svelte'
  import FileTree from './FileTree.svelte'
  import { indent } from './file-tree'
  import Icon from './Icon.svelte'
  import Label from './Label.svelte'
  import { layout } from './layout.svelte'
  import Splitter from './Splitter.svelte'
  import { t, errorText } from './i18n.svelte'

  let {
    tab,
    entries,
    identity,
    editIdentity,
    identityBusy,
    rebasing,
    conflicts,
    prefill,
    reload,
    oncommitted,
    discard,
    discardLines,
    onfilemenu,
    onexternal,
    check,
    beforeCommit,
    showLog,
    busy = $bindable(false),
  }: {
    tab: number
    entries: api.Entry[]
    identity: api.Identity | null
    editIdentity: () => void
    identityBusy: boolean
    rebasing: boolean
    /** 进行中的操作冲突过的全部文件，已解决的也在内 */
    conflicts: { path: string; resolved: boolean }[]
    /** Git 为进行中的合并 / 摘取 / 撤销准备好的提交信息；没有时为空 */
    prefill: string
    reload: () => Promise<void>
    oncommitted: () => void
    /** 丢弃这些路径的改动；确认对话框和执行都由上层负责 */
    discard: (paths: string[]) => void
    discardLines: (path: string, hunk: number, header: string, lines: number[]) => void
    /** 文件行的右键菜单 */
    onfilemenu: (e: MouseEvent, entry: api.Entry, staged: boolean) => void
    /** 用外部合并工具打开冲突文件；没配置外部工具时不传 */
    onexternal?: (path: string) => void
    /** 提交前检查：仓库设置里的命令、是否自动、是否正在跑、上一次的结果 */
    check: { command: string; auto: boolean; running: boolean; result: api.Log | null; run: () => Promise<boolean> }
    /** 提交前调用；返回 false 就不提交 */
    beforeCommit: () => Promise<boolean>
    /** 打开命令日志，看检查的输出 */
    showLog: () => void
    busy?: boolean
  } = $props()

  let sel = $state<{ path: string; staged: boolean } | null>(null)
  let diff = $state.raw<api.Diff | null>(null)
  /** 选中的是冲突文件时显示逐处解决的视图 */
  let conflict = $state.raw<api.Conflict | null>(null)
  let conflicted = $state(false)
  let message = $state('')
  let amend = $state(false)
  let error = $state('')
  const disabled = $derived(busy || identityBusy)

  /** 上一次自动填进提交框的内容。提交框里还是它（或还空着）才会换，用户写过的不动 */
  let filled = ''
  $effect(() => {
    const next = prefill
    untrack(() => {
      if (message === filled) message = next
      filled = next
    })
  })

  let filter = $state('')
  const trees = $state<{ unstaged?: { setAll(open: boolean): void }; staged?: { setAll(open: boolean): void } }>({})
  function shown(list: api.Entry[]) {
    const query = filter.trim().toLowerCase()
    return query ? list.filter((e) => e.path.toLowerCase().includes(query)) : list
  }

  const unstaged = $derived(entries.filter((e) => e.unstaged))
  const staged = $derived(entries.filter((e) => e.staged))
  const canCommit = $derived(!busy && !identityBusy && !check.running && message.trim() !== '' && (staged.length > 0 || amend))

  /** 最近的提交信息，点开下拉时才取 */
  let recent = $state.raw<string[]>([])
  const loadRecent = async () => (recent = await api.recentMessages(tab).catch(() => []))

  $effect(() => {
    const s = sel
    if (!s) {
      diff = null
      return
    }
    const entry = entries.find((e) => e.path === s.path && (s.staged ? e.staged : e.unstaged))
    if (!entry) {
      sel = null
      return
    }
    let stale = false
    const fail = (e: unknown) => stale || (error = String(e))
    conflicted = entry.conflicted
    if (entry.conflicted) api.conflictRead(tab, s.path).then((c) => stale || (conflict = c), fail)
    else api.diffWorktree(tab, s.path, s.staged, !s.staged && entry.unstaged === '?', diffOptions()).then((d) => stale || (diff = d), fail)
    return () => (stale = true)
  })

  async function run(op: () => Promise<unknown>) {
    if (disabled) return
    busy = true
    error = ''
    try {
      await op()
    } catch (e) {
      error = String(e)
    }
    await reload()
    busy = false
  }

  const stage = (list: api.Entry[]) => run(() => api.stage(tab, list.map((e) => e.path)))
  const unstage = (list: api.Entry[]) => run(() => api.unstage(tab, list.map((e) => e.path)))

  async function commit() {
    if (!canCommit || !(await beforeCommit())) return
    await run(() => api.commit(tab, message, amend))
    if (error) return
    // 回到自动填入的内容（提交信息模板），没有就是空的
    message = filled
    amend = false
    oncommitted()
  }

  async function toggleAmend() {
    if (amend && !message.trim()) message = await api.lastMessage(tab).catch(() => '')
  }

  const dir = (p: string) => p.slice(0, p.lastIndexOf('/') + 1)
  const base = (p: string) => p.slice(p.lastIndexOf('/') + 1)

  let folded = $state([false, false, false])
  let sizes = $state([120, 120, 170])
  const panes: HTMLDivElement[] = []
  let dragSizes: number[] = []
  const nextOpen = (index: number) => folded.findIndex((closed, i) => i > index && !closed)

  function startResize() {
    // flex-basis 留出 80px，拖动只分配剩余空间；按实际高度起拖，窗口缩放后也不跳动。
    dragSizes = sizes.map((size, i) => folded[i] ? size : Math.max(1, panes[i]!.getBoundingClientRect().height - 80))
  }

  function resize(index: number, delta: number) {
    const next = nextOpen(index)
    if (next < 0) return
    const total = dragSizes[index]! + dragSizes[next]!
    const first = Math.max(1, Math.min(total - 1, dragSizes[index]! + delta))
    sizes = dragSizes.map((size, i) => i === index ? first : i === next ? total - first : size)
  }
</script>

{#snippet divider(index: number)}
  {#if !folded[index] && nextOpen(index) >= 0}
    <Splitter dir="y" label={t.dragHint} value={Math.round(100 * sizes[index]! / (sizes[index]! + sizes[nextOpen(index)]!))} onstart={startResize} onresize={(delta) => resize(index, delta)} onreset={() => (sizes = [120, 120, 170])} />
  {/if}
{/snippet}

{#snippet list(title: string, hint: string, items: api.Entry[], isStaged: boolean)}
  {@const index = isStaged ? 1 : 0}
  <div class="group" class:folded={folded[index]} style:flex-grow={folded[index] ? 0 : sizes[index]} bind:this={panes[index]}>
    <div class="title">
      <button class="section-toggle" aria-expanded={!folded[index]} onclick={() => (folded[index] = !folded[index])} title={`${title} · ${hint}`}>
        <Icon name={folded[index] ? 'right' : 'down'} size={12} />
        <span>{title}<span class="count">{filter.trim() ? t.fileList.shown(shown(items).length, items.length) : items.length}</span><span class="hint">{hint}</span></span>
      </button>
      {#if !isStaged}
        <button class="btn small quiet danger" disabled={disabled || !items.length} title={t.discardAll} onclick={() => discard([])}><Label text={t.discardAll} /></button>
      {/if}
      <button class="btn small" disabled={disabled || !items.length} title={isStaged ? t.unstageAll : t.stageAll} onclick={() => (isStaged ? unstage(items) : stage(items))}>
        <Label text={isStaged ? t.unstageAll : t.stageAll} />
      </button>
    </div>
    <div class="items" hidden={folded[index]}>
      <FileTree items={shown(items)} bind:this={trees[isStaged ? 'staged' : 'unstaged']}>
        {#snippet row(e, depth)}
          {@const s = (isStaged ? e.staged : e.unstaged) ?? ''}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div
            class="item"
            class:selected={sel?.path === e.path && sel.staged === isStaged}
            style:padding-left={indent(depth)}
            title={e.path}
            onclick={() => (sel = { path: e.path, staged: isStaged })}
            ondblclick={() => (isStaged ? unstage([e]) : stage([e]))}
            oncontextmenu={(ev) => { sel = { path: e.path, staged: isStaged }; onfilemenu(ev, e, isStaged) }}
          >
            <span class="status s{s === '?' ? 'A' : s}" title={t.status[s] ?? s}>{s}</span>
            <FileIcon path={e.path} />
            <span class="path">{base(e.path) || e.path}{#if depth < 0}<span class="dir">{dir(e.path.replace(/\/$/, ''))}</span>{/if}</span>
            {#if !isStaged && !e.conflicted}
              <button class="btn small quiet icon danger" disabled={disabled} title={t.discardFile} onclick={(ev) => (ev.stopPropagation(), discard([e.path]))}>
                <Icon name="undo" size={14} />
              </button>
            {/if}
            <button
              class="btn small quiet icon"
              disabled={disabled}
              title={isStaged ? t.unstageFile : t.stageFile}
              onclick={(ev) => (ev.stopPropagation(), isStaged ? unstage([e]) : stage([e]))}
            >
              <Icon name={isStaged ? 'minus' : 'plus'} size={14} />
            </button>
          </div>
        {/snippet}
        {#snippet folder(list)}
          {#if !isStaged}
            <!-- 空列表在 discard 里表示「全部」，文件夹里没有可丢弃的文件时必须禁用 -->
            {@const paths = list.filter((e) => !e.conflicted).map((e) => e.path)}
            <button class="btn small quiet icon danger" disabled={disabled || !paths.length} title={t.fileList.discardFolder} onclick={(ev) => (ev.stopPropagation(), discard(paths))}>
              <Icon name="undo" size={14} />
            </button>
          {/if}
          <button
            class="btn small quiet icon"
            disabled={disabled}
            title={isStaged ? t.fileList.unstageFolder : t.fileList.stageFolder}
            onclick={(ev) => (ev.stopPropagation(), isStaged ? unstage(list) : stage(list))}
          >
            <Icon name={isStaged ? 'minus' : 'plus'} size={14} />
          </button>
        {/snippet}
      </FileTree>
    </div>
  </div>
  {@render divider(index)}
{/snippet}

<div class="working">
  <div class="left" style:width="min({layout.changes}px, 50%)">
    {#if !entries.length}<p class="clean">{t.clean}</p>{/if}
    {#if conflicts.length}
      <div class="conflicts">
        <div class="title"><span>{t.conflictFiles}<span class="count">{t.conflictProgress(conflicts.filter((c) => c.resolved).length, conflicts.length)}</span></span></div>
        {#each conflicts as c (c.path)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="item" class:selected={sel?.path === c.path} title={c.path} onclick={() => (sel = { path: c.path, staged: c.resolved })}>
            <span class="status" class:sA={c.resolved} class:sU={!c.resolved} title={c.resolved ? t.conflictResolved : t.conflictOpen}>{c.resolved ? '✓' : '!'}</span>
            <FileIcon path={c.path} />
            <span class="path">{base(c.path)}<span class="dir">{dir(c.path)}</span></span>
          </div>
        {/each}
      </div>
    {/if}
    <FileListBar bind:filter onfold={(open) => { trees.unstaged?.setAll(open); trees.staged?.setAll(open) }} />
    {@render list(t.unstaged, t.unstagedHint, unstaged, false)}
    {@render list(t.staged, t.stagedHint, staged, true)}
    <div class="group" class:folded={folded[2]} style:flex-grow={folded[2] ? 0 : sizes[2]} bind:this={panes[2]}>
      <div class="title">
        <button class="section-toggle" aria-expanded={!folded[2]} onclick={() => (folded[2] = !folded[2])}>
          <Icon name={folded[2] ? 'right' : 'down'} size={12} />
          <span>{amend ? t.committer : t.commitIdentity}</span>
        </button>
      </div>
      <div class="commit" hidden={folded[2]}>
        {#if error}<p class="error">{errorText(error)}</p>{/if}
        <div class="identity">
          <span title={(amend ? identity?.committer : identity?.author) ?? t.identityMissing}>
            <strong>{(amend ? identity?.committer : identity?.author) ?? t.identityMissing}</strong>
          </span>
          <button class="btn small" disabled={busy || identityBusy} title={t.editIdentity} onclick={editIdentity}><Label text={t.editIdentity} /></button>
        </div>
        {#if amend}<small class="identity-note">{t.amendIdentityHint}</small>
        {:else if identity?.committer && identity.committer !== identity.author}
          <small class="identity-note">{t.committer}：{identity.committer}</small>
        {/if}
        <select class="field recent" aria-label={t.recentMessages} title={t.recentMessages} value="" onfocus={loadRecent} onchange={(e) => { message = recent[+e.currentTarget.value] ?? message; e.currentTarget.value = '' }}>
          <option value="" disabled hidden>{t.recentMessages}</option>
          {#each recent as m, i (i)}<option value={i}>{m.split('\n')[0].slice(0, 80)}</option>{/each}
        </select>
        <textarea
          placeholder={t.commitPlaceholder}
          bind:value={message}
          onkeydown={(e) => e.ctrlKey && e.key === 'Enter' && commit()}
        ></textarea>
        {#if check.command}
          <div class="check">
            <button class="btn small" disabled={check.running || disabled} title={`${t.check.run} · ${check.command}`} onclick={check.run}>{check.running ? t.working : t.check.run}</button>
            <code title={check.command}>{check.command}</code>
            {#if check.result}
              <button class="btn small quiet" class:failed={!check.result.ok} title={t.check.output} onclick={showLog}>{check.result.ok ? t.check.passed : t.check.failed}</button>
            {:else if check.auto}<small>{t.check.autoOn}</small>{/if}
          </div>
        {/if}
        <div class="actions">
          <label title={t.amend}><input type="checkbox" bind:checked={amend} onchange={toggleAmend} /><span><Label text={t.amend} /></span></label>
          <button class="btn primary" disabled={!canCommit} title={t.commit} onclick={commit}><Label text={t.commit} /></button>
        </div>
      </div>
    </div>
  </div>
  <Splitter key="changes" min={260} max={720} />
  {#if sel && conflicted}
    <ConflictView
      {conflict}
      {rebasing}
      onresolve={(choices) => sel && run(() => api.conflictResolve(tab, sel!.path, choices))}
      ontake={(theirs) => sel && run(() => api.conflictTake(tab, sel!.path, theirs))}
      onexternal={onexternal && (() => sel && onexternal(sel.path))}
    />
  {:else}
    <DiffView
      {diff}
      path={sel?.path}
      fetchPage={sel ? (skip) => api.diffPage(tab, sel!.staged ? 'staged' : entries.find((e) => e.path === sel!.path)?.unstaged === '?' ? 'untracked' : 'unstaged', '', '', sel!.path, skip, null, diffOptions()) : undefined}
      mode={sel?.staged ? 'staged' : 'unstaged'}
      onapply={(hunk, header, lines) => sel && run(() => api.applyLines(tab, sel!.path, sel!.staged, hunk, header, lines, diffOptions().context))}
      ondiscard={(hunk, header, lines) => sel && discardLines(sel.path, hunk, header, lines)}
    />
  {/if}
</div>

<style>
  .working {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .left {
    position: relative;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
    background: var(--panel);
    container-type: inline-size;
  }
  /* 分组标题一行放不下命令；其余按钮在面板窄时也只留名字，命令在悬停提示里 */
  .title :global(.cmd) {
    display: none;
  }
  @container (max-width: 560px) {
    .left :global(.cmd) {
      display: none;
    }
  }
  .clean {
    position: absolute;
    inset: 22% 0 auto;
    margin: 0;
    text-align: center;
    color: var(--muted);
    font-family: var(--prose);
    font-size: var(--fs-lg);
    pointer-events: none;
  }
  .group {
    flex: 1 0 80px;
    min-height: 80px;
    display: flex;
    flex-direction: column;
    border-bottom: 1px solid var(--border);
  }
  .group.folded {
    flex-basis: 37px;
    min-height: 37px;
  }
  [hidden] {
    display: none !important;
  }
  .section-toggle {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
    align-self: stretch;
  }
  .section-toggle > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* 冲突清单最多占三成高度，文件多时自己滚动，不挤掉下面两组 */
  .conflicts {
    flex: none;
    max-height: 30%;
    overflow-y: auto;
    border-bottom: 1px solid var(--border);
  }
  .title {
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 36px;
    padding: 0 8px 0 12px;
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .title > span:first-child {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font-weight: 600;
  }
  .count,
  .hint {
    margin-left: 8px;
    color: var(--muted);
    font-weight: 400;
  }
  .items {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    height: var(--row);
    padding: 0 8px 0 12px;
    cursor: default;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.selected {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 var(--accent);
  }
  .path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dir {
    margin-left: 8px;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  /* 行内操作按钮只在悬停或选中时出现。用 opacity 而不是 visibility：
     visibility: visible 会穿透上层隐藏的面板显示出来 */
  .item .btn {
    opacity: 0;
  }
  .item:hover .btn:enabled,
  .item.selected .btn:enabled,
  .item .btn:focus-visible {
    opacity: 1;
  }
  /* 文件夹行在 FileTree 里，按钮是这里传进去的 */
  :global(.folder) .btn {
    opacity: 0;
  }
  :global(.folder:hover) .btn:enabled,
  :global(.folder) .btn:focus-visible {
    opacity: 1;
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
  .sD { color: var(--diff-deleted); }
  .sU {
    color: var(--red);
  }
  .sR,
  .sC {
    color: var(--accent);
  }
  .commit {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
  }
  .commit > :global(*) {
    flex-shrink: 0;
  }
  textarea {
    flex-grow: 1;
    min-height: 96px;
    height: 96px;
    padding: 8px;
    resize: none;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    font-family: var(--prose);
    font-size: var(--fs-md);
    line-height: var(--lh-body);
    outline: none;
    user-select: text;
  }
  .recent {
    height: 24px;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-sm);
  }
  .check code {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
  .check small {
    color: var(--muted);
    white-space: nowrap;
  }
  .check .btn {
    flex: none;
    color: var(--green);
  }
  .check .btn:first-child {
    color: inherit;
  }
  .check .failed {
    color: var(--red);
  }
  .identity {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .identity > span {
    flex: 1;
    min-width: 0;
  }
  .identity strong {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .identity-note {
    color: var(--muted);
  }
  .identity strong {
    font-size: var(--fs-sm);
    font-weight: 500;
    user-select: text;
  }
  textarea:focus {
    border-color: var(--accent);
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
  }
  label {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  label span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions .btn {
    flex: none;
  }
  .error {
    margin: 0;
    max-height: 96px;
    overflow: auto;
    color: var(--red);
    font-size: var(--fs-sm);
    white-space: pre-wrap;
    user-select: text;
  }
</style>
