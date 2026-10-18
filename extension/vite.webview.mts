import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

// 侧栏的紧凑提交图：用 src/lib 里的提交图组件，引擎调用发给扩展主机
export default defineConfig({
  plugins: [svelte()],
  base: './',
  build: {
    outDir: 'extension/media',
    emptyOutDir: true,
    cssCodeSplit: false,
    // 文件名不带哈希，扩展主机按固定名字引用
    rollupOptions: {
      input: { sidebar: 'extension/webview/sidebar.ts' },
      output: { entryFileNames: '[name].js', chunkFileNames: '[name].js', assetFileNames: '[name][extname]' },
    },
  },
})
