<script lang="ts">
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

<Modal title="Download complete" onclose={() => onclose(hideInFuture)}>
  <p><strong>{displayName(row)}</strong> ({formatBytes(row.size)})</p>
  <p class="note selectable">{row.finalPath}</p>
  {#if row.checksum && row.checksumMatches === true}
    <p>The {row.checksum.algorithm} checksum matches: this is the file the site published.</p>
  {:else if row.checksum && row.checksumMatches === false}
    <p class="error-text" role="alert">
      The {row.checksum.algorithm} checksum doesn't match. The file may be damaged, or not the
      one the site meant. Download it again before opening it.
    </p>
  {/if}
  <label class="check">
    <input type="checkbox" bind:checked={hideInFuture} />
    Don't show this again
  </label>

  {#snippet footer()}
    <button type="button" onclick={() => onclose(hideInFuture)}>Close</button>
    <button type="button" onclick={onfolder}>Open folder</button>
    <button type="button" class="primary" onclick={onopen}>Open file</button>
  {/snippet}
</Modal>
