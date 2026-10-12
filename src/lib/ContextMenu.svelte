<script lang="ts" module>
  /** null 表示分隔线。给了 `items` 的是子菜单：点它换成子项，顶上多一行「返回」 */
  export type Item = { label: string; hint?: string; action?: () => void; items?: Item[]; danger?: boolean; disabled?: boolean } | null
</script>

<script lang="ts">
  import Icon from './Icon.svelte'
  import Label from './Label.svelte'
  import { t } from './i18n.svelte'

  let items = $state<Item[]>([])
  /** 进入子菜单前的各级菜单，「返回」时弹出 */
  let parents = $state<{ title: string; items: Item[] }[]>([])
  let x = $state(0)
  let y = $state(0)
  let menu = $state<HTMLDivElement>()

  export function show(e: MouseEvent, list: Item[]) {
    e.preventDefault()
    items = list
    parents = []
    x = e.clientX
    y = e.clientY
  }

  // 贴近窗口边缘时往回收，保证整个菜单可见；进出子菜单后高度变了也要重算
  $effect(() => {
    items
    if (!menu) return
    const r = menu.getBoundingClientRect()
    if (r.right > innerWidth) x = Math.max(0, innerWidth - r.width - 4)
    if (r.bottom > innerHeight) y = Math.max(0, innerHeight - r.height - 4)
  })

  const close = () => (items = [])

  function pick(item: NonNullable<Item>) {
    if (item.items) {
      parents.push({ title: item.label, items })
      items = item.items
    } else {
      close()
      item.action?.()
    }
  }
</script>

<svelte:window onmousedown={(e) => menu?.contains(e.target as Node) || close()} onkeydown={(e) => e.key === 'Escape' && close()} onblur={close} />

{#if items.length}
  <div class="menu" bind:this={menu} style:left="{x}px" style:top="{y}px" role="menu">
    {#if parents.length}
      <button role="menuitem" class="back" onclick={() => (items = parents.pop()!.items)}><Icon name="left" size={12} />{t.menuBack} · {parents.at(-1)!.title}</button>
      <hr />
    {/if}
    {#each items as item, i (i)}
      {#if item}
        <button role="menuitem" aria-haspopup={item.items ? 'menu' : undefined} disabled={item.disabled} class:danger={item.danger} onclick={() => pick(item)}>
          {#if item.items}<span class="more"><Icon name="right" size={12} /></span>{/if}
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
  .more {
    float: right;
    margin-top: 2px;
    opacity: 0.7;
  }
  .back {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: var(--fs-sm);
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
