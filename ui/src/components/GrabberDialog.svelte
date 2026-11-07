<script lang="ts">
  import { untrack } from 'svelte';

  import * as api from '../lib/api';
  import { formatBytes } from '../lib/format';
  import {
    TEMPLATES,
    bytesToKilobytes,
    fileNameOf,
    fromLines,
    kilobytesToBytes,
    toLines,
    type TemplateId,
  } from '../lib/grabber';
  import type { GrabberFile, GrabberProgress, GrabberProject, ProjectView } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    onerror: (error: unknown) => void;
    onclose: () => void;
  }

  let { onerror, onclose }: Props = $props();

  const STEPS = ['Start', 'Where to look', 'What to keep', 'When'] as const;

  let view = $state<'list' | 'wizard' | 'project'>('list');
  let projects = $state<ProjectView[]>([]);

  // The wizard.
  let step = $state(0);
  let template = $state<TemplateId>('allFiles');
  let startUrl = $state('');
  let draft = $state<GrabberProject | null>(null);
  let includePages = $state('');
  let excludePages = $state('');
  let fileTypes = $state('');
  let excludeFiles = $state('');
  let minSize = $state('');
  let maxSize = $state('');
  let repeats = $state(false);
  let problem = $state<string | null>(null);

  // One project, being explored.
  let current = $state<ProjectView | null>(null);
  let progress = $state<GrabberProgress | null>(null);
  let exploring = $state(false);
  let files = $state<GrabberFile[]>([]);
  let chosen = $state<Set<string>>(new Set());
  let message = $state<string | null>(null);

  const unsaved = $derived(files.filter((file) => !file.saved));

  async function refresh() {
    projects = await api.listProjects();
  }

  async function loadFiles(id: number) {
    files = await api.projectFiles(id);
    chosen = new Set(files.filter((file) => !file.saved).map((file) => file.url));
  }

  $effect(() => {
    untrack(() => void refresh().catch(onerror));

    const stopListening = [
      api.onGrabberProgress((event) => {
        if (event.id === current?.id) progress = event;
      }),
      api.onGrabberFinished((id) => {
        if (id !== current?.id) return;
        exploring = false;
        void loadFiles(id).catch(onerror);
        void refresh().catch(onerror);
      }),
    ];
    return () => stopListening.forEach((listener) => void listener.then((stop) => stop()));
  });

  function newProject() {
    step = 0;
    template = 'allFiles';
    startUrl = '';
    draft = null;
    problem = null;
    view = 'wizard';
  }

  /** The template sets everything else, so it applies when leaving the first step. */
  async function leaveFirstStep() {
    problem = null;
    let made: GrabberProject;
    try {
      made = await api.newProject(template, startUrl);
    } catch (error) {
      problem = String(error);
      return;
    }

    draft = made;
    includePages = toLines(made.includePages);
    excludePages = toLines(made.excludePages);
    fileTypes = toLines(made.fileTypes);
    excludeFiles = toLines(made.excludeFiles);
    minSize = bytesToKilobytes(made.minSize);
    maxSize = bytesToKilobytes(made.maxSize);
    repeats = made.repeatHours !== null;
    step = 1;
  }

  async function finishWizard() {
    if (!draft) return;
    problem = null;

    const project: GrabberProject = {
      ...$state.snapshot(draft),
      includePages: fromLines(includePages),
      excludePages: fromLines(excludePages),
      fileTypes: fromLines(fileTypes),
      excludeFiles: fromLines(excludeFiles),
      minSize: kilobytesToBytes(minSize),
      maxSize: kilobytesToBytes(maxSize),
      repeatHours: repeats ? (draft.repeatHours ?? 24) : null,
    };

    try {
      const id = await api.saveProject(project);
      await refresh();
      await openProject(id);
      await explore();
    } catch (error) {
      problem = String(error);
    }
  }

  async function openProject(id: number) {
    current = projects.find((project) => project.id === id) ?? null;
    progress = null;
    message = null;
    exploring = current?.running ?? false;
    await loadFiles(id);
    view = 'project';
  }

  async function explore() {
    if (!current) return;
    message = null;
    exploring = true;
    await api.startExploring(current.id);
  }

  async function saveFiles(urls: string[]) {
    if (!current) return;
    try {
      const count = await api.downloadProjectFiles(current.id, urls);
      message = `Added ${count} ${count === 1 ? 'file' : 'files'} to the main queue.`;
      await loadFiles(current.id);
    } catch (error) {
      onerror(error);
    }
  }

  async function remove(id: number) {
    await api.deleteProject(id);
    await refresh();
  }

  function toggle(url: string) {
    const next = new Set(chosen);
    if (!next.delete(url)) next.add(url);
    chosen = next;
  }
</script>

<Modal title="Site grabber" wide {onclose}>
  {#if view === 'list'}
    {#if projects.length === 0}
      <p>No projects yet. A project explores a website for pages and files to save.</p>
    {:else}
      <ul class="projects">
        {#each projects as project (project.id)}
          <li>
            <div class="grow">
              <strong>{project.name}</strong>
              <span class="note">{project.startUrl}</span>
              <span class="note">
                {project.filesFound} files found{project.repeatHours === null
                  ? ''
                  : `, repeats every ${project.repeatHours} hours`}
              </span>
            </div>
            <button type="button" onclick={() => openProject(project.id).catch(onerror)}>
              Open
            </button>
            <button type="button" onclick={() => remove(project.id).catch(onerror)}>Delete</button>
          </li>
        {/each}
      </ul>
    {/if}
  {:else if view === 'wizard'}
    <ol class="steps" aria-label="Steps">
      {#each STEPS as name, index (name)}
        <li class:active={index === step}>{name}</li>
      {/each}
    </ol>

    {#if step === 0}
      <label class="field">
        Starting address
        <input type="text" bind:value={startUrl} placeholder="https://" />
      </label>
      <fieldset>
        <legend>What to get</legend>
        {#each TEMPLATES as item (item.id)}
          <label class="check">
            <input type="radio" name="template" value={item.id} bind:group={template} />
            {item.label}
            <span class="muted">{item.hint}</span>
          </label>
        {/each}
      </fieldset>
    {:else if draft && step === 1}
      <label class="field">
        Project name
        <input type="text" bind:value={draft.name} />
      </label>
      <div class="pair">
        <label class="field">
          How many links deep
          <input type="number" min="0" bind:value={draft.maxDepth} />
        </label>
        <label class="field">
          Most pages to read
          <input type="number" min="1" bind:value={draft.maxPages} />
        </label>
      </div>
      <div class="pair">
        <label class="field">
          Stay within
          <select bind:value={draft.scope}>
            <option value="site">This site</option>
            <option value="folder">This folder of the site</option>
          </select>
        </label>
        <label class="field">
          Wait between pages (milliseconds)
          <input type="number" min="100" step="100" bind:value={draft.delayMs} />
        </label>
      </div>
      <div class="pair">
        <label class="field">
          Only read pages matching
          <textarea rows="2" bind:value={includePages} placeholder="One per line, * as a wildcard"
          ></textarea>
        </label>
        <label class="field">
          Never read pages matching
          <textarea rows="2" bind:value={excludePages}></textarea>
        </label>
      </div>
      <p class="note">This site's robots.txt is always followed.</p>
    {:else if draft && step === 2}
      {#if !draft.offline}
        <label class="field">
          File types to keep
          <textarea rows="2" bind:value={fileTypes} placeholder="One per line. Empty keeps them all"
          ></textarea>
        </label>
      {/if}
      <label class="field">
        Never keep files matching
        <textarea rows="2" bind:value={excludeFiles}></textarea>
      </label>
      <div class="pair">
        <label class="field">
          Smallest size (KB)
          <input type="text" bind:value={minSize} />
        </label>
        <label class="field">
          Largest size (KB)
          <input type="text" bind:value={maxSize} />
        </label>
      </div>
      <label class="field">
        Save into
        <input type="text" bind:value={draft.folder} />
      </label>
      {#if !draft.offline}
        <label class="check">
          <input type="checkbox" bind:checked={draft.keepFolders} />
          Keep the site's folders
        </label>
      {/if}
      <label class="check">
        <input type="checkbox" bind:checked={draft.filesAnywhere} />
        Also take files kept on other sites
      </label>
    {:else if draft}
      <label class="check">
        <input type="checkbox" bind:checked={repeats} />
        Run this project again by itself
      </label>
      {#if repeats}
        <label class="field">
          Every how many hours
          <input
            type="number"
            min="1"
            value={draft.repeatHours ?? 24}
            oninput={(event) => {
              if (draft) draft.repeatHours = Number(event.currentTarget.value) || 24;
            }}
          />
        </label>
        <p class="note">Each run saves whatever is new on the site, while the app is open.</p>
      {/if}
    {/if}

    {#if problem}<p class="error-text" role="alert">{problem}</p>{/if}
  {:else if current}
    <p class="note">{current.startUrl}</p>
    <p role="status">
      {#if exploring}
        Exploring:
      {:else if progress?.finished || current.explored}
        Finished:
      {/if}
      {progress?.pagesRead ?? current.pagesRead} pages read,
      {progress?.filesFound ?? files.length} files found{#if progress?.pagesRefused}, {progress.pagesRefused}
        left alone for robots.txt{/if}
    </p>

    {#if files.length > 0}
      <ul class="files" aria-label="Files found">
        {#each files as file (file.url)}
          <li>
            <label class="check" title={file.url}>
              <input
                type="checkbox"
                disabled={file.saved}
                checked={file.saved || chosen.has(file.url)}
                onchange={() => toggle(file.url)}
              />
              {fileNameOf(file.url)}
              {#if file.size !== null}<span class="muted">{formatBytes(file.size)}</span>{/if}
              {#if file.saved}<span class="muted">saved</span>{/if}
            </label>
          </li>
        {/each}
      </ul>
    {/if}

    {#if message}<p role="status">{message}</p>{/if}
  {/if}

  {#snippet footer()}
    {#if view === 'list'}
      <button type="button" onclick={onclose}>Close</button>
      <button type="button" class="primary" onclick={newProject}>New project</button>
    {:else if view === 'wizard'}
      <button type="button" onclick={() => (view = 'list')}>Cancel</button>
      {#if step > 0}
        <button type="button" onclick={() => (step -= 1)}>Back</button>
      {/if}
      {#if step === 0}
        <button
          type="button"
          class="primary"
          disabled={!startUrl.trim()}
          onclick={leaveFirstStep}
        >
          Next
        </button>
      {:else if step < STEPS.length - 1}
        <button type="button" class="primary" onclick={() => (step += 1)}>Next</button>
      {:else}
        <button type="button" class="primary" onclick={finishWizard}>Start exploring</button>
      {/if}
    {:else}
      <button
        type="button"
        onclick={() => {
          view = 'list';
          void refresh().catch(onerror);
        }}
      >
        Back to projects
      </button>
      {#if exploring}
        <button
          type="button"
          onclick={() => current && api.stopExploring(current.id).catch(onerror)}
        >
          Stop
        </button>
      {:else}
        <button type="button" onclick={() => explore().catch(onerror)}>
          {current?.explored || progress?.finished ? 'Explore again' : 'Continue exploring'}
        </button>
        {#if !current?.offline}
          <button
            type="button"
            disabled={chosen.size === 0}
            onclick={() => saveFiles([...chosen])}
          >
            Download selected
          </button>
          <button
            type="button"
            class="primary"
            disabled={unsaved.length === 0}
            onclick={() => saveFiles(unsaved.map((file) => file.url))}
          >
            Download all
          </button>
        {/if}
      {/if}
    {/if}
  {/snippet}
</Modal>

<style>
  .projects,
  .files,
  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .projects li {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }

  .grow {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .steps {
    display: flex;
    gap: 16px;
    margin-bottom: 14px;
    color: var(--muted);
  }

  .steps .active {
    color: inherit;
    font-weight: 600;
  }

  .files {
    max-height: 240px;
    margin-bottom: 10px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 10px;
  }

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

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  textarea {
    width: 100%;
    font: inherit;
    resize: vertical;
  }
</style>
