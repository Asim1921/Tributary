<script lang="ts">
  import { untrack } from 'svelte';

  import { addTorrent, inspectTorrent, pickFolder } from '../lib/api';
  import { formatBytes } from '../lib/format';
  import type { TorrentPreview } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    url: string;
    defaultFolder: string;
    onadded: () => void;
    onclose: () => void;
  }

  let { url, defaultFolder, onadded, onclose }: Props = $props();

  let preview = $state<TorrentPreview | null>(null);
  let problem = $state<string | null>(null);
  let chosen = $state<Set<number>>(new Set());
  let folder = $state(untrack(() => defaultFolder));
  let keepSharing = $state(false);
  let adding = $state(false);

  const chosenSize = $derived(
    (preview?.files ?? []).reduce((sum, file, index) => (chosen.has(index) ? sum + file.size : sum), 0),
  );

  $effect(() => {
    untrack(() => {
      inspectTorrent(url)
        .then((found) => {
          preview = found;
          chosen = new Set(found.files.map((_, index) => index));
        })
        .catch((error) => (problem = String(error)));
    });
  });

  function toggle(index: number) {
    const next = new Set(chosen);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    chosen = next;
  }

  async function browse() {
    const picked = await pickFolder(folder);
    if (picked) folder = picked;
  }

  async function add(later: boolean) {
    adding = true;
    try {
      await addTorrent([...chosen].sort((a, b) => a - b), folder, keepSharing, later);
      onadded();
    } catch (error) {
      problem = String(error);
      adding = false;
    }
  }
</script>

<Modal title={preview ? preview.name : 'Torrent'} wide {onclose}>
  <p class="selectable note address">{url}</p>

  {#if problem}
    <p class="refused" role="alert">{problem}</p>
  {:else if !preview}
    <p>Reading the torrent…</p>
  {:else}
    <div class="tools">
      <span>{chosen.size} of {preview.files.length} files, {formatBytes(chosenSize)}</span>
      <button type="button" onclick={() => (chosen = new Set(preview?.files.map((_, index) => index)))}>
        Choose all
      </button>
      <button type="button" onclick={() => (chosen = new Set())}>Choose none</button>
    </div>
    <ul class="files">
      {#each preview.files as file, index (index)}
        <li>
          <label class="check">
            <input type="checkbox" checked={chosen.has(index)} onchange={() => toggle(index)} />
            <span class="name">{file.name}</span>
            <span class="muted">{formatBytes(file.size)}</span>
          </label>
        </li>
      {/each}
    </ul>

    <div class="field-block">
      <label class="field" for="torrent-folder">Folder</label>
      <div class="folder-row">
        <input id="torrent-folder" type="text" bind:value={folder} />
        <button type="button" onclick={browse}>Browse…</button>
      </div>
    </div>

    <label class="check">
      <input type="checkbox" bind:checked={keepSharing} />
      Keep sharing until I've sent as much as I got
    </label>
    <p class="note">
      A torrent shares what it has with others while it downloads. Unless you tick the box above,
      it stops once it has finished. Only download what you have the right to.
    </p>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={onclose}>{problem ? 'Close' : 'Cancel'}</button>
    {#if !problem}
      <button type="button" disabled={!preview || chosen.size === 0 || adding} onclick={() => add(true)}>
        Download later
      </button>
      <button
        type="button"
        class="primary"
        disabled={!preview || chosen.size === 0 || adding}
        onclick={() => add(false)}
      >
        Download
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .address {
    overflow-wrap: anywhere;
  }

  .tools {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-bottom: 8px;
  }

  .tools span {
    margin-inline-end: auto;
  }

  .files {
    max-height: 280px;
    margin: 0 0 12px;
    padding: 4px 8px;
    overflow-y: auto;
    list-style: none;
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .name {
    overflow-wrap: anywhere;
  }

  .muted {
    margin-inline-start: 8px;
    color: var(--muted);
  }

  .refused {
    margin: 0 0 14px;
    padding: 8px 10px;
    color: var(--warn);
    border: 1px solid var(--warn);
    border-radius: 6px;
  }

  .field-block {
    margin-bottom: 12px;
  }

  .field-block .field {
    margin-bottom: 4px;
  }
</style>
