<script lang="ts">
  import { untrack } from 'svelte';

  import { pickFolder } from '../lib/api';
  import { columnById, resolveColumns } from '../lib/columns';
  import { moveItem, resolveToolbar, TOOLBAR_BUTTONS } from '../lib/toolbar';
  import type { ColumnSetting, Settings } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    settings: Settings;
    onsave: (settings: Settings) => void;
    onclose: () => void;
  }

  let { settings, onsave, onclose }: Props = $props();

  type Tab = 'general' | 'downloads' | 'columns' | 'toolbar';
  const TABS: { id: Tab; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'downloads', label: 'Downloads' },
    { id: 'columns', label: 'Columns' },
    { id: 'toolbar', label: 'Toolbar' },
  ];

  interface ToolbarEntry {
    id: string;
    label: string;
    shown: boolean;
  }

  function toolbarEntries(saved: string[]): ToolbarEntry[] {
    const shown = resolveToolbar(saved);
    const shownIds = new Set(shown.map((button) => button.id as string));
    return [
      ...shown.map(({ id, label }) => ({ id: id as string, label, shown: true })),
      ...TOOLBAR_BUTTONS.filter((button) => !shownIds.has(button.id)).map(({ id, label }) => ({
        id: id as string,
        label,
        shown: false,
      })),
    ];
  }

  // A working copy, taken once when the dialog opens. Save applies it, Cancel drops it.
  const opening = untrack(() => $state.snapshot(settings)) as Settings;

  let tab = $state<Tab>('general');
  let draft = $state<Settings>(opening);
  let columns = $state<ColumnSetting[]>(resolveColumns(opening.columns));
  let toolbar = $state<ToolbarEntry[]>(toolbarEntries(opening.toolbar));

  async function browseDownloads() {
    const chosen = await pickFolder(draft.downloadDir);
    if (chosen) draft.downloadDir = chosen;
  }

  async function browseTemp() {
    const chosen = await pickFolder(draft.tempDir ?? undefined);
    if (chosen) draft.tempDir = chosen;
  }

  function save() {
    onsave({
      ...$state.snapshot(draft),
      columns: $state.snapshot(columns),
      toolbar: toolbar.filter((entry) => entry.shown).map((entry) => entry.id),
    } as Settings);
  }
</script>

<Modal title="Options" wide {onclose}>
  <div class="tabs" role="tablist">
    {#each TABS as item (item.id)}
      <button
        type="button"
        role="tab"
        aria-selected={tab === item.id}
        class:active={tab === item.id}
        onclick={() => (tab = item.id)}
      >
        {item.label}
      </button>
    {/each}
  </div>

  <div class="panel">
    {#if tab === 'general'}
      <label class="field">
        Theme
        <select bind:value={draft.theme}>
          <option value="system">Match Windows</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.startWithWindows} />
        Start with Windows
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.closeToTray} />
        Keep running in the tray when the window is closed
      </label>
    {:else if tab === 'downloads'}
      <div class="block">
        <label class="field" for="options-folder">Default folder</label>
        <div class="folder-row">
          <input id="options-folder" type="text" bind:value={draft.downloadDir} />
          <button type="button" onclick={browseDownloads}>Browse…</button>
        </div>
      </div>

      <div class="block">
        <label class="field" for="options-temp">Temporary folder</label>
        <div class="folder-row">
          <input
            id="options-temp"
            type="text"
            value={draft.tempDir ?? ''}
            placeholder="Same as the destination"
            readonly
          />
          <button type="button" onclick={browseTemp}>Browse…</button>
          <button type="button" disabled={!draft.tempDir} onclick={() => (draft.tempDir = null)}>
            Clear
          </button>
        </div>
        <p class="note">Partial files go here. Applies to new downloads.</p>
      </div>

      <label class="field">
        Connections per download
        <input type="number" min="1" max="32" bind:value={draft.connections} />
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={draft.showStartDialog} />
        Ask before starting a download
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.showCompleteDialog} />
        Show a message when a download finishes
      </label>

      <button
        type="button"
        disabled={draft.urlHistory.length === 0}
        onclick={() => (draft.urlHistory = [])}
      >
        Clear address history
      </button>
    {:else if tab === 'columns'}
      <ul>
        {#each columns as column, index (column.id)}
          <li>
            <label class="check">
              <input type="checkbox" bind:checked={column.visible} disabled={column.id === 'name'} />
              {columnById(column.id)?.label}
            </label>
            <span class="moves">
              <button
                type="button"
                aria-label="Move up"
                disabled={index === 0}
                onclick={() => (columns = moveItem(columns, index, -1))}>↑</button
              >
              <button
                type="button"
                aria-label="Move down"
                disabled={index === columns.length - 1}
                onclick={() => (columns = moveItem(columns, index, 1))}>↓</button
              >
            </span>
          </li>
        {/each}
      </ul>
    {:else}
      <label class="field">
        Button size
        <select bind:value={draft.toolbarSize}>
          <option value="large">Large, with labels</option>
          <option value="small">Small, icons only</option>
        </select>
      </label>
      <ul>
        {#each toolbar as entry, index (entry.id)}
          <li>
            <label class="check">
              <input type="checkbox" bind:checked={entry.shown} />
              {entry.label}
            </label>
            <span class="moves">
              <button
                type="button"
                aria-label="Move up"
                disabled={index === 0}
                onclick={() => (toolbar = moveItem(toolbar, index, -1))}>↑</button
              >
              <button
                type="button"
                aria-label="Move down"
                disabled={index === toolbar.length - 1}
                onclick={() => (toolbar = moveItem(toolbar, index, 1))}>↓</button
              >
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    <button type="button" class="primary" onclick={save}>Save</button>
  {/snippet}
</Modal>

<style>
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

  .panel {
    min-height: 290px;
  }

  .block {
    margin-bottom: 12px;
  }

  .block .field {
    margin-bottom: 4px;
  }

  .block .note {
    margin: 4px 0 0;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 0;
  }

  li .check {
    margin: 0;
  }

  .moves button {
    padding: 0 8px;
  }
</style>
