<script lang="ts">
  import { explain } from './i18n.svelte'
  import Explainer from './Explainer.svelte'
  import Label from './Label.svelte'
  import { t } from './i18n.svelte'

  let dialog: HTMLDialogElement
  let reset = $state(false)

  export function open() {
    reset = false
    dialog.showModal()
  }

  /** 让所有勾过「下次不再显示」的说明重新出现 */
  function showAgain() {
    for (const key of Object.keys(explain)) localStorage.removeItem(`skip:${key}`)
    reset = true
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} onclick={(e) => e.target === dialog && dialog.close()}>
  <header>
    <h2>{t.help}</h2>
    <button class="btn small" disabled={reset} onclick={showAgain}>{reset ? t.helpResetDone : t.helpReset}</button>
    <button class="btn small" onclick={() => dialog.close()}>{t.close}</button>
  </header>
  <p class="legend">{t.helpLegend}</p>
  <div class="list">
    {#each Object.values(explain) as e (e.name)}
      <section>
        <h3><Label text={e.name} /></h3>
        <Explainer explain={e} />
      </section>
    {/each}
  </div>
</dialog>

<style>
  dialog {
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-lg);
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-pop);
  }
  dialog::backdrop {
    background: var(--backdrop);
  }

  dialog {
    width: 680px;
    max-height: 86vh;
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 24px 24px 8px;
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .legend {
    flex: none;
    margin: 0;
    padding: 0 24px 16px;
    color: var(--muted);
    font-family: var(--prose);
    font-size: var(--fs-md);
    line-height: var(--lh-body);
    border-bottom: 1px solid var(--border);
  }
  .list {
    overflow-y: auto;
    padding: 0 24px 24px;
    user-select: text;
  }
  section {
    padding: 24px 0;
    border-bottom: 1px solid var(--border);
  }
  section:last-child {
    border-bottom: 0;
  }
  h3 {
    margin: 0 0 12px;
    color: var(--accent);
    font-size: var(--fs-xl);
    font-weight: 700;
  }
</style>
