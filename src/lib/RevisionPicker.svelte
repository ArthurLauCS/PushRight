<script lang="ts">
  import type { Refs } from './api'
  import { revisionOptions } from './revision-options'
  import Icon from './Icon.svelte'
  import { t } from './i18n.svelte'

  let { refs, value = $bindable(''), label }: { refs: Refs; value?: string; label: string } = $props()
  const id = $props.id()
  let root: HTMLDivElement
  let input: HTMLInputElement
  let open = $state(false)
  let query = $state('')
  let active = $state(-1)
  const groups = $derived(revisionOptions(refs, query))
  const options = $derived(groups.flatMap((group) => group.items))
  const indexes = $derived(new Map(options.map((item, index) => [item.name, index])))
  const labels = $derived({ current: t.tools.currentBranches, common: t.tools.commonBranches,
    branches: t.branches, remotes: t.remotes, tags: t.tags, other: t.tools.otherRefs })

  function show() { query = ''; active = -1; open = true }
  function pick(name: string) { value = name; input.focus(); open = false; active = -1 }
  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && open) { event.preventDefault(); event.stopPropagation(); open = false }
    else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      if (!open) show()
      if (!options.length) return
      active = event.key === 'ArrowDown' ? (active + 1) % options.length : active <= 0 ? options.length - 1 : active - 1
    } else if (event.key === 'Enter' && open) {
      if (options[active]) { event.preventDefault(); pick(options[active].name) }
      else open = false
    }
  }
  $effect(() => { if (open && active >= 0) root?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }) })
</script>

<svelte:window onpointerdown={(event) => { if (!root.contains(event.target as Node)) open = false }} onblur={() => open = false} />

<div class="picker" bind:this={root} onfocusout={(event) => { if (!root.contains(event.relatedTarget as Node)) open = false }}>
  <label for={id}>{label}</label>
  <div class="entry">
    <input class="field" {id} bind:this={input} bind:value required autocomplete="off" spellcheck="false"
      role="combobox" aria-autocomplete="list" aria-expanded={open} aria-controls={`${id}-list`}
      aria-activedescendant={open && options[active] ? `${id}-${active}` : undefined}
      placeholder={t.tools.selectRevision} onfocus={show} onclick={() => { if (!open) show() }}
      oninput={(event) => { query = event.currentTarget.value; active = -1; open = true }} onkeydown={keydown} />
    <button class="btn icon" type="button" aria-label={t.tools.selectRevision} aria-expanded={open} aria-controls={`${id}-list`}
      onclick={() => { if (open) open = false; else { input.focus(); show() } }}><Icon name="down" size={14} /></button>
  </div>
  {#if open}
    <div class="options" id={`${id}-list`} role="listbox" aria-label={label}>
      {#each groups as group (group.key)}
        <div role="group" aria-labelledby={`${id}-${group.key}`}>
          <div class="heading" id={`${id}-${group.key}`}>{labels[group.key]}</div>
          {#each group.items as item (item.name)}
            {@const index = indexes.get(item.name)!}
            <button type="button" role="option" id={`${id}-${index}`} aria-selected={active === index} tabindex="-1"
              title={item.name} onpointerdown={(event) => event.preventDefault()} onclick={() => pick(item.name)}>{item.label}</button>
          {/each}
        </div>
      {:else}<div class="heading">{t.tools.noMatchingRefs}</div>{/each}
    </div>
  {/if}
</div>

<style>
  .picker { position: relative; flex: 1; min-width: 200px; }
  label { display: block; margin-bottom: 4px; color: var(--muted); }
  .entry { display: flex; gap: 4px; }
  input { flex: 1; min-width: 0; }
  .options { position: absolute; top: 100%; left: 0; right: 0; z-index: 10; max-height: 320px; overflow: auto; padding: 4px;
    background: var(--raised); border: 1px solid var(--border-strong); border-radius: var(--r-md); box-shadow: var(--shadow-pop); }
  .heading { padding: 8px; color: var(--muted); font-size: var(--fs-sm); }
  .options button { display: block; width: 100%; padding: 6px 12px; border: 0; border-radius: var(--r-sm);
    background: none; text-align: left; overflow-wrap: anywhere; }
  .options button:hover, .options button[aria-selected="true"] { background: var(--accent); color: var(--on-accent); }
</style>
