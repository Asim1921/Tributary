<script lang="ts">
  import { t, tMarkup } from '../lib/locale.svelte';
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

<Modal title={t('Login needed')} {onclose}>
  <p>
    <!-- eslint-disable-next-line svelte/no-at-html-tags -- the app's own text, with the name escaped -->
    {@html tMarkup('<strong>{name}</strong> needs a user name and password.', { name: displayName(row) })}
  </p>
  <p class="note selectable">{row.url}</p>

  <form id="download-login" autocomplete="off" onsubmit={submit}>
    <label class="field">
      {t('User name')}
      <!-- svelte-ignore a11y_autofocus -->
      <input type="text" bind:value={username} autocomplete="off" autofocus />
    </label>
    <label class="field">
      {t('Password')}
      <input type="password" bind:value={password} autocomplete="off" />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={remember} />
      {t('Remember for this site')}
    </label>
  </form>

  {#snippet footer()}
    <button type="button" onclick={onclose}>{t('Cancel')}</button>
    <button type="submit" form="download-login" class="primary" disabled={!username.trim()}>
      {t('Try again')}
    </button>
  {/snippet}
</Modal>
