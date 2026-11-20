<script lang="ts">
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
    });
  }
</script>

<Modal title="Properties" wide {onclose}>
  <dl>
    <dt>File</dt>
    <dd class="selectable">{displayName(row)}</dd>
    <dt>Status</dt>
    <dd>{statusText(row)}</dd>
    <dt>Size</dt>
    <dd>{formatBytes(row.size)}</dd>
    <dt>Added</dt>
    <dd>{formatDate(row.addedAt)}</dd>
    {#if row.completedAt}
      <dt>Finished</dt>
      <dd>{formatDate(row.completedAt)}</dd>
    {/if}
    {#if row.finalPath}
      <dt>Saved to</dt>
      <dd class="selectable">{row.finalPath}</dd>
    {/if}
    {#if row.checksum}
      <dt>Checksum</dt>
      <dd class:error-text={row.checksumMatches === false}>{checksumText(row)}</dd>
    {/if}
  </dl>

  {#if row.error}
    <p class="error-text">{row.error}</p>
  {/if}

  <label class="field">
    Address
    <input type="url" bind:value={url} disabled={running || completed} />
  </label>

  <div class="field-block">
    <label class="field" for="properties-folder">Folder</label>
    <div class="folder-row">
      <input id="properties-folder" type="text" bind:value={folder} disabled={running || completed} />
      <button type="button" disabled={running || completed} onclick={browse}>Browse…</button>
    </div>
  </div>

  <label class="field">
    Description
    <input type="text" bind:value={description} disabled={running} />
  </label>

  <label class="field">
    Checksum
    <input
      type="text"
      bind:value={checksum}
      disabled={running}
      placeholder="Optional: the MD5, SHA-1, SHA-256 or SHA-512 the site gives"
      spellcheck="false"
      autocomplete="off"
    />
  </label>
  {#if !checksumOk}
    <p class="error-text" role="alert">{NOT_A_CHECKSUM}</p>
  {/if}

  {#if running}
    <p class="note">Stop the download to change these.</p>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    <button type="button" class="primary" disabled={running || !checksumOk} onclick={save}>
      Save
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
