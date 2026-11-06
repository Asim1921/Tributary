<script lang="ts">
  import { onMount } from 'svelte';

  import AddUrlDialog from './components/AddUrlDialog.svelte';
  import CompleteDialog from './components/CompleteDialog.svelte';
  import ContextMenu, { type MenuItem } from './components/ContextMenu.svelte';
  import DeleteDialog from './components/DeleteDialog.svelte';
  import DownloadList from './components/DownloadList.svelte';
  import Modal from './components/Modal.svelte';
  import OptionsDialog from './components/OptionsDialog.svelte';
  import ProgressDialog from './components/ProgressDialog.svelte';
  import PropertiesDialog from './components/PropertiesDialog.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import StartDialog from './components/StartDialog.svelte';
  import Toolbar from './components/Toolbar.svelte';
  import * as api from './lib/api';
  import { resolveColumns, sortRows, type SortOrder } from './lib/columns';
  import { downloads } from './lib/downloads.svelte';
  import { filterCounts, filterRows, type Filter } from './lib/filters';
  import { formatSpeed } from './lib/format';
  import { clickRow, keepExisting, rightClickRow, type SelectionState } from './lib/selection';
  import { resolveToolbar, type ToolbarAction } from './lib/toolbar';
  import type { AddRequest, DownloadRow, EditRequest, Settings } from './lib/types';

  type Dialog =
    | { kind: 'add' }
    | { kind: 'start'; url: string }
    | { kind: 'progress'; id: number }
    | { kind: 'properties'; id: number }
    | { kind: 'complete'; id: number }
    | { kind: 'delete'; ids: number[] }
    | { kind: 'options' }
    | { kind: 'error'; message: string };

  let settings = $state<Settings | null>(null);
  let selection = $state<SelectionState>({ selected: new Set(), anchor: null });
  let order = $state<SortOrder>({ column: 'added', ascending: false });
  let filter = $state<Filter>('all');
  let dialog = $state<Dialog | null>(null);
  let menu = $state<{ x: number; y: number } | null>(null);

  const rows = $derived(sortRows(filterRows(downloads.rows, filter), order));
  const selectedRows = $derived(rows.filter((row) => selection.selected.has(row.id)));
  const columns = $derived(resolveColumns(settings?.columns ?? []));
  const toolbarButtons = $derived(resolveToolbar(settings?.toolbar ?? []));
  const counts = $derived(filterCounts(downloads.rows));
  const running = $derived(downloads.rows.filter((row) => row.state === 'running').length);

  const canResume = (row: DownloadRow) => row.state === 'paused' || row.state === 'failed';
  const canStop = (row: DownloadRow) => row.state === 'running';

  onMount(() => {
    Promise.all([api.getSettings(), downloads.start()])
      .then(([loaded]) => {
        settings = loaded;
      })
      .catch(showError);
  });

  $effect(() => {
    if (settings) document.documentElement.dataset.theme = settings.theme;
  });

  // Rows that vanish, such as after a delete, must leave the selection too.
  $effect(() => {
    const ids = downloads.rows.map((row) => row.id);
    if ([...selection.selected].some((id) => !ids.includes(id))) {
      selection = keepExisting(selection, ids);
    }
  });

  // Shows finished downloads one at a time. A finished download takes over from its
  // own progress dialog, but waits for anything else that's open.
  $effect(() => {
    const id = downloads.justCompleted[0];
    if (id === undefined || !settings) return;

    const watchingIt = dialog?.kind === 'progress' && dialog.id === id;
    if (dialog && !watchingIt) return;

    downloads.justCompleted.shift();
    if (settings.showCompleteDialog && downloads.byId(id)) {
      dialog = { kind: 'complete', id };
    } else if (watchingIt) {
      dialog = null;
    }
  });

  function showError(error: unknown) {
    dialog = { kind: 'error', message: String(error) };
  }

  const closeDialog = () => {
    dialog = null;
  };

  /** Runs an action on several rows, showing the first failure. */
  async function forEachRow(items: DownloadRow[], action: (id: number) => Promise<void>) {
    const results = await Promise.allSettled(items.map((row) => action(row.id)));
    const failed = results.find((result) => result.status === 'rejected');
    if (failed) showError(failed.reason);
  }

  async function saveSettings(next: Settings) {
    try {
      await api.saveSettings(next);
      settings = next;
    } catch (error) {
      showError(error);
    }
  }

  function submitUrl(url: string) {
    if (settings?.showStartDialog) {
      dialog = { kind: 'start', url };
    } else {
      void addDownload({ url });
    }
  }

  async function addDownload(request: AddRequest) {
    closeDialog();
    try {
      await api.addDownload(request);
      await downloads.refresh();
      const fresh = await api.getSettings();
      if (settings) settings.urlHistory = fresh.urlHistory;
    } catch (error) {
      showError(error);
    }
  }

  async function saveEdit(id: number, edit: EditRequest) {
    closeDialog();
    try {
      await api.editDownload(id, edit);
      await downloads.refresh();
    } catch (error) {
      showError(error);
    }
  }

  async function deleteRows(ids: number[], deleteFiles: boolean) {
    closeDialog();
    const results = await Promise.allSettled(ids.map((id) => api.removeDownload(id, deleteFiles)));
    await downloads.refresh();
    const failed = results.find((result) => result.status === 'rejected');
    if (failed) showError(failed.reason);
  }

  function openRow(id: number) {
    const row = downloads.byId(id);
    if (!row) return;

    if (row.state === 'completed') {
      api.openFile(id).catch(showError);
    } else {
      dialog = { kind: 'progress', id };
    }
  }

  function runAction(action: ToolbarAction) {
    switch (action) {
      case 'add':
        dialog = { kind: 'add' };
        break;
      case 'resume':
        void forEachRow(selectedRows.filter(canResume), api.resumeDownload);
        break;
      case 'stop':
        void forEachRow(selectedRows.filter(canStop), api.pauseDownload);
        break;
      case 'stopAll':
        api.stopAll().catch(showError);
        break;
      case 'delete':
        if (selectedRows.length > 0) {
          dialog = { kind: 'delete', ids: selectedRows.map((row) => row.id) };
        }
        break;
      case 'deleteCompleted':
        void deleteRows(
          downloads.rows.filter((row) => row.state === 'completed').map((row) => row.id),
          false,
        );
        break;
      case 'options':
        dialog = { kind: 'options' };
        break;
    }
  }

  function actionEnabled(action: ToolbarAction): boolean {
    switch (action) {
      case 'resume':
        return selectedRows.some(canResume);
      case 'stop':
        return selectedRows.some(canStop);
      case 'stopAll':
        return running > 0;
      case 'delete':
        return selectedRows.length > 0;
      case 'deleteCompleted':
        return downloads.rows.some((row) => row.state === 'completed');
      default:
        return true;
    }
  }

  function sortBy(column: string) {
    order =
      order.column === column
        ? { column, ascending: !order.ascending }
        : { column, ascending: true };
  }

  function resizeColumn(id: string, width: number) {
    if (!settings) return;
    settings.columns = columns.map((column) => (column.id === id ? { ...column, width } : column));
  }

  const menuItems = $derived.by((): MenuItem[] => {
    const first = selectedRows[0];
    const single = selectedRows.length === 1 ? first : undefined;
    const finished = single?.state === 'completed';

    return [
      {
        label: 'Open file',
        disabled: !finished,
        run: () => single && api.openFile(single.id).catch(showError),
      },
      {
        label: 'Open folder',
        disabled: !single,
        run: () => single && api.showInFolder(single.id).catch(showError),
      },
      {
        label: 'Resume',
        separatorBefore: true,
        disabled: !selectedRows.some(canResume),
        run: () => runAction('resume'),
      },
      { label: 'Stop', disabled: !selectedRows.some(canStop), run: () => runAction('stop') },
      {
        label: 'Show progress',
        disabled: !single || finished,
        run: () => {
          if (single) dialog = { kind: 'progress', id: single.id };
        },
      },
      {
        label: 'Copy address',
        separatorBefore: true,
        disabled: !single,
        run: () => single && navigator.clipboard.writeText(single.url).catch(showError),
      },
      {
        label: 'Properties',
        disabled: !single,
        run: () => {
          if (single) dialog = { kind: 'properties', id: single.id };
        },
      },
      { label: 'Delete', separatorBefore: true, run: () => runAction('delete') },
    ];
  });

  function onKeyDown(event: KeyboardEvent) {
    if (dialog) return;
    const typing = event.target instanceof HTMLInputElement;

    if (event.ctrlKey && event.key.toLowerCase() === 'n') {
      event.preventDefault();
      runAction('add');
    } else if (event.ctrlKey && event.key.toLowerCase() === 'a' && !typing) {
      event.preventDefault();
      selection = { selected: new Set(rows.map((row) => row.id)), anchor: rows[0]?.id ?? null };
    } else if (event.key === 'Delete' && !typing) {
      runAction('delete');
    } else if (event.key === 'Enter' && !typing && selectedRows.length === 1 && selectedRows[0]) {
      openRow(selectedRows[0].id);
    }
  }

  const progressRow = $derived(
    dialog?.kind === 'progress' ? downloads.byId(dialog.id) : undefined,
  );
  const propertiesRow = $derived(
    dialog?.kind === 'properties' ? downloads.byId(dialog.id) : undefined,
  );
  const completeRow = $derived(
    dialog?.kind === 'complete' ? downloads.byId(dialog.id) : undefined,
  );
</script>

<svelte:window
  onkeydown={onKeyDown}
  oncontextmenu={(event) => {
    if (!(event.target instanceof HTMLInputElement)) event.preventDefault();
  }}
/>

<div class="app">
  {#if settings}
    <Toolbar
      buttons={toolbarButtons}
      size={settings.toolbarSize}
      enabled={actionEnabled}
      onaction={runAction}
    />
  {/if}

  <div class="body">
    <Sidebar filters={counts} active={filter} onchange={(next) => (filter = next)} />

    <main>
      {#if downloads.rows.length === 0}
        <div class="empty">
          <h1>No downloads yet</h1>
          <p>Add a web address to start your first download.</p>
          <button type="button" class="primary" onclick={() => runAction('add')}>Add URL</button>
        </div>
      {:else}
        <DownloadList
          {rows}
          {columns}
          {order}
          selected={selection.selected}
          onsort={sortBy}
          onrowclick={(id, event) => {
            selection = clickRow(
              selection,
              rows.map((row) => row.id),
              id,
              { ctrl: event.ctrlKey, shift: event.shiftKey },
            );
          }}
          onrowopen={openRow}
          onrowmenu={(id, event) => {
            selection = rightClickRow(selection, id);
            menu = { x: event.clientX, y: event.clientY };
          }}
          onresize={resizeColumn}
          onresized={() => {
            if (settings) void saveSettings($state.snapshot(settings) as Settings);
          }}
          onclear={() => {
            selection = { selected: new Set(), anchor: null };
          }}
        />
      {/if}
    </main>
  </div>

  <footer>
    <span>{running === 0 ? 'Idle' : `${running} downloading`}</span>
    {#if running > 0}<span>{formatSpeed(downloads.totalSpeed)}</span>{/if}
  </footer>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menuItems} onclose={() => (menu = null)} />
{/if}

{#if dialog?.kind === 'add'}
  <AddUrlDialog history={settings?.urlHistory ?? []} onsubmit={submitUrl} onclose={closeDialog} />
{:else if dialog?.kind === 'start'}
  <StartDialog
    url={dialog.url}
    defaultFolder={settings?.downloadDir ?? ''}
    onsubmit={addDownload}
    onclose={closeDialog}
  />
{:else if progressRow}
  <ProgressDialog
    row={progressRow}
    onpause={() => api.pauseDownload(progressRow.id).catch(showError)}
    onresume={() => api.resumeDownload(progressRow.id).catch(showError)}
    onclose={closeDialog}
  />
{:else if propertiesRow}
  <PropertiesDialog
    row={propertiesRow}
    onsave={(edit) => saveEdit(propertiesRow.id, edit)}
    onclose={closeDialog}
  />
{:else if completeRow}
  <CompleteDialog
    row={completeRow}
    onopen={() => {
      closeDialog();
      api.openFile(completeRow.id).catch(showError);
    }}
    onfolder={() => {
      closeDialog();
      api.showInFolder(completeRow.id).catch(showError);
    }}
    onclose={(hideInFuture) => {
      closeDialog();
      if (hideInFuture && settings) {
        void saveSettings({
          ...($state.snapshot(settings) as Settings),
          showCompleteDialog: false,
        });
      }
    }}
  />
{:else if dialog?.kind === 'delete'}
  {@const ids = dialog.ids}
  <DeleteDialog
    rows={downloads.rows.filter((row) => ids.includes(row.id))}
    onconfirm={(deleteFiles) => deleteRows(ids, deleteFiles)}
    onclose={closeDialog}
  />
{:else if dialog?.kind === 'options' && settings}
  <OptionsDialog
    {settings}
    onsave={(next) => {
      closeDialog();
      void saveSettings(next);
    }}
    onclose={closeDialog}
  />
{:else if dialog?.kind === 'error'}
  <Modal title="Something went wrong" onclose={closeDialog}>
    <p class="error-text">{dialog.message}</p>
    {#snippet footer()}
      <button type="button" class="primary" onclick={closeDialog}>Close</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  main {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .empty {
    display: grid;
    flex: 1;
    align-content: center;
    justify-items: center;
    gap: 4px;
    color: var(--muted);
  }

  .empty h1 {
    margin: 0;
    color: var(--text);
    font-size: 18px;
  }

  .empty p {
    margin: 0 0 12px;
  }

  footer {
    display: flex;
    justify-content: space-between;
    padding: 3px 12px;
    background: var(--surface);
    border-top: 1px solid var(--line);
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
</style>
