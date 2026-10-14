<script lang="ts">
  import { DEFAULTS, layout, saveLayout, type LayoutKey } from './layout.svelte'

  let {
    key,
    dir = 'x',
    min = 0,
    max = Infinity,
    invert = false,
    onstart,
    onresize,
    onreset,
    label,
    value = 0,
  }: {
    /** 拖动时改 layout 里的哪一项 */
    key?: LayoutKey
    /** x：左右拖改宽度；y：上下拖改高度 */
    dir?: 'x' | 'y'
    min?: number
    max?: number
    /** 被调整的栏位在分隔条的右侧或下方时，拖动方向与尺寸变化相反 */
    invert?: boolean
    onstart?: () => void
    onresize?: (delta: number) => void
    onreset?: () => void
    label?: string
    value?: number
  } = $props()

  let dragging = $state(false)

  function down(e: PointerEvent) {
    if (e.button !== 0) return
    e.preventDefault()
    const el = e.currentTarget as HTMLElement
    const pos = (ev: PointerEvent) => (dir === 'x' ? ev.clientX : ev.clientY)
    const start = pos(e)
    const from = key ? layout[key] : 0
    onstart?.()
    el.setPointerCapture(e.pointerId)
    dragging = true
    const move = (ev: PointerEvent) => {
      const delta = (pos(ev) - start) * (invert ? -1 : 1)
      if (key) layout[key] = Math.round(Math.min(max, Math.max(min, from + delta)))
      onresize?.(delta)
    }
    const end = () => {
      el.removeEventListener('pointermove', move)
      el.removeEventListener('pointerup', end)
      el.removeEventListener('pointercancel', end)
      el.removeEventListener('lostpointercapture', end)
      dragging = false
      if (key) saveLayout()
    }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', end)
    el.addEventListener('pointercancel', end)
    el.addEventListener('lostpointercapture', end)
  }

  /** 双击恢复默认尺寸 */
  function reset() {
    if (key) {
      layout[key] = DEFAULTS[key]
      saveLayout()
    }
    onreset?.()
  }

  function keydown(e: KeyboardEvent) {
    const delta = e.key === (dir === 'x' ? 'ArrowRight' : 'ArrowDown') ? 16
      : e.key === (dir === 'x' ? 'ArrowLeft' : 'ArrowUp') ? -16 : 0
    if (!delta && e.key !== 'Home') return
    e.preventDefault()
    if (e.key === 'Home') return reset()
    onstart?.()
    if (key) {
      layout[key] = Math.min(max, Math.max(min, layout[key] + delta * (invert ? -1 : 1)))
      saveLayout()
    }
    onresize?.(delta * (invert ? -1 : 1))
  }
</script>

<!-- A focusable separator supports arrow keys, as in the ARIA window splitter pattern. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div class="splitter {dir}" class:dragging onpointerdown={down} ondblclick={reset} onkeydown={keydown} role="separator" tabindex="0" aria-label={label ?? key} title={label} aria-valuenow={key ? layout[key] : value} aria-valuemin={min} aria-valuemax={Number.isFinite(max) ? max : 100} aria-orientation={dir === 'x' ? 'vertical' : 'horizontal'}></div>

<style>
  /* 看得见的是 1px 的线，能抓住的是它两侧各 4px */
  .splitter {
    position: relative;
    flex: none;
    background: var(--border);
    z-index: 2;
    touch-action: none;
  }
  .splitter::after {
    content: '';
    position: absolute;
  }
  .x {
    width: 1px;
    cursor: col-resize;
  }
  .x::after {
    inset: 0 -4px;
  }
  .y {
    height: 1px;
    cursor: row-resize;
  }
  .y::after {
    inset: -4px 0;
  }
  .splitter:hover,
  .splitter:focus-visible,
  .dragging {
    background: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
</style>
