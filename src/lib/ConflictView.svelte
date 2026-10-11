<script lang="ts">
  import type { Conflict, Side } from './api'
  import Label from './Label.svelte'
  import { t } from './i18n.svelte'

  let {
    conflict,
    rebasing,
    onresolve,
    ontake,
  }: {
    conflict: Conflict | null
    rebasing: boolean
    /** 每个冲突块选了哪一边，按出现顺序 */
    onresolve: (choices: Side[]) => void
    /** 整个文件取一边 */
    ontake: (theirs: boolean) => void
  } = $props()

  let choices = $state<(Side | null)[]>([])
  $effect(() => {
    choices = (conflict?.blocks ?? []).filter((b) => b.kind === 'conflict').map(() => null)
  })

  // 冲突块在 choices 里的下标
  const numbered = $derived.by(() => {
    let n = 0
    return (conflict?.blocks ?? []).map((b) => ({ block: b, index: b.kind === 'conflict' ? n++ : -1 }))
  })
  const done = $derived(choices.length > 0 && choices.every((c) => c !== null))
</script>

<div class="conflict">
  {#if !conflict}
    <p class="note">{t.pickFile}</p>
  {:else}
    <header>
      <span>{conflict.binary ? t.conflictBinary : t.conflictHint(choices.length)}{rebasing ? ' ' + t.rebaseConflictHint : ''}</span>
      <button class="btn small" onclick={() => ontake(false)}><Label text={t.takeOurs} /></button>
      <button class="btn small" onclick={() => ontake(true)}><Label text={t.takeTheirs} /></button>
      {#if !conflict.binary}
        <button class="btn small primary" disabled={!done} onclick={() => onresolve(choices as Side[])}><Label text={t.resolve} /></button>
      {/if}
    </header>
    <div class="body">
      {#each numbered as { block, index }, i (i)}
        {#if block.kind === 'text'}
          <pre class="same">{block.text}</pre>
        {:else}
          {@const choice = choices[index]}
          <section>
            <div class="side" class:picked={choice === 'ours' || choice === 'both'} class:dropped={choice === 'theirs'}>
              <div class="bar">
                <span><Label text={t.ours} /><small>{block.ours_label}</small></span>
                <button class="btn small" class:on={choice === 'ours'} onclick={() => (choices[index] = 'ours')}>{t.useThis}</button>
              </div>
              <pre>{block.ours || t.emptySide}</pre>
            </div>
            <div class="side theirs" class:picked={choice === 'theirs' || choice === 'both'} class:dropped={choice === 'ours'}>
              <div class="bar">
                <span><Label text={t.theirs} /><small>{block.theirs_label}</small></span>
                <button class="btn small" class:on={choice === 'theirs'} onclick={() => (choices[index] = 'theirs')}>{t.useThis}</button>
                <button class="btn small" class:on={choice === 'both'} onclick={() => (choices[index] = 'both')}>{t.useBoth}</button>
              </div>
              <pre>{block.theirs || t.emptySide}</pre>
            </div>
          </section>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .conflict {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .note {
    margin: 0;
    padding: 48px 24px;
    text-align: center;
    color: var(--muted);
  }
  /* 说明独占一行，按钮在下面一行并可换行：按钮再长也不会把说明挤成一列 */
  header {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    min-height: 44px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }
  header span {
    flex: 1 1 100%;
    color: var(--yellow);
    font-family: var(--prose);
    font-size: var(--fs-md);
  }
  .body {
    flex: 1;
    overflow: auto;
    user-select: text;
  }
  pre {
    margin: 0;
    padding: 4px 16px;
    font: var(--fs-sm) / 20px var(--mono);
    white-space: pre;
    tab-size: 4;
  }
  .same {
    color: var(--muted);
  }
  section {
    margin: 8px 0;
    border-block: 1px solid var(--border);
  }
  .side {
    border-left: 3px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .side.theirs {
    border-left-color: var(--green);
    background: color-mix(in srgb, var(--green) 8%, transparent);
  }
  .side.dropped {
    opacity: 0.35;
  }
  .side.dropped pre {
    text-decoration: line-through;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    font-size: var(--fs-sm);
  }
  .bar span {
    flex: 1;
    font-weight: 600;
  }
  small {
    margin-left: 8px;
    font-family: var(--mono);
    font-weight: 400;
    color: var(--muted);
  }
</style>
