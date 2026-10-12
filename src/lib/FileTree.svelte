<script lang="ts" generics="T extends { path: string }">
  import type { Snippet } from 'svelte'
  import { diffPrefs } from './diff-prefs.svelte'
  import { treeRows } from './file-tree'
  import FileIcon from './FileIcon.svelte'
  import Icon from './Icon.svelte'

  let {
    items,
    row,
    folder,
    compact = false,
  }: {
    items: T[]
    /** 一行文件。depth 为 -1 表示平铺显示，目录由调用方自己带上；否则用 indent(depth) 缩进 */
    row: Snippet<[T, number]>
    /** 文件夹行右侧的按钮，参数是它下面的所有文件 */
    folder?: Snippet<[T[]]>
    compact?: boolean
  } = $props()

  let closed = $state.raw(new Set<string>())
  const rows = $derived(diffPrefs.tree ? treeRows(items, closed) : null)

  function toggle(key: string) {
    const next = new Set(closed)
    if (!next.delete(key)) next.add(key)
    closed = next
  }

  /** 全部展开 / 全部折叠 */
  export function setAll(open: boolean) {
    closed = new Set(open ? [] : treeRows(items).flatMap((r) => (r.items ? [r.key] : [])))
  }
</script>

{#if rows}
  {#each rows as r (r.key)}
    {#if r.item}
      {@render row(r.item, r.depth)}
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="folder" class:compact style:padding-left="{12 + r.depth * 16}px" title={r.key} onclick={() => toggle(r.key)}>
        <Icon name={r.open ? 'down' : 'right'} size={12} />
        <FileIcon folder />
        <span class="name">{r.name}</span>
        <span class="count">{r.items!.length}</span>
        {@render folder?.(r.items!)}
      </div>
    {/if}
  {/each}
{:else}
  {#each items as item (item.path)}
    {@render row(item, -1)}
  {/each}
{/if}

<style>
  .folder {
    display: flex;
    align-items: center;
    gap: 4px;
    height: var(--row);
    padding-right: 8px;
    color: var(--muted);
    cursor: default;
  }
  .folder.compact {
    height: 24px;
    font-size: var(--fs-sm);
  }
  .folder:hover {
    background: var(--hover);
    color: var(--text);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    flex: 1;
    margin-left: 4px;
    font-size: var(--fs-sm);
    opacity: 0.7;
  }
</style>
