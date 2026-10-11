<script lang="ts">
  import { DIFF_PRESETS, isDiffColor, validDiffColors, type DiffColors } from './diff-colors'
  import { diffColors, setDiffColors } from './diff-colors.svelte'
  import Icon from './Icon.svelte'
  import { t, errorText } from './i18n.svelte'

  let { editor = false, onsave = setDiffColors }: { editor?: boolean; onsave?: (colors: DiffColors | null) => void | Promise<void> } = $props()
  let dialog: HTMLDialogElement
  let colors = $state<DiffColors>({ ...DIFF_PRESETS.blueOrange })
  let useDefault = $state(true)
  let saving = $state(false)
  let error = $state('')
  const valid = $derived(validDiffColors(colors))
  const preset = $derived(useDefault ? 'default' : Object.entries(DIFF_PRESETS).find(([, value]) => value.added === colors.added && value.deleted === colors.deleted)?.[0] ?? 'custom')
  const defaults = (): DiffColors => ({ added: getComputedStyle(document.documentElement).getPropertyValue('--green').trim(), deleted: getComputedStyle(document.documentElement).getPropertyValue('--red').trim() })

  export function show() {
    colors = { ...(diffColors.current ?? defaults()) }
    useDefault = !diffColors.current
    error = ''
    dialog.showModal()
  }
  async function save() {
    saving = true
    error = ''
    try { await onsave(useDefault ? null : { ...colors }); dialog.close() }
    catch (e) { error = errorText(e) }
    finally { saving = false }
  }
</script>

<button type="button" class="btn quiet icon" title={t.diffColors.title} aria-label={t.diffColors.title} onclick={show}><Icon name="contrast" /></button>
<dialog bind:this={dialog} aria-labelledby="diff-color-title" oncancel={(event) => { if (saving) event.preventDefault() }}>
  <form onsubmit={(event) => { event.preventDefault(); if (valid && !saving) save() }}>
    <h2 id="diff-color-title">{t.diffColors.title}</h2>
    <p>{editor ? t.diffColors.editorHint : t.diffColors.hint}</p>
    <label>{t.diffColors.preset}
      <select class="field" value={preset} disabled={saving} onchange={(event) => {
        const key = event.currentTarget.value
        useDefault = key === 'default'
        if (useDefault) colors = defaults()
        if (key in DIFF_PRESETS) colors = { ...DIFF_PRESETS[key as keyof typeof DIFF_PRESETS] }
      }}>
        <option value="default">{t.diffColors.default}</option>
        {#each Object.keys(DIFF_PRESETS) as key}<option value={key}>{t.diffColors[key as keyof typeof DIFF_PRESETS]}</option>{/each}
        <option value="custom">{t.diffColors.custom}</option>
      </select>
    </label>
    {#each ['added', 'deleted'] as key}
      {@const side = key as keyof DiffColors}
      <label>{t.diffColors[side]}
        <span class="color">
          <input type="color" aria-label={t.diffColors[side]} value={isDiffColor(colors[side]) ? colors[side] : '#000000'} disabled={saving}
            oninput={(event) => { colors[side] = event.currentTarget.value; useDefault = false }} />
          <input class="field" aria-label={`${t.diffColors[side]} HEX`} bind:value={colors[side]} pattern={'#[0-9a-fA-F]{6}'} required maxlength="7" disabled={saving}
            oninput={() => useDefault = false} />
        </span>
      </label>
    {/each}
    <div class="preview" aria-label={t.diffColors.preview} style:--preview-added={useDefault ? 'var(--green)' : valid ? colors.added : 'var(--green)'}
      style:--preview-deleted={useDefault ? 'var(--red)' : valid ? colors.deleted : 'var(--red)'}>
      <div class="deleted"><strong>− {t.diffColors.deleted}</strong><code>const value = 1</code></div>
      <div class="added"><strong>+ {t.diffColors.added}</strong><code>const value = 2</code></div>
    </div>
    {#if !valid}<p class="error">{t.diffColors.invalid}</p>{/if}
    {#if !useDefault && colors.added.toLowerCase() === colors.deleted.toLowerCase()}<p>{t.diffColors.same}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="buttons">
      <button class="btn" type="button" disabled={saving} onclick={() => dialog.close()}>{t.cancel}</button>
      <button class="btn primary" type="submit" disabled={!valid || saving}>{t.ok}</button>
    </div>
  </form>
</dialog>

<style>
  dialog { width: 480px; max-width: 95vw; max-height: 90vh; overflow: auto; padding: 24px; color: var(--text); background: var(--panel);
    border: 1px solid var(--border-strong); border-radius: var(--r-lg); box-shadow: var(--shadow-pop); }
  dialog::backdrop { background: var(--backdrop); }
  form { display: flex; flex-direction: column; gap: 16px; }
  h2, p { margin: 0; }
  h2 { font-size: var(--fs-xl); font-weight: 700; }
  p { color: var(--muted); line-height: var(--lh-body); }
  label { display: flex; flex-direction: column; gap: 4px; color: var(--muted); font-size: var(--fs-sm); }
  .field { height: 32px; color: var(--text); font-size: var(--fs-md); }
  .color, .buttons { display: flex; gap: 8px; align-items: center; }
  .color .field { flex: 1; min-width: 0; }
  .buttons .btn { min-width: 80px; height: 32px; }
  input[type=color] { width: 48px; height: 32px; padding: 2px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--bg); }
  .preview { background: var(--bg); border: 1px solid var(--border); border-radius: var(--r-sm); overflow: hidden; }
  .preview div { padding: 8px; display: flex; gap: 16px; }
  .added { background: color-mix(in srgb, var(--preview-added) 22%, transparent); border-left: 4px solid var(--preview-added); }
  .deleted { background: color-mix(in srgb, var(--preview-deleted) 22%, transparent); border-left: 4px solid var(--preview-deleted); }
  .buttons { justify-content: flex-end; }
  .error { color: var(--red); }
</style>
