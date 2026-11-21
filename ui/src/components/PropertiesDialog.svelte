<script lang="ts">
  import { sayError, t } from '../lib/locale.svelte';
  import { untrack } from 'svelte';

  import { pickFolder } from '../lib/api';
  import { NOT_A_CHECKSUM, looksLikeChecksum } from '../lib/checksum';
  import { checksumText, displayName, formatBytes, formatDate, statusText } from '../lib/format';
  import type { DownloadRow, EditRequest } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    row: DownloadRow;
    onsave: (edit: EditRequest) => void;
    onclose: () => void;
  }

  let { row, onsave, onclose }: Props = $props();

  // Read once when the dialog opens, so list updates can't overwrite what's being typed.
  let url = $state(untrack(() => row.url));
  let folder = $state(untrack(() => row.destDir));
  let description = $state(untrack(() => row.description ?? ''));
  let checksum = $state(untrack(() => row.checksum?.hex ?? ''));
  let mirrors = $state(untrack(() => (row.mirrors ?? []).join('\n')));

  /** The mirror addresses typed, one a line. */
  const mirrorLines = (text: string) =>
    text
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
  const checksumOk = $derived(looksLikeChecksum(checksum));

  const running = $derived(row.state === 'running');
  const completed = $derived(row.state === 'completed');

  async function browse() {
    const chosen = await pickFolder(folder);
    if (chosen) folder = chosen;
  }

  function save() {
    onsave({
      url: url === row.url ? undefined : url,
      destDir: folder === row.destDir ? undefined : folder,
      description: description === (row.description ?? '') ? undefined : description,
      checksum: checksum.trim() === (row.checksum?.hex ?? '') ? undefined : checksum.trim(),
      mirrors:
        mirrorLines(mirrors).join('\n') === (row.mirrors ?? []).join('\n')
          ? undefined
          : mirrorLines(mirrors),
    });
  }
</script>

<Modal title={t('Properties')} wide {onclose}>
  <dl>
    <dt>{t('File')}</dt>
    <dd class="selectable">{displayName(row)}</dd>
    <dt>{t('Status')}</dt>
    <dd>{statusText(row)}</dd>
    <dt>{t('Size')}</dt>
    <dd>{formatBytes(row.size)}</dd>
    <dt>{t('Added')}</dt>
    <dd>{formatDate(row.addedAt)}</dd>
    {#if row.completedAt}
      <dt>{t('Finished')}</dt>
      <dd>{formatDate(row.completedAt)}</dd>
    {/if}
    {#if row.finalPath}
      <dt>{t('Saved to')}</dt>
      <dd class="selectable">{row.finalPath}</dd>
    {/if}
    {#if row.checksum}
      <dt>{t('Checksum')}</dt>
      <dd class:error-text={row.checksumMatches === false}>{checksumText(row)}</dd>
    {/if}
  </dl>

  {#if row.error}
    <p class="error-text">{sayError(row.error)}</p>
  {/if}

  <label class="field">
    {t('Address')}
    <input type="url" bind:value={url} disabled={running || completed} />
  </label>

  <div class="field-block">
    <label class="field" for="properties-folder">{t('Folder')}</label>
    <div class="folder-row">
      <input id="properties-folder" type="text" bind:value={folder} disabled={running || completed} />
      <button type="button" disabled={running || completed} onclick={browse}>{t('Browse…')}</button>
    </div>
  </div>

  <label class="field">
    {t('Description')}
    <input type="text" bind:value={description} disabled={running} />
  </label>

  <label class="field">
    {t('Checksum')}
    <input
      type="text"
      bind:value={checksum}
      disabled={running}
      placeholder={t('Optional: the MD5, SHA-1, SHA-256 or SHA-512 the site gives')}
      spellcheck="false"
      autocomplete="off"
    />
  </label>
  {#if !checksumOk}
    <p class="error-text" role="alert">{NOT_A_CHECKSUM}</p>
  {/if}

  <label class="field">
    {t('Mirrors')}
    <textarea
      rows="2"
      bind:value={mirrors}
      disabled={running || completed}
      placeholder={t('Other addresses for the same file, one a line')}
      spellcheck="false"
    ></textarea>
  </label>
  <p class="note">
    {t("Parts of the file are fetched from these too. A mirror that fails, or has a file of another size, is left out. Cookies and logins only go to the file's own site.")}
  </p>

  {#if running}
    <p class="note">{t('Stop the download to change these.')}</p>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={onclose}>{t('Cancel')}</button>
    <button type="button" class="primary" disabled={running || !checksumOk} onclick={save}>
      {t('Save')}
    </button>
  {/snippet}
</Modal>

<style>
  dl {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 4px 12px;
    margin: 0 0 14px;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
  }

  .field-block {
    margin-bottom: 12px;
  }

  .field-block .field {
    margin-bottom: 4px;
  }
</style>
