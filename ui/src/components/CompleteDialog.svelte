<script lang="ts">
  import { t } from '../lib/locale.svelte';
  import { displayName, formatBytes } from '../lib/format';
  import type { DownloadRow } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    row: DownloadRow;
    onopen: () => void;
    onfolder: () => void;
    onclose: (hideInFuture: boolean) => void;
  }

  let { row, onopen, onfolder, onclose }: Props = $props();

  let hideInFuture = $state(false);
</script>

<Modal title={t('Download complete')} onclose={() => onclose(hideInFuture)}>
  <p><strong>{displayName(row)}</strong> ({formatBytes(row.size)})</p>
  <p class="note selectable">{row.finalPath}</p>
  {#if row.checksum && row.checksumMatches === true}
    <p>
      {t('The {algorithm} checksum matches: this is the file the site published.', {
        algorithm: row.checksum.algorithm,
      })}
    </p>
  {:else if row.checksum && row.checksumMatches === false}
    <p class="error-text" role="alert">
      {t(
        "The {algorithm} checksum doesn't match. The file may be damaged, or not the one the site meant. Download it again before opening it.",
        { algorithm: row.checksum.algorithm },
      )}
    </p>
  {/if}
  <label class="check">
    <input type="checkbox" bind:checked={hideInFuture} />
    {t("Don't show this again")}
  </label>

  {#snippet footer()}
    <button type="button" onclick={() => onclose(hideInFuture)}>{t('Close')}</button>
    <button type="button" onclick={onfolder}>{t('Open folder')}</button>
    <button type="button" class="primary" onclick={onopen}>{t('Open file')}</button>
  {/snippet}
</Modal>
