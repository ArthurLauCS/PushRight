<script lang="ts">
  import type { Worktree } from './api'
  import { explain } from './i18n.svelte'
  import { fmtTime, t } from './i18n.svelte'
  import Label from './Label.svelte'

  let {
    worktrees,
    onopen,
    onremove,
    onnew,
  }: { worktrees: Worktree[]; onopen: (path: string) => void; onremove: (w: Worktree) => void; onnew: () => void } = $props()

  const name = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p
</script>

<div class="overview">
  <header>
    <div>
      <h2>{t.worktrees}</h2>
      <p>{explain.worktree.what}</p>
    </div>
    <button class="btn primary" onclick={onnew}><Label text={t.newWorktree} /></button>
  </header>
  <div class="table">
    <div class="row head">
      <span>{t.wtFolder}</span><span>{t.wtBranch}</span><span>{t.wtChanges}</span><span>{t.wtSync}</span><span>{t.wtLast}</span><span></span>
    </div>
    {#each worktrees as w, i (w.path)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="row" class:current={w.current} ondblclick={() => w.current || onopen(w.path)}>
        <span class="folder" title={w.path}><strong>{name(w.path)}</strong><small>{w.path}</small></span>
        <span>
          <span class="branch">{w.branch ? w.branch.replace('refs/heads/', '') : t.detached}</span>
          {#if w.current}<small>{t.wtCurrent}</small>{/if}
        </span>
        <span class:dirty={w.changes > 0}>{w.changes ? t.wtDirty(w.changes) : t.wtClean}</span>
        <span>
          {#if !w.ahead_behind}<small>{t.wtNoUpstream}</small>
          {:else if w.ahead_behind[0] + w.ahead_behind[1] === 0}{t.wtSynced}
          {:else}{t.toPush(w.ahead_behind[0])} · {t.toPull(w.ahead_behind[1])}{/if}
        </span>
        <span class="last" title={w.subject}>{w.subject}<small>{w.time ? fmtTime(w.time) : ''}</small></span>
        <span class="actions">
          {#if !w.current}
            <button class="btn small" onclick={() => onopen(w.path)}>{t.openInTab}</button>
            <!-- 第一个是主工作树，git 不允许删除 -->
            {#if i > 0}<button class="btn small danger" title={t.wtRemove} onclick={() => onremove(w)}><Label text={t.wtRemove} /></button>{/if}
          {/if}
        </span>
      </div>
    {/each}
  </div>
</div>

<style>
  .overview {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 24px 32px;
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 24px;
    margin-bottom: 24px;
  }
  header div {
    flex: 1;
  }
  h2 {
    margin: 0 0 8px;
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  p {
    margin: 0;
    max-width: 38em;
    color: var(--muted);
    font-family: var(--prose);
    font-size: var(--fs-lg);
    line-height: var(--lh-body);
  }
  .table {
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--panel);
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(160px, 1.3fr) minmax(120px, 1fr) 96px 160px minmax(160px, 1.4fr) 168px;
    gap: 16px;
    align-items: center;
    min-height: 56px;
    padding: 8px 16px;
    border-top: 1px solid var(--border);
  }
  .row:first-child {
    border-top: 0;
  }
  .row.head {
    min-height: 32px;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .row:not(.head):hover {
    background: var(--hover);
  }
  .row.current {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 var(--accent);
  }
  .row > span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  small {
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .folder small {
    font-family: var(--mono);
  }
  .folder small,
  .last small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .branch {
    margin-right: 8px;
    padding: 2px 8px;
    border-radius: var(--r-sm);
    color: var(--accent);
    background: var(--accent-soft);
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .dirty {
    color: var(--yellow);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .actions :global(.cmd) {
    display: none;
  }
</style>
