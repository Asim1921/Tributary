<script lang="ts">
  import { languageCode, mark, t } from '../lib/locale.svelte';
  import { displayName, statusText } from '../lib/format';
  import { DAY_NAMES, dayEnabled, dayName, minutesToTime, timeToMinutes, toggleDay } from '../lib/time';
  import { moveItem } from '../lib/toolbar';
  import type { DownloadRow, Queue } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    queues: Queue[];
    rows: DownloadRow[];
    onsave: (queue: Queue) => Promise<number>;
    ondelete: (id: number) => void;
    onstart: (id: number) => void;
    onstop: (id: number) => void;
    onreorder: (ids: number[]) => void;
    onremovefile: (id: number) => void;
    onclose: () => void;
  }

  let { queues, rows, onsave, ondelete, onstart, onstop, onreorder, onremovefile, onclose }: Props =
    $props();

  type Tab = 'schedule' | 'files' | 'end';
  const TABS: { id: Tab; label: string }[] = [
    { id: 'schedule', label: mark('Schedule') },
    { id: 'files', label: mark('Files') },
    { id: 'end', label: mark('When it ends') },
  ];

  const NEW_QUEUE: Queue = {
    id: 0,
    name: t('New queue'),
    sync: false,
    concurrency: 2,
    retries: 5,
    startMinute: null,
    stopMinute: null,
    days: 127,
    syncMinutes: 60,
    after: { shutdown: false, force: false, program: '', wake: false },
    active: false,
    files: [],
  };

  let tab = $state<Tab>('schedule');
  let selectedId = $state(1);
  let draft = $state<Queue>({ ...NEW_QUEUE });
  let startTime = $state('');
  let stopTime = $state('');

  const current = $derived(queues.find((queue) => queue.id === selectedId));
  const files = $derived(
    (current?.files ?? []).flatMap((id) => rows.find((row) => row.id === id) ?? []),
  );

  function load(queue: Queue) {
    selectedId = queue.id;
    draft = { ...$state.snapshot(queue), after: { ...$state.snapshot(queue.after) } } as Queue;
    startTime = minutesToTime(queue.startMinute);
    stopTime = minutesToTime(queue.stopMinute);
  }

  // Opens on the first queue, once the list is there.
  $effect(() => {
    if (draft.id === 0 && selectedId !== 0 && current) load(current);
  });

  function addQueue() {
    selectedId = 0;
    draft = { ...NEW_QUEUE, after: { ...NEW_QUEUE.after } };
    startTime = '';
    stopTime = '';
    tab = 'schedule';
  }

  async function save() {
    const id = await onsave({
      ...($state.snapshot(draft) as Queue),
      startMinute: timeToMinutes(startTime),
      stopMinute: timeToMinutes(stopTime),
    });
    selectedId = id;
    draft.id = id;
  }

  function move(index: number, offset: -1 | 1) {
    onreorder(moveItem(files, index, offset).map((row) => row.id));
  }
</script>

<Modal title={t('Scheduler')} wide {onclose}>
  <div class="layout">
    <div class="queues">
      {#each queues as queue (queue.id)}
        <button
          type="button"
          class="queue"
          class:active={queue.id === selectedId}
          onclick={() => load(queue)}
        >
          {t(queue.name)}
          {#if queue.active}<span class="running">{t('running')}</span>{/if}
        </button>
      {/each}
      {#if selectedId === 0}
        <button type="button" class="queue active">{draft.name}</button>
      {/if}
      <button type="button" onclick={addQueue}>{t('New queue')}</button>
    </div>

    <div class="details">
      <div class="tabs" role="tablist">
        {#each TABS as item (item.id)}
          <button
            type="button"
            role="tab"
            aria-selected={tab === item.id}
            class:active={tab === item.id}
            onclick={() => (tab = item.id)}
          >
            {t(item.label)}
          </button>
        {/each}
      </div>

      {#if tab === 'schedule'}
        <label class="field">
          {t('Queue name')}
          <input type="text" bind:value={draft.name} />
        </label>

        <div class="pair">
          <label class="field">
            {t('Start at')}
            <input type="time" bind:value={startTime} />
          </label>
          <label class="field">
            {t('Stop at')}
            <input type="time" bind:value={stopTime} />
          </label>
        </div>

        <div class="days" role="group" aria-label={t('Days')}>
          {#each DAY_NAMES as day, index (day)}
            <label class="check">
              <input
                type="checkbox"
                checked={dayEnabled(draft.days, index)}
                onchange={() => (draft.days = toggleDay(draft.days, index))}
              />
              {dayName(index, languageCode())}
            </label>
          {/each}
        </div>

        <div class="pair">
          <label class="field">
            {t('Files at once')}
            <input type="number" min="1" max="32" bind:value={draft.concurrency} />
          </label>
          <label class="field">
            {t('Retries per file')}
            <input type="number" min="0" max="999" bind:value={draft.retries} />
          </label>
        </div>

        <label class="check">
          <input type="checkbox" bind:checked={draft.sync} />
          {t('Keep these files in sync with the server')}
        </label>
        {#if draft.sync}
          <label class="field">
            {t('Check every (minutes)')}
            <input type="number" min="1" bind:value={draft.syncMinutes} />
          </label>
        {/if}
      {:else if tab === 'files'}
        {#if selectedId === 0}
          <p class="note">{t('Save the queue first, then add files to it.')}</p>
        {:else if files.length === 0}
          <p class="note">{t('No files. Right-click a download to add it to this queue.')}</p>
        {:else}
          <ul>
            {#each files as row, index (row.id)}
              <li>
                <span class="name" title={displayName(row)}>{displayName(row)}</span>
                <span class="note">{statusText(row)}</span>
                <span class="moves">
                  <button
                    type="button"
                    aria-label={t('Move up')}
                    disabled={index === 0}
                    onclick={() => move(index, -1)}>↑</button
                  >
                  <button
                    type="button"
                    aria-label={t('Move down')}
                    disabled={index === files.length - 1}
                    onclick={() => move(index, 1)}>↓</button
                  >
                  <button
                    type="button"
                    aria-label={t('Remove from queue')}
                    onclick={() => onremovefile(row.id)}>✕</button
                  >
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      {:else}
        <label class="field">
          {t('Run a program or open a file')}
          <input type="text" bind:value={draft.after.program} placeholder={t('Optional')} />
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={draft.after.shutdown} />
          {t('Shut down the computer')}
        </label>
        <label class="check indented">
          <input
            type="checkbox"
            bind:checked={draft.after.force}
            disabled={!draft.after.shutdown}
          />
          {t('Force programs to close (unsaved work may be lost)')}
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={draft.after.wake} />
          {t('Wake the computer for the start time')}
        </label>
        <p class="note">
          {t('These run when the queue ends by itself, not when you stop it. A shutdown gives you 60 seconds to cancel.')}
        </p>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    {#if current && current.id !== 1}
      <button type="button" class="left" onclick={() => ondelete(current.id)}>{t('Delete queue')}</button>
    {/if}
    {#if current?.active}
      <button type="button" onclick={() => onstop(current.id)}>{t('Stop queue')}</button>
    {:else if current}
      <button type="button" onclick={() => onstart(current.id)}>{t('Start now')}</button>
    {/if}
    <button type="button" onclick={save}>{t('Save')}</button>
    <button type="button" class="primary" onclick={onclose}>{t('Close')}</button>
  {/snippet}
</Modal>

<style>
  .layout {
    display: flex;
    gap: 14px;
    min-height: 340px;
  }

  .queues {
    display: grid;
    align-content: start;
    gap: 2px;
    width: 150px;
    flex: none;
  }

  .queue {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    background: transparent;
    border-color: transparent;
    text-align: start;
  }

  .queue.active {
    background: var(--selected);
    font-weight: 600;
  }

  .running {
    color: var(--accent);
    font-size: 11px;
  }

  .details {
    flex: 1;
    min-width: 0;
  }

  .tabs {
    display: flex;
    gap: 2px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }

  .tabs button {
    margin-bottom: -1px;
    background: transparent;
    border: 0;
    border-bottom: 2px solid transparent;
    border-radius: 0;
  }

  .tabs button.active {
    border-bottom-color: var(--accent);
    font-weight: 600;
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .days {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 12px;
    margin-bottom: 8px;
  }

  .days .check {
    gap: 4px;
    margin-bottom: 4px;
  }

  .indented {
    margin-inline-start: 22px;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .moves button {
    padding: 0 7px;
  }

  .left {
    margin-inline-end: auto;
  }
</style>
