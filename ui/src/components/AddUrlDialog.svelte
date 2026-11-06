<script lang="ts">
  import { isDownloadableUrl } from '../lib/format';
  import Modal from './Modal.svelte';

  interface Props {
    history: string[];
    onsubmit: (url: string) => void;
    onclose: () => void;
  }

  let { history, onsubmit, onclose }: Props = $props();

  let url = $state('');
  const valid = $derived(isDownloadableUrl(url));

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (valid) onsubmit(url.trim());
  }
</script>

<Modal title="Add URL" {onclose}>
  <form id="add-url" onsubmit={submit}>
    <label class="field">
      Address
      <!-- svelte-ignore a11y_autofocus -->
      <input type="url" list="url-history" bind:value={url} placeholder="https://" autofocus />
    </label>
    <datalist id="url-history">
      {#each history as item (item)}
        <option value={item}></option>
      {/each}
    </datalist>
    {#if url.trim() && !valid}
      <p class="note">Enter a full address starting with http:// or https://</p>
    {/if}
  </form>

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    <button type="submit" form="add-url" class="primary" disabled={!valid}>Continue</button>
  {/snippet}
</Modal>
