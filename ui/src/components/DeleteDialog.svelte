<script lang="ts">
  import { displayName } from '../lib/format';
  import type { DownloadRow } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    rows: DownloadRow[];
    onconfirm: (deleteFiles: boolean) => void;
    onclose: () => void;
  }

  let { rows, onconfirm, onclose }: Props = $props();

  let deleteFiles = $state(false);
  const finished = $derived(rows.filter((row) => row.state === 'completed').length);
  const unfinished = $derived(rows.length - finished);
</script>

<Modal title={rows.length === 1 ? 'Delete this download?' : `Delete ${rows.length} downloads?`} {onclose}>
  {#if rows.length === 1 && rows[0]}
    <p class="selectable"><strong>{displayName(rows[0])}</strong></p>
  {/if}

  {#if unfinished > 0}
    <p class="note">
      {unfinished === 1 ? 'Its' : 'Their'} partial data will be deleted and can't be recovered.
    </p>
  {/if}

  {#if finished > 0}
    <label class="check">
      <input type="checkbox" bind:checked={deleteFiles} />
      Also delete the finished {finished === 1 ? 'file' : 'files'} from disk
    </label>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    <button type="button" class="primary" onclick={() => onconfirm(deleteFiles)}>Delete</button>
  {/snippet}
</Modal>
