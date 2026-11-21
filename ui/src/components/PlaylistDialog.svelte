<script lang="ts">
  import { sayError, t } from '../lib/locale.svelte';
  import { untrack } from 'svelte';

  import { addList, inspectList, pickFolder } from '../lib/api';
  import { folderForPage } from '../lib/batch';
  import { formatDuration } from '../lib/format';
  import type { ListView } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    url: string;
    defaultFolder: string;
    onsaved: (count: number) => void;
    onclose: () => void;
  }

  let { url, defaultFolder, onsaved, onclose }: Props = $props();

  let list = $state<ListView | null>(null);
  let problem = $state<string | null>(null);
  let chosen = $state<Set<number>>(new Set());
  let folder = $state(untrack(() => defaultFolder));
  let saving = $state(false);
  let folderEdited = false;

  const available = $derived(
    (list?.entries ?? []).flatMap((entry, index) => (entry.available ? [index] : [])),
  );

  $effect(() => {
    untrack(() => {
      inspectList(url)
        .then((found) => {
          list = found;
          chosen = new Set(found.entries.flatMap((entry, index) => (entry.available ? [index] : [])));
          if (!folderEdited) folder = folderForPage(defaultFolder, found.title);
        })
        .catch((error) => (problem = sayError(error)));
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
    if (picked) {
      folder = picked;
      folderEdited = true;
    }
  }

  async function save(later: boolean) {
    saving = true;
    try {
      const count = await addList([...chosen].sort((a, b) => a - b), folder, later);
      onsaved(count);
    } catch (error) {
      problem = sayError(error);
      saving = false;
    }
  }
</script>

<Modal
  title={list?.title ? t('Videos in {title}', { title: list.title }) : t('Videos in this list')}
  wide
  {onclose}
>
  <p class="selectable note">{url}</p>

  {#if problem}
    <p class="refused" role="alert">{problem}</p>
  {:else if !list}
    <p>{t('Reading the list…')}</p>
  {:else}
    <div class="tools">
      <span>{t('{chosen} of {count} chosen', { chosen: chosen.size, count: list.entries.length })}</span>
      <button type="button" onclick={() => (chosen = new Set(available))}>{t('Choose all')}</button>
      <button type="button" onclick={() => (chosen = new Set())}>{t('Choose none')}</button>
    </div>
    <ul class="entries">
      {#each list.entries as entry, index (index)}
        <li class:gone={!entry.available}>
          <label class="check">
            <input
              type="checkbox"
              checked={chosen.has(index)}
              disabled={!entry.available}
              onchange={() => toggle(index)}
            />
            <span class="title">{entry.title}</span>
            {#if entry.seconds}<span class="muted">{formatDuration(entry.seconds)}</span>{/if}
            {#if !entry.available}<span class="muted">{t("can't be saved")}</span>{/if}
          </label>
        </li>
      {/each}
    </ul>

    <div class="field-block">
      <label class="field" for="list-folder">{t('Folder')}</label>
      <div class="folder-row">
        <input id="list-folder" type="text" bind:value={folder} oninput={() => (folderEdited = true)} />
        <button type="button" onclick={browse}>{t('Browse…')}</button>
      </div>
    </div>
    <p class="note">
      {t('Each video is read from its own page one at a time, a few seconds apart, and saved at its best quality. Saving video from a site may go against its terms. Private, paid and login-only videos are refused.')}
    </p>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={onclose}>{problem ? t('Close') : t('Cancel')}</button>
    {#if !problem}
      <button type="button" disabled={!list || chosen.size === 0 || saving} onclick={() => save(true)}>
        {t('Download later')}
      </button>
      <button
        type="button"
        class="primary"
        disabled={!list || chosen.size === 0 || saving}
        onclick={() => save(false)}
      >
        {t('{count, plural, one {Download # video} other {Download # videos}}', { count: chosen.size })}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .tools {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-bottom: 8px;
  }

  .tools span {
    margin-inline-end: auto;
  }

  .entries {
    max-height: 320px;
    margin: 0 0 12px;
    padding: 4px 8px;
    overflow-y: auto;
    list-style: none;
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .entries li {
    padding: 2px 0;
  }

  .gone {
    opacity: 0.55;
  }

  .title {
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
