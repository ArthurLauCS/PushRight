<script lang="ts">
  import { fileIcon, GLYPHS } from './file-icons'

  /** `folder` 为 true 时画文件夹，否则按 `path` 的文件类型选图标 */
  let { path = '', folder = false, size = 14 }: { path?: string; folder?: boolean; size?: number } = $props()
  const icon = $derived(folder ? { glyph: 'folder' as const, color: 'var(--yellow)' } : fileIcon(path))
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={icon.color} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d={GLYPHS[icon.glyph]} />
</svg>

<style>
  svg {
    flex: none;
  }
</style>
