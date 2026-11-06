<script lang="ts">
  import { untrack } from 'svelte';

  import { LOGIN_NEEDED, inspectUrl, pickFolder, suggestCategory } from '../lib/api';
  import { formatBytes } from '../lib/format';
  import type { AddRequest, Category, FileInfo, Queue } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    url: string;
    defaultFolder: string;
    categories: Category[];
    queues: Queue[];
    onsubmit: (request: AddRequest) => void;
    onclose: () => void;
  }

  let { url, defaultFolder, categories, queues, onsubmit, onclose }: Props = $props();

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

  // The check finishes after the dialog opens, and must never overwrite what's been typed.
  let nameEdited = false;
  let folderEdited = false;
  let categoryEdited = false;

  async function check() {
    checking = true;
    problem = null;
    try {
      const found = await inspectUrl(url, username || undefined, password || undefined);
      info = found;
      if (!nameEdited) fileName = found.fileName;

      const suggestion = await suggestCategory(fileName || found.fileName, url);
      if (!categoryEdited) category = suggestion.name ?? '';
      if (!folderEdited) folder = suggestion.folder;
    } catch (error) {
      needsLogin = needsLogin || String(error) === LOGIN_NEEDED;
      problem = String(error);
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
    });
  }
</script>

<Modal title="Download file" wide {onclose}>
  <p class="selectable note">{url}</p>

  <div class="facts">
    {#if checking}
      <span>Checking the file…</span>
    {:else if info}
      <span>Size: <strong>{formatBytes(info.size)}</strong></span>
      <span>
        Resume:
        <strong>{info.resumable ? 'Supported' : 'Not supported'}</strong>
      </span>
    {:else}
      <span class="error-text">Couldn't check this file: {problem}</span>
    {/if}
  </div>

  {#if needsLogin}
    <div class="login">
      <div class="pair">
        <label class="field">
          User name
          <input type="text" bind:value={username} autocomplete="off" />
        </label>
        <label class="field">
          Password
          <input type="password" bind:value={password} autocomplete="off" />
        </label>
      </div>
      <div class="login-actions">
        <label class="check">
          <input type="checkbox" bind:checked={rememberLogin} />
          Remember for this site
        </label>
        <button type="button" disabled={checking || !username} onclick={check}>Try again</button>
      </div>
    </div>
  {/if}

  {#if info?.isWebPage}
    <p class="warning" role="alert">
      This address is a web page, not a file. Saving it keeps the page's text only, not any video
      or file shown on it.
    </p>
  {/if}

  <label class="field">
    Save as
    <input
      type="text"
      bind:value={fileName}
      placeholder="Name from the server"
      oninput={() => (nameEdited = true)}
    />
  </label>

  <div class="field-block">
    <label class="field" for="start-folder">Folder</label>
    <div class="folder-row">
      <input
        id="start-folder"
        type="text"
        bind:value={folder}
        oninput={() => (folderEdited = true)}
      />
      <button type="button" onclick={browse}>Browse…</button>
    </div>
  </div>

  <div class="pair">
    <label class="field">
      Category
      <select bind:value={category} onchange={changeCategory}>
        <option value="">None</option>
        {#each categories as item (item.name)}
          <option value={item.name}>{item.name}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      Queue for later
      <select bind:value={queueId}>
        {#each queues as queue (queue.id)}
          <option value={queue.id}>{queue.name}</option>
        {/each}
      </select>
    </label>
  </div>

  <label class="field">
    Description
    <input type="text" bind:value={description} placeholder="Optional" />
  </label>

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    <button type="button" disabled={checking} onclick={() => submit(true)}>Download later</button>
    <button type="button" class="primary" disabled={checking} onclick={() => submit(false)}>
      Start download
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

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
</style>
