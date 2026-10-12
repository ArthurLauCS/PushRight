<script lang="ts">
  import { diffPrefs, setDiffPref } from './diff-prefs.svelte'
  import Icon from './Icon.svelte'
  import { t } from './i18n.svelte'

  /** 变更文件列表上方的一排：按路径筛选、树 / 平铺切换、全部展开 / 折叠。 */
  let { filter = $bindable(''), onfold }: { filter?: string; onfold: (open: boolean) => void } = $props()
</script>

<div class="bar">
  <input class="field" type="search" placeholder={t.fileList.filter} aria-label={t.fileList.filter} bind:value={filter} />
  <button class="btn small quiet icon" disabled={!diffPrefs.tree} title={t.fileList.expandAll} aria-label={t.fileList.expandAll} onclick={() => onfold(true)}><Icon name="expand" size={14} /></button>
  <button class="btn small quiet icon" disabled={!diffPrefs.tree} title={t.fileList.collapseAll} aria-label={t.fileList.collapseAll} onclick={() => onfold(false)}><Icon name="collapse" size={14} /></button>
  <button class="btn small quiet icon" class:on={diffPrefs.tree} title={t.fileList.tree} aria-label={t.fileList.tree} aria-pressed={diffPrefs.tree} onclick={() => setDiffPref('tree', !diffPrefs.tree)}><Icon name="tree" size={14} /></button>
</div>

<style>
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 8px;
  }
  .field {
    flex: 1;
    min-width: 0;
    height: 24px;
    margin-right: 4px;
    font-size: var(--fs-sm);
    user-select: text;
  }
</style>
