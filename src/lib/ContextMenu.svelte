<script lang="ts" module>
  /** null 表示分隔线 */
  export type Item = { label: string; hint?: string; action: () => void; danger?: boolean; disabled?: boolean } | null
</script>

<script lang="ts">
  import Label from './Label.svelte'

  let items = $state<Item[]>([])
  let x = $state(0)
  let y = $state(0)
  let menu = $state<HTMLDivElement>()

  export function show(e: MouseEvent, list: Item[]) {
    e.preventDefault()
    items = list
    x = e.clientX
    y = e.clientY
  }

  // 贴近窗口边缘时往回收，保证整个菜单可见
  $effect(() => {
    if (!menu) return
    const r = menu.getBoundingClientRect()
    if (r.right > innerWidth) x = Math.max(0, innerWidth - r.width - 4)
    if (r.bottom > innerHeight) y = Math.max(0, innerHeight - r.height - 4)
  })

  const close = () => (items = [])
</script>

<svelte:window onmousedown={(e) => menu?.contains(e.target as Node) || close()} onkeydown={(e) => e.key === 'Escape' && close()} onblur={close} />

{#if items.length}
  <div class="menu" bind:this={menu} style:left="{x}px" style:top="{y}px" role="menu">
    {#each items as item, i (i)}
      {#if item}
        <button role="menuitem" disabled={item.disabled} class:danger={item.danger} onclick={() => (close(), item.action())}>
          <Label text={item.label} />
          {#if item.hint}<small class="hint">{item.hint}</small>{/if}
        </button>
      {:else}
        <hr />
      {/if}
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 10;
    min-width: 264px;
    max-width: calc(100vw - 8px);
    max-height: calc(100vh - 8px);
    overflow: auto;
    padding: 4px;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-md);
    background: var(--raised);
    box-shadow: var(--shadow-pop);
    animation: pop var(--t-pop);
  }
  button {
    display: block;
    width: 100%;
    padding: 4px 12px;
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    text-align: left;
    white-space: normal;
    overflow-wrap: anywhere;
  }
  button:hover:enabled {
    background: var(--accent);
    color: var(--on-accent);
  }
  button:active:enabled {
    transform: none;
  }
  .hint {
    display: block;
    font-size: var(--fs-sm);
    opacity: 0.7;
  }
  .danger {
    color: var(--red);
  }
  .danger:hover:enabled {
    background: var(--red);
    color: var(--on-danger);
  }
  hr {
    margin: 4px 0;
    border: 0;
    border-top: 1px solid var(--border);
  }
</style>
