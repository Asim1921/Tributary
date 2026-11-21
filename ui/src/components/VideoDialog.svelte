<script lang="ts">
  import { sayError, t } from '../lib/locale.svelte';
  import { untrack } from 'svelte';

  import { YTDLP_MISSING, inspectVideo, installYtdlp, pickFolder, suggestCategory } from '../lib/api';
  import { describeVideoChoice } from '../lib/format';
  import {
    clock,
    defaultSubtitle,
    holdsCover,
    parseClock,
    sectionProblem,
    subtitleLabel,
  } from '../lib/video';
  import type { AddVideoRequest, VideoDetails } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    url: string;
    captureToken?: number;
    suggestedName?: string | null;
    /** The address is a web page with a player on it, not the video itself. */
    page?: boolean;
    defaultFolder: string;
    onsubmit: (request: AddVideoRequest) => void;
    /** The address turned out to be a list of videos, to choose from. */
    onlist?: () => void;
    onclose: () => void;
  }

  let { url, captureToken, suggestedName, page, defaultFolder, onsubmit, onlist, onclose }: Props =
    $props();

  let details = $state<VideoDetails | null>(null);
  /** The address is a playlist or channel, which the list dialog handles. */
  let isList = $state(false);
  let problem = $state<string | null>(null);
  let chosen = $state(0);
  let name = $state('');
  let folder = $state(untrack(() => defaultFolder));
  let installing = $state(false);
  let installProblem = $state<string | null>(null);
  /** The subtitles chosen, by position, or -1 for none. */
  let subtitle = $state(-1);
  let cover = $state(false);
  let mp3 = $state(false);
  let partOnly = $state(false);
  let from = $state('0:00');
  let to = $state('');
  let exact = $state(false);

  // The check finishes after the dialog opens, and must never overwrite what's been typed.
  let nameEdited = false;
  let folderEdited = false;

  const choice = $derived(details?.choices[chosen]);
  const audioOnly = $derived(choice?.audioOnly ?? false);
  const partProblem = $derived(partOnly ? sectionProblem(from, to, choice?.seconds ?? 0) : null);
  const offersSubtitles = $derived(!audioOnly && (details?.subtitles?.length ?? 0) > 0);
  /** Where the cover and subtitles end up, for the note below them. */
  const extension = $derived(audioOnly && mp3 ? 'mp3' : (choice?.extension ?? ''));
  const extrasBeside = $derived(
    !details?.canJoin || (cover && !holdsCover(extension)) || extension === 'ts',
  );

  async function check() {
    problem = null;
    isList = false;
    try {
      const found = await inspectVideo(url, suggestedName ?? undefined, captureToken, page);
      details = found;
      if (!nameEdited) name = found.suggestedName;
      subtitle = defaultSubtitle(found.subtitles ?? []);

      const first = found.choices[0];
      const suggestion = await suggestCategory(`${found.suggestedName}.${first?.extension}`, url);
      if (!folderEdited) folder = suggestion.folder;
    } catch (error) {
      problem = sayError(error);
      isList = String(error).includes('list of videos');
    }
  }

  $effect(() => {
    untrack(() => void check());
  });

  async function install() {
    installing = true;
    installProblem = null;
    try {
      await installYtdlp();
      await check();
    } catch (error) {
      installProblem = sayError(error);
    } finally {
      installing = false;
    }
  }

  async function browse() {
    const picked = await pickFolder(folder);
    if (picked) {
      folder = picked;
      folderEdited = true;
    }
  }

  /** Choosing sound alone offers its cover, which is what music players show. */
  function choose() {
    cover = audioOnly && (details?.hasCover ?? false);
    if (!audioOnly) mp3 = false;
  }

  function submit(later: boolean) {
    onsubmit({
      url,
      choice: chosen,
      destDir: folder,
      name,
      later,
      captureToken,
      subtitles: offersSubtitles && subtitle >= 0 ? [subtitle] : [],
      cover: (details?.hasCover ?? false) && cover,
      mp3: audioOnly && (details?.canJoin ?? false) && mp3,
      section:
        partOnly && !partProblem
          ? { from: parseClock(from) ?? 0, to: parseClock(to) ?? 0, exact }
          : undefined,
    });
  }
</script>

<Modal title={t('Save video')} wide {onclose}>
  <p class="selectable note">{url}</p>

  {#if problem === YTDLP_MISSING}
    <p class="refused" role="alert">
      {t("This site hides its video from ordinary downloading. Tributary can use yt-dlp, a free open-source tool, to find it. It will be downloaded from yt-dlp's own releases and checked against their published checksum.")}
    </p>
    {#if installProblem}<p class="error-text" role="alert">{installProblem}</p>{/if}
    <p>
      <button type="button" class="primary" disabled={installing} onclick={install}>
        {installing ? t('Installing…') : t('Install yt-dlp')}
      </button>
    </p>
  {:else if problem}
    <p class="refused" role="alert">{problem}</p>
    {#if isList && onlist}
      <p>
        <button type="button" class="primary" onclick={onlist}>{t('Choose videos from this list')}</button>
      </p>
    {/if}
  {:else if !details}
    <p>{t('Reading the video…')}</p>
  {:else}
    <fieldset>
      <legend>{t('Quality')}</legend>
      {#each details.choices as item, index (index)}
        <label class="check">
          <input
            type="radio"
            name="quality"
            value={index}
            bind:group={chosen}
            onchange={choose}
          />
          {item.label}
          <span class="muted">{describeVideoChoice(item)}</span>
        </label>
      {/each}
    </fieldset>

    {#if audioOnly && details.canJoin}
      <label class="check">
        <input type="checkbox" bind:checked={mp3} />
        {t('Convert to MP3')}
      </label>
    {/if}

    {#if offersSubtitles}
      <label class="field">
        {t('Subtitles')}
        <select bind:value={subtitle}>
          <option value={-1}>{t('None')}</option>
          {#each details.subtitles ?? [] as item, index (index)}
            <option value={index}>{subtitleLabel(item)}</option>
          {/each}
        </select>
      </label>
    {/if}

    {#if details.canJoin}
      <label class="check">
        <input
          type="checkbox"
          bind:checked={partOnly}
          onchange={() => {
            if (partOnly && !to) to = clock(choice?.seconds ?? 0);
          }}
        />
        {t('Save only part of it')}
      </label>
      {#if partOnly}
        <div class="pair">
          <label class="field">
            {t('From')}
            <input type="text" bind:value={from} placeholder="0:00" />
          </label>
          <label class="field">
            {t('To')}
            <input type="text" bind:value={to} placeholder={clock(choice?.seconds ?? 0)} />
          </label>
        </div>
        {#if !audioOnly}
          <label class="check">
            <input type="checkbox" bind:checked={exact} />
            {t('Cut exactly (slower, because the picture is encoded again)')}
          </label>
        {/if}
        {#if partProblem}<p class="error-text" role="alert">{partProblem}</p>{/if}
      {/if}
    {/if}

    {#if details.hasCover}
      <label class="check">
        <input type="checkbox" bind:checked={cover} />
        {t('Add the cover picture')}
      </label>
    {/if}

    {#if (cover || (offersSubtitles && subtitle >= 0)) && extrasBeside}
      <p class="note">
        {#if !details.canJoin}
          {t('Without ffmpeg, the subtitles and cover are saved beside the file. Set up ffmpeg in Options to have them put inside it.')}
        {:else}
          {t("This kind of file can't hold everything chosen, so what doesn't fit is saved beside it.")}
        {/if}
      </p>
    {/if}

    {#if page}
      <p class="note">
        {t("Saving video from a site may go against that site's terms. Protected, paid and login-only videos are refused.")}
      </p>
    {/if}

    {#if choice?.separateAudio}
      <p class="note">
        {#if details.canJoin}
          {t('The picture and sound are separate, and will be joined into one file when both finish.')}
        {:else}
          {t('The picture and sound are separate, and stay as two files. Set up ffmpeg in Options to have them joined.')}
        {/if}
      </p>
    {/if}

    <label class="field">
      {t('Save as')}
      <input type="text" bind:value={name} oninput={() => (nameEdited = true)} />
    </label>

    <div class="field-block">
      <label class="field" for="video-folder">{t('Folder')}</label>
      <div class="folder-row">
        <input
          id="video-folder"
          type="text"
          bind:value={folder}
          oninput={() => (folderEdited = true)}
        />
        <button type="button" onclick={browse}>{t('Browse…')}</button>
      </div>
    </div>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={onclose}>{problem ? t('Close') : t('Cancel')}</button>
    {#if !problem}
      <button
        type="button"
        disabled={!details || !name.trim() || !!partProblem}
        onclick={() => submit(true)}
      >
        {t('Download later')}
      </button>
      <button
        type="button"
        class="primary"
        disabled={!details || !name.trim() || !!partProblem}
        onclick={() => submit(false)}
      >
        {t('Start download')}
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
    margin-inline-start: 6px;
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

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .field-block .field {
    margin-bottom: 4px;
  }
</style>
