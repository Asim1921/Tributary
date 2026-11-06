<script lang="ts">
  import { inspectUrl, pickFolder } from '../lib/api';
  import { formatBytes } from '../lib/format';
  import type { AddRequest, FileInfo } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    url: string;
    defaultFolder: string;
    onsubmit: (request: AddRequest) => void;
    onclose: () => void;
  }

  let { url, defaultFolder, onsubmit, onclose }: Props = $props();

  let info = $state<FileInfo | null>(null);
  let problem = $state<string | null>(null);
  let checking = $state(true);
  let fileName = $state('');
  let folder = $state('');
  let description = $state('');

  $effect(() => {
    folder = defaultFolder;
    inspectUrl(url)
      .then((found) => {
        info = found;
        fileName = found.fileName;
      })
      .catch((error) => {
        problem = String(error);
      })
      .finally(() => {
        checking = false;
      });
  });

  async function browse() {
    const chosen = await pickFolder(folder);
    if (chosen) folder = chosen;
  }

  function submit(paused: boolean) {
    onsubmit({ url, destDir: folder, fileName, description, paused });
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

  <label class="field">
    Save as
    <input type="text" bind:value={fileName} placeholder="Name from the server" />
  </label>

  <div class="field-block">
    <label class="field" for="start-folder">Folder</label>
    <div class="folder-row">
      <input id="start-folder" type="text" bind:value={folder} />
      <button type="button" onclick={browse}>Browse…</button>
    </div>
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

  .field-block {
    margin-bottom: 12px;
  }

  .field-block .field {
    margin-bottom: 4px;
  }
</style>
