<script lang="ts">
  /** 文案末尾的「（git …）」拆出来单独排：名字是主角，命令退后；地方不够时由容器隐藏 .cmd */
  let { text }: { text: string } = $props()
  const at = $derived(text.endsWith('）') ? text.lastIndexOf('（git ') : -1)
</script>

{#if at < 0}{text}{:else}{text.slice(0, at)}<small class="cmd">{text.slice(at + 1, -1)}</small>{/if}
