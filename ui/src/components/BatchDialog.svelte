<script lang="ts">
  import { untrack } from 'svelte';

  import { pickFolder } from '../lib/api';
  import { displayName } from '../lib/format';
  import type { CaptureItem, Queue } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    items: CaptureItem[];
    defaultFolder: string;
    queues: Queue[];
    onsubmit: (urls: string[], folder: string, queueId: number | null, later: boolean) => void;
    onclose: () => void;
  }

  let { items, defaultFolder, queues, onsubmit, onclose }: Props = $props();

  let chosen = $state(new Set(untrack(() => items.map((item) => item.url))));
  let filter = $state('');
  let folder = $state(untrack(() => defaultFolder));
  let queueId = $state(1);

  const shown = $derived(
    items.filter((item) => item.url.toLowerCase().includes(filter.trim().toLowerCase())),
  );
  const chosenShown = $derived(shown.filter((item) => chosen.has(item.url)));

  function toggle(url: string) {
    const next = new Set(chosen);
    if (!next.delete(url)) next.add(url);
    chosen = next;
  }

  function setShown(selected: boolean) {
    const next = new Set(chosen);
    for (const item of shown) {
      if (selected) next.add(item.url);
      else next.delete(item.url);
    }
    chosen = next;
  }

  async function browse() {
    const picked = await pickFolder(folder);
    if (picked) folder = picked;
  }

  function submit(later: boolean) {
    const urls = chosenShown.map((item) => item.url);
    onsubmit(urls, folder, later ? queueId : null, later);
  }
</script>

<Modal title="Download links" wide {onclose}>
  <label class="field">
    Only show addresses containing
    <input type="text" bind:value={filter} placeholder=".zip" />
  </label>

  <div class="actions">
    <span class="note">{chosenShown.length} of {items.length} chosen</span>
    <span>
      <button type="button" onclick={() => setShown(true)}>Select all</button>
      <button type="button" onclick={() => setShown(false)}>Select none</button>
    </span>
  </div>

  <ul>
    {#each shown as item (item.url)}
      <li>
        <label class="check" title={item.url}>
          <input type="checkbox" checked={chosen.has(item.url)} onchange={() => toggle(item.url)} />
          <span class="name">{displayName({ fileName: item.fileName, url: item.url })}</span>
        </label>
      </li>
    {/each}
  </ul>

  <div class="block">
    <label class="field" for="batch-folder">Folder</label>
    <div class="folder-row">
      <input id="batch-folder" type="text" bind:value={folder} />
      <button type="button" onclick={browse}>Browse…</button>
    </div>
  </div>

  <label class="field">
    Queue for later
    <select bind:value={queueId}>
      {#each queues as queue (queue.id)}
        <option value={queue.id}>{queue.name}</option>
      {/each}
    </select>
  </label>

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    <button type="button" disabled={chosenShown.length === 0} onclick={() => submit(true)}>
      Download later
    </button>
    <button
      type="button"
      class="primary"
      disabled={chosenShown.length === 0}
      onclick={() => submit(false)}
    >
      Start now
    </button>
  {/snippet}
</Modal>

<style>
  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 6px;
  }

  ul {
    max-height: 220px;
    margin: 0 0 12px;
    padding: 6px 8px;
    overflow-y: auto;
    list-style: none;
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  li .check {
    margin-bottom: 2px;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .block {
    margin-bottom: 12px;
  }

  .block .field {
    margin-bottom: 4px;
  }
</style>
