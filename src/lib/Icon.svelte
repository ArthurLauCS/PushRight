<script lang="ts" module>
  // 24 网格的线性图标，只收用到的
  const PATHS = {
    undo: 'M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11',
    fetch: 'M21 12a9 9 0 1 1-2.6-6.4M21 3v6h-6',
    pull: 'M12 3v12m-5-5 5 5 5-5M5 21h14',
    push: 'M12 21V9m-5 5 5-5 5 5M5 3h14',
    stash: 'M3 6h18v5H3zM5 11v9h14v-9M10 15h4',
    branch: 'M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 9a9 9 0 0 1-9 9',
    log: 'm4 17 6-5-6-5M12 19h8',
    help: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01',
    settings: 'M4 7h16M4 17h16M9 4v6M15 14v6',
    plus: 'M12 5v14M5 12h14',
    minus: 'M5 12h14',
    close: 'M18 6 6 18M6 6l12 12',
    moon: 'M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z',
    sun: 'M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4',
    windows: 'M8 8h12v12H8zM4 16V4h12',
    changes: 'M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z',
    history: 'M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l4 2',
    folders: 'M3 7V5a2 2 0 0 1 2-2h4l2 3h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
    search: 'M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16zM21 21l-4.3-4.3',
    sync: 'M21 12a9 9 0 0 0-15-6.7L3 8M3 3v5h5M3 12a9 9 0 0 0 15 6.7l3-2.7M21 21v-5h-5',
    contrast: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 2v20M12 7h7.5M12 12h10M12 17h7.5',
    tools: 'M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94z',
    up: 'm6 15 6-6 6 6',
    down: 'm6 9 6 6 6-6',
    right: 'm9 6 6 6-6 6',
    filter: 'M3 5h18l-7 8v6l-4 2v-8z',
    space: 'M4 10v4h16v-4',
    pilcrow: 'M13 4v16M17 4v16M19 4H9.5a4.5 4.5 0 0 0 0 9H13',
    wrap: 'M3 6h18M3 12h15a3 3 0 1 1 0 6h-4m2-2-2 2 2 2M3 18h7',
    contextLess: 'M3 5h18M3 19h18M8 12h8',
    contextMore: 'M3 5h18M3 19h18M8 12h8M12 8v8',
    wholeFile: 'M3 4h18M3 20h18M12 8v8M9 11l3-3 3 3M9 13l3 3 3-3',
    split: 'M3 4h18v16H3zM12 4v16',
    tree: 'M21 12h-8M21 6H8M21 18h-8M3 6v4a2 2 0 0 0 2 2h3M3 10v6a2 2 0 0 0 2 2h3',
    expand: 'm7 15 5 5 5-5M7 9l5-5 5 5',
    collapse: 'm7 20 5-5 5 5M7 4l5 5 5-5',
  }
  export type IconName = keyof typeof PATHS
</script>

<script lang="ts">
  let { name, size = 16 }: { name: IconName; size?: number } = $props()
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d={PATHS[name]} />
</svg>

<style>
  svg {
    flex: none;
  }
</style>
