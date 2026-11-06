<script lang="ts">
  import { untrack } from 'svelte';

  import { inspectVideo, pickFolder, suggestCategory } from '../lib/api';
  import { describeVideoChoice } from '../lib/format';
  import type { AddVideoRequest, VideoDetails } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    url: string;
    captureToken?: number;
    suggestedName?: string | null;
    defaultFolder: string;
    onsubmit: (request: AddVideoRequest) => void;
    onclose: () => void;
  }

  let { url, captureToken, suggestedName, defaultFolder, onsubmit, onclose }: Props = $props();

  let details = $state<VideoDetails | null>(null);
  let problem = $state<string | null>(null);
  let chosen = $state(0);
  let name = $state('');
  let folder = $state(untrack(() => defaultFolder));

  // The check finishes after the dialog opens, and must never overwrite what's been typed.
  let nameEdited = false;
  let folderEdited = false;

  const choice = $derived(details?.choices[chosen]);

  async function check() {
    try {
      const found = await inspectVideo(url, suggestedName ?? undefined, captureToken);
      details = found;
      if (!nameEdited) name = found.suggestedName;

      const first = found.choices[0];
      const suggestion = await suggestCategory(`${found.suggestedName}.${first?.extension}`, url);
      if (!folderEdited) folder = suggestion.folder;
    } catch (error) {
      problem = String(error);
    }
  }

  $effect(() => {
    untrack(() => void check());
  });

  async function browse() {
    const picked = await pickFolder(folder);
    if (picked) {
      folder = picked;
      folderEdited = true;
    }
  }

  function submit(later: boolean) {
    onsubmit({ url, choice: chosen, destDir: folder, name, later, captureToken });
  }
</script>

<Modal title="Save video" wide {onclose}>
  <p class="selectable note">{url}</p>

  {#if problem}
    <p class="refused" role="alert">{problem}</p>
  {:else if !details}
    <p>Reading the video…</p>
  {:else}
    <fieldset>
      <legend>Quality</legend>
      {#each details.choices as item, index (index)}
        <label class="check">
          <input type="radio" name="quality" value={index} bind:group={chosen} />
          {item.label}
          <span class="muted">{describeVideoChoice(item)}</span>
        </label>
      {/each}
    </fieldset>

    {#if choice?.separateAudio}
      <p class="note">
        {#if details.canJoin}
          The picture and sound are separate, and will be joined into one file when both finish.
        {:else}
          The picture and sound are separate, and stay as two files. Set up ffmpeg in Options to
          have them joined.
        {/if}
      </p>
    {/if}

    <label class="field">
      Save as
      <input type="text" bind:value={name} oninput={() => (nameEdited = true)} />
    </label>

    <div class="field-block">
      <label class="field" for="video-folder">Folder</label>
      <div class="folder-row">
        <input
          id="video-folder"
          type="text"
          bind:value={folder}
          oninput={() => (folderEdited = true)}
        />
        <button type="button" onclick={browse}>Browse…</button>
      </div>
    </div>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={onclose}>{problem ? 'Close' : 'Cancel'}</button>
    {#if !problem}
      <button type="button" disabled={!details || !name.trim()} onclick={() => submit(true)}>
        Download later
      </button>
      <button
        type="button"
        class="primary"
        disabled={!details || !name.trim()}
        onclick={() => submit(false)}
      >
        Start download
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  fieldset {
    margin: 0 0 12px;
    padding: 8px 12px 4px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  legend {
    padding: 0 4px;
  }

  .muted {
    margin-left: 6px;
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
