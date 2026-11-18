<script lang="ts">
  import { untrack } from 'svelte';

  import { pickFolder } from '../lib/api';
  import { folderForPage, kindCounts, kindLabel } from '../lib/batch';
  import { displayName } from '../lib/format';
  import type { CaptureItem, Queue } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    items: CaptureItem[];
    defaultFolder: string;
    queues: Queue[];
    /** For every picture and video on a page, that page's title. */
    title?: string | null;
    /** Whether these are every picture and video on a page. */
    gallery?: boolean;
    onsubmit: (urls: string[], folder: string, queueId: number | null, later: boolean) => void;
    onclose: () => void;
  }

  let { items, defaultFolder, queues, title, gallery, onsubmit, onclose }: Props = $props();

  let chosen = $state(new Set(untrack(() => items.map((item) => item.url))));
  let filter = $state('');
  let ownFolder = $state(untrack(() => Boolean(gallery)));
  // The folder picked or typed, which the page's own folder goes inside when that's on.
  let baseFolder = $state(untrack(() => defaultFolder));
  let folder = $state(untrack(() => (gallery ? folderForPage(defaultFolder, title) : defaultFolder)));
  let queueId = $state(1);
  let pictures = $state(true);
  let videos = $state(true);

  const counts = $derived(kindCounts(items));
  const wanted = (item: CaptureItem) =>
    !gallery ||
    (item.kind === 'image' ? pictures : item.kind === 'video' || item.kind === 'page' ? videos : true);
  const shown = $derived(
    items.filter(
      (item) => wanted(item) && item.url.toLowerCase().includes(filter.trim().toLowerCase()),
    ),
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

  function setOwnFolder(on: boolean) {
    ownFolder = on;
    folder = on ? folderForPage(baseFolder, title) : baseFolder;
  }

  // A folder typed in is taken as it is.
  function typeFolder(typed: string) {
    baseFolder = typed;
    folder = typed;
    ownFolder = false;
  }

  async function browse() {
    const picked = await pickFolder(baseFolder);
    if (!picked) return;
    baseFolder = picked;
    folder = ownFolder ? folderForPage(picked, title) : picked;
  }

  function submit(later: boolean) {
    const urls = chosenShown.map((item) => item.url);
    onsubmit(urls, folder, later ? queueId : null, later);
  }
</script>

<Modal title={gallery ? 'Save pictures and videos' : 'Download links'} wide {onclose}>
  {#if gallery}
    <p class="lede">
      {#if title}<strong>{title}</strong>: {/if}{counts.pictures} pictures and {counts.videos} videos
      found on the page.
    </p>
    <div class="kinds">
      <label class="check">
        <input id="batch-pictures" type="checkbox" bind:checked={pictures} />
        Pictures ({counts.pictures})
      </label>
      <label class="check">
        <input id="batch-videos" type="checkbox" bind:checked={videos} />
        Videos ({counts.videos})
      </label>
    </div>
  {/if}

  <label class="field">
    Only show addresses containing
    <input type="text" bind:value={filter} placeholder={gallery ? '.jpg' : '.zip'} />
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
          {#if gallery}<span class="kind">{kindLabel(item.kind)}</span>{/if}
          <span class="name">{displayName({ fileName: item.fileName, url: item.url })}</span>
        </label>
      </li>
    {/each}
  </ul>

  <div class="block">
    <label class="field" for="batch-folder">Folder</label>
    <div class="folder-row">
      <input
        id="batch-folder"
        type="text"
        value={folder}
        oninput={(event) => typeFolder(event.currentTarget.value)}
      />
      <button type="button" onclick={browse}>Browse…</button>
    </div>
    {#if gallery}
      <label class="check own">
        <input
          id="batch-own-folder"
          type="checkbox"
          checked={ownFolder}
          onchange={(event) => setOwnFolder(event.currentTarget.checked)}
        />
        Put them in a folder named after the page
      </label>
    {/if}
  </div>

  <label class="field">
    Queue for later
    <select bind:value={queueId}>
      {#each queues as queue (queue.id)}
        <option value={queue.id}>{queue.name}</option>
      {/each}
    </select>
  </label>

  {#if gallery}
    <p class="note">
      Pictures download a few at a time, through the Page media queue. Videos are read from their
      pages one at a time, and any that need a login are skipped. Saving from a site may go against
      its terms, and the pictures belong to the people who posted them.
    </p>
  {/if}

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
      {gallery ? 'Download all' : 'Start now'}
    </button>
  {/snippet}
</Modal>

<style>
  .lede {
    margin: 0 0 8px;
  }

  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    margin-bottom: 10px;
  }

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

  .kind {
    flex: none;
    min-width: 72px;
    font-size: 12px;
    color: var(--muted);
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

  .own {
    margin-top: 6px;
  }
</style>
