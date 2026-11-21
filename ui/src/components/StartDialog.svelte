<script lang="ts">
  import { sayError, t } from '../lib/locale.svelte';
  import { untrack } from 'svelte';

  import { LOGIN_NEEDED, findChecksum, inspectUrl, pickFolder, suggestCategory } from '../lib/api';
  import { NOT_A_CHECKSUM, looksLikeChecksum } from '../lib/checksum';
  import { formatBytes } from '../lib/format';
  import type { AddRequest, Category, FileInfo, Queue } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    url: string;
    captureToken?: number;
    suggestedName?: string | null;
    defaultFolder: string;
    categories: Category[];
    queues: Queue[];
    onsubmit: (request: AddRequest) => void;
    /** The address is a stream, or a page that may hold a video. Both have a dialog of
     * their own. */
    onstream: (page?: boolean) => void;
    onclose: () => void;
  }

  let {
    url,
    captureToken,
    suggestedName,
    defaultFolder,
    categories,
    queues,
    onsubmit,
    onstream,
    onclose,
  }: Props = $props();

  let info = $state<FileInfo | null>(null);
  let problem = $state<string | null>(null);
  let checking = $state(true);
  let fileName = $state('');
  let folder = $state(untrack(() => defaultFolder));
  let description = $state('');
  let category = $state('');
  let queueId = $state(1);
  let needsLogin = $state(false);
  let username = $state('');
  let password = $state('');
  let rememberLogin = $state(false);
  let checksum = $state('');
  let finding = $state(false);
  let found = $state<string | null>(null);
  const checksumOk = $derived(looksLikeChecksum(checksum));

  // The check finishes after the dialog opens, and must never overwrite what's been typed.
  let nameEdited = false;
  let folderEdited = false;
  let categoryEdited = false;

  async function check() {
    checking = true;
    problem = null;
    try {
      const found = await inspectUrl(
        url,
        username || undefined,
        password || undefined,
        captureToken,
      );
      if (found.isStream) {
        onstream(false);
        return;
      }

      info = found;
      if (!nameEdited) fileName = suggestedName || found.fileName;

      const suggestion = await suggestCategory(fileName || found.fileName, url);
      if (!categoryEdited) category = suggestion.name ?? '';
      if (!folderEdited) folder = suggestion.folder;
    } catch (error) {
      needsLogin = needsLogin || String(error) === LOGIN_NEEDED;
      problem = sayError(error);
    } finally {
      checking = false;
    }
  }

  $effect(() => {
    untrack(() => void check());
  });

  function changeCategory() {
    categoryEdited = true;
    if (folderEdited) return;
    const chosen = categories.find((item) => item.name === category);
    folder = chosen?.folder ?? defaultFolder;
  }

  async function browse() {
    const chosen = await pickFolder(folder);
    if (chosen) {
      folder = chosen;
      folderEdited = true;
    }
  }

  /** Looks for a checksum the site publishes beside the file. */
  async function find() {
    finding = true;
    found = null;
    try {
      const result = await findChecksum(url, fileName || undefined);
      if (result) {
        checksum = result.hex;
        found = t('Found the {algorithm} checksum the site publishes for this file.', {
          algorithm: result.algorithm,
        });
      } else {
        found = t("The site doesn't publish a checksum beside this file.");
      }
    } catch (error) {
      found = t("Couldn't look for one: {error}", { error: sayError(error) });
    } finally {
      finding = false;
    }
  }

  function submit(later: boolean) {
    onsubmit({
      url,
      destDir: folder,
      fileName,
      description,
      category: category || null,
      paused: later,
      queueId: later ? queueId : null,
      username: username || undefined,
      password: password || undefined,
      rememberLogin,
      captureToken,
      checksum: checksum.trim() || undefined,
    });
  }
</script>

<Modal title={t('Download file')} wide {onclose}>
  <p class="selectable note">{url}</p>

  <div class="facts">
    {#if checking}
      <span>{t('Checking the file…')}</span>
    {:else if info}
      <span>{t('Size:')} <strong>{formatBytes(info.size)}</strong></span>
      <span>
        {t('Resume:')}
        <strong>{info.resumable ? t('Supported') : t('Not supported')}</strong>
      </span>
    {:else}
      <span class="error-text">{t("Couldn't check this file: {problem}", { problem })}</span>
    {/if}
  </div>

  {#if needsLogin}
    <div class="login">
      <div class="pair">
        <label class="field">
          {t('User name')}
          <input type="text" bind:value={username} autocomplete="off" />
        </label>
        <label class="field">
          {t('Password')}
          <input type="password" bind:value={password} autocomplete="off" />
        </label>
      </div>
      <div class="login-actions">
        <label class="check">
          <input type="checkbox" bind:checked={rememberLogin} />
          {t('Remember for this site')}
        </label>
        <button type="button" disabled={checking || !username} onclick={check}>{t('Try again')}</button>
      </div>
    </div>
  {/if}

  {#if info?.isWebPage}
    <p class="warning" role="alert">
      {t("This address is a web page, not a file. Saving it keeps the page's text only, not any video or file shown on it.")}
    </p>
    <p>
      <button type="button" onclick={() => onstream(true)}>{t('Look for a video on this page')}</button>
    </p>
  {/if}

  <label class="field">
    {t('Save as')}
    <input
      type="text"
      bind:value={fileName}
      placeholder={t('Name from the server')}
      oninput={() => (nameEdited = true)}
    />
  </label>

  <div class="field-block">
    <label class="field" for="start-folder">{t('Folder')}</label>
    <div class="folder-row">
      <input
        id="start-folder"
        type="text"
        bind:value={folder}
        oninput={() => (folderEdited = true)}
      />
      <button type="button" onclick={browse}>{t('Browse…')}</button>
    </div>
  </div>

  <div class="pair">
    <label class="field">
      {t('Category')}
      <select bind:value={category} onchange={changeCategory}>
        <option value="">{t('None')}</option>
        {#each categories as item (item.name)}
          <option value={item.name}>{t(item.name)}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      {t('Queue for later')}
      <select bind:value={queueId}>
        {#each queues as queue (queue.id)}
          <option value={queue.id}>{t(queue.name)}</option>
        {/each}
      </select>
    </label>
  </div>

  <label class="field">
    {t('Description')}
    <input type="text" bind:value={description} placeholder={t('Optional')} />
  </label>

  <div class="field-block">
    <label class="field" for="start-checksum">{t('Checksum')}</label>
    <div class="folder-row">
      <input
        id="start-checksum"
        type="text"
        bind:value={checksum}
        placeholder={t('Optional: the MD5, SHA-1, SHA-256 or SHA-512 the site gives')}
        spellcheck="false"
        autocomplete="off"
      />
      <button type="button" disabled={finding} onclick={find}>
        {finding ? t('Looking…') : t('Find')}
      </button>
    </div>
    {#if !checksumOk}
      <p class="error-text found" role="alert">{NOT_A_CHECKSUM}</p>
    {:else if found}
      <p class="note found" role="status">{found}</p>
    {/if}
  </div>

  {#snippet footer()}
    <button type="button" onclick={onclose}>{t('Cancel')}</button>
    <button type="button" disabled={checking || !checksumOk} onclick={() => submit(true)}>
      {t('Download later')}
    </button>
    <button
      type="button"
      class="primary"
      disabled={checking || !checksumOk}
      onclick={() => submit(false)}
    >
      {t('Start download')}
    </button>
  {/snippet}
</Modal>

<style>
  .facts {
    display: flex;
    gap: 24px;
    margin-bottom: 14px;
  }

  .warning {
    margin: 0 0 14px;
    padding: 8px 10px;
    color: var(--warn);
    border: 1px solid var(--warn);
    border-radius: 6px;
  }

  .login {
    margin-bottom: 12px;
    padding: 10px 12px 2px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .login-actions {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .field-block {
    margin-bottom: 12px;
  }

  .field-block .field {
    margin-bottom: 4px;
  }

  .found {
    margin: 4px 0 0;
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
</style>
