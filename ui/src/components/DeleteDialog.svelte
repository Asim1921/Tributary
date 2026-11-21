<script lang="ts">
  import { t } from '../lib/locale.svelte';
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

<Modal
  title={t('{count, plural, one {Delete this download?} other {Delete # downloads?}}', {
    count: rows.length,
  })}
  {onclose}
>
  {#if rows.length === 1 && rows[0]}
    <p class="selectable"><strong>{displayName(rows[0])}</strong></p>
  {/if}

  {#if unfinished > 0}
    <p class="note">
      {t(
        "{count, plural, one {Its partial data will be deleted and can't be recovered.} other {Their partial data will be deleted and can't be recovered.}}",
        { count: unfinished },
      )}
    </p>
  {/if}

  {#if finished > 0}
    <label class="check">
      <input type="checkbox" bind:checked={deleteFiles} />
      {t(
        '{count, plural, one {Also delete the finished file from disk} other {Also delete the finished files from disk}}',
        { count: finished },
      )}
    </label>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={onclose}>{t('Cancel')}</button>
    <button type="button" class="primary" onclick={() => onconfirm(deleteFiles)}>{t('Delete')}</button>
  {/snippet}
</Modal>
