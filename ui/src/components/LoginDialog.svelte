<script lang="ts">
  import { displayName } from '../lib/format';
  import type { DownloadRow } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    row: DownloadRow;
    onsubmit: (username: string, password: string, remember: boolean) => void;
    onclose: () => void;
  }

  let { row, onsubmit, onclose }: Props = $props();

  let username = $state('');
  let password = $state('');
  let remember = $state(false);

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (username.trim()) onsubmit(username.trim(), password, remember);
  }
</script>

<Modal title="Login needed" {onclose}>
  <p><strong>{displayName(row)}</strong> needs a user name and password.</p>
  <p class="note selectable">{row.url}</p>

  <form id="download-login" onsubmit={submit}>
    <label class="field">
      User name
      <!-- svelte-ignore a11y_autofocus -->
      <input type="text" bind:value={username} autocomplete="off" autofocus />
    </label>
    <label class="field">
      Password
      <input type="password" bind:value={password} autocomplete="off" />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={remember} />
      Remember for this site
    </label>
  </form>

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    <button type="submit" form="download-login" class="primary" disabled={!username.trim()}>
      Try again
    </button>
  {/snippet}
</Modal>
