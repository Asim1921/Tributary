<script lang="ts">
  import { onMount } from 'svelte';

  import AddUrlDialog from './components/AddUrlDialog.svelte';
  import BatchDialog from './components/BatchDialog.svelte';
  import CompleteDialog from './components/CompleteDialog.svelte';
  import ContextMenu, { type MenuItem } from './components/ContextMenu.svelte';
  import DeleteDialog from './components/DeleteDialog.svelte';
  import DownloadList from './components/DownloadList.svelte';
  import LoginDialog from './components/LoginDialog.svelte';
  import Modal from './components/Modal.svelte';
  import OptionsDialog from './components/OptionsDialog.svelte';
  import ProgressDialog from './components/ProgressDialog.svelte';
  import PropertiesDialog from './components/PropertiesDialog.svelte';
  import SchedulerDialog from './components/SchedulerDialog.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import StartDialog from './components/StartDialog.svelte';
  import Toolbar from './components/Toolbar.svelte';
  import VideoDialog from './components/VideoDialog.svelte';
  import * as api from './lib/api';
  import { resolveColumns, sortRows, type SortOrder } from './lib/columns';
  import { downloads } from './lib/downloads.svelte';
  import {
    ALL,
    categoryEntries,
    filterKey,
    filterRows,
    queueEntries,
    statusEntries,
    type Filter,
  } from './lib/filters';
  import { droppedLink, formatSpeed, isStreamAddress } from './lib/format';
  import {
    clickRow,
    dragIds,
    keepExisting,
    rightClickRow,
    type SelectionState,
  } from './lib/selection';
  import { resolveToolbar, type ToolbarAction } from './lib/toolbar';
  import type {
    AddRequest,
    AddVideoRequest,
    CaptureEvent,
    CaptureItem,
    DownloadRow,
    EditRequest,
    Settings,
  } from './lib/types';

  type Dialog =
    | { kind: 'add' }
    | { kind: 'start'; url: string; token?: number; fileName?: string | null }
    | { kind: 'batch'; token: number; items: CaptureItem[] }
    | { kind: 'video'; url: string; token?: number; fileName?: string | null }
    | { kind: 'progress'; id: number }
    | { kind: 'properties'; id: number }
    | { kind: 'complete'; id: number }
    | { kind: 'delete'; ids: number[] }
    | { kind: 'options' }
    | { kind: 'scheduler' }
    | { kind: 'shutdown'; seconds: number }
    | { kind: 'login'; id: number }
    | { kind: 'scan'; file: string; problem: string }
    | { kind: 'info'; message: string }
    | { kind: 'error'; message: string };

  const MAIN_QUEUE = 1;

  let settings = $state<Settings | null>(null);
  let selection = $state<SelectionState>({ selected: new Set(), anchor: null });
  let order = $state<SortOrder>({ column: 'added', ascending: false });
  let filter = $state<Filter>(ALL);
  let dialog = $state<Dialog | null>(null);
  let menu = $state<{ x: number; y: number } | null>(null);
  /** Files from the browser, waiting for a free moment to ask about them. */
  let captures = $state<CaptureEvent[]>([]);

  const rows = $derived(sortRows(filterRows(downloads.rows, filter), order));
  const selectedRows = $derived(rows.filter((row) => selection.selected.has(row.id)));
  const columns = $derived(resolveColumns(settings?.columns ?? []));
  const toolbarButtons = $derived(resolveToolbar(settings?.toolbar ?? []));
  const sidebarGroups = $derived([
    { title: 'Status', entries: statusEntries(downloads.rows) },
    { title: 'Categories', entries: categoryEntries(downloads.rows, settings?.categories ?? []) },
    { title: 'Queues', entries: queueEntries(downloads.rows, downloads.queues) },
  ]);
  /** The queue the toolbar acts on: the one being viewed, otherwise the main queue. */
  const targetQueue = $derived(
    downloads.queues.find((queue) => queue.id === (filter.kind === 'queue' ? filter.id : MAIN_QUEUE)),
  );
  const defaultFolder = $derived(
    settings?.rememberLastFolder && settings.lastFolder
      ? settings.lastFolder
      : (settings?.downloadDir ?? ''),
  );
  const running = $derived(downloads.rows.filter((row) => row.state === 'running').length);

  const canResume = (row: DownloadRow) => row.state === 'paused' || row.state === 'failed';
  const canStop = (row: DownloadRow) => row.state === 'running';

  onMount(() => {
    Promise.all([api.getSettings(), downloads.start()])
      .then(([loaded]) => {
        settings = loaded;
      })
      .catch(showError);

    void api.onShutdownPending((seconds) => {
      dialog = { kind: 'shutdown', seconds };
    });
    void api.onScanFinished(({ file, problem }) => {
      if (problem) dialog = { kind: 'scan', file, problem };
    });
    void api.onVideoProblem(showError);
    // Listening comes first. The app may have been started just to receive a file.
    api
      .onCapture((event) => captures.push(event))
      .then(() => api.windowReady())
      .then((missed) => captures.push(...missed))
      .catch(showError);
  });

  // Asks about captured files one at a time, when nothing else is open.
  $effect(() => {
    if (dialog || captures.length === 0 || !settings) return;

    const event = captures.shift();
    const first = event?.items[0];
    if (!event || !first) return;

    if (event.batch) {
      dialog = { kind: 'batch', token: event.token, items: event.items };
    } else {
      const kind = first.video ? 'video' : 'start';
      dialog = { kind, url: first.url, token: event.token, fileName: first.fileName };
    }
  });

  // Asks for a login when a download needs one.
  $effect(() => {
    if (dialog || downloads.needLogin.length === 0) return;
    const id = downloads.needLogin.shift();
    if (id !== undefined && downloads.byId(id)) dialog = { kind: 'login', id };
  });

  // Counts down the shutdown message.
  $effect(() => {
    if (dialog?.kind !== 'shutdown') return;

    const timer = setInterval(() => {
      if (dialog?.kind === 'shutdown' && dialog.seconds > 0) {
        dialog = { kind: 'shutdown', seconds: dialog.seconds - 1 };
      }
    }, 1000);
    return () => clearInterval(timer);
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

  async function saveSettings(next: Settings, proxyPassword = '') {
    try {
      await api.saveSettings(next);
      if (proxyPassword) await api.setProxyPassword(proxyPassword);
      settings = next;
    } catch (error) {
      showError(error);
    }
  }

  async function exportData() {
    try {
      const path = await api.pickSavePath();
      if (!path) return;
      const count = await api.exportData(path);
      dialog = { kind: 'info', message: `Exported your settings and ${count} downloads.` };
    } catch (error) {
      showError(error);
    }
  }

  async function importData() {
    try {
      const path = await api.pickFile('Tributary export', ['json']);
      if (!path) return;
      const added = await api.importData(path);
      settings = await api.getSettings();
      await downloads.refresh();
      dialog = { kind: 'info', message: `Imported your settings and added ${added} downloads.` };
    } catch (error) {
      showError(error);
    }
  }

  function submitUrl(url: string) {
    if (isStreamAddress(url)) {
      dialog = { kind: 'video', url };
    } else if (settings?.showStartDialog) {
      dialog = { kind: 'start', url };
    } else {
      void addDownload({ url });
    }
  }

  async function addVideo(request: AddVideoRequest) {
    closeDialog();
    try {
      await api.addVideo(request);
      await downloads.refresh();
    } catch (error) {
      showError(error);
    }
  }

  async function addDownload(request: AddRequest) {
    closeDialog();
    try {
      await api.addDownload(request);
      await downloads.refresh();
      const fresh = await api.getSettings();
      if (settings) {
        settings.urlHistory = fresh.urlHistory;
        settings.lastFolder = fresh.lastFolder;
      }
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
      case 'scheduler':
        dialog = { kind: 'scheduler' };
        break;
      case 'startQueue':
        if (targetQueue) api.startQueue(targetQueue.id).catch(showError);
        break;
      case 'stopQueue':
        if (targetQueue) api.stopQueue(targetQueue.id).catch(showError);
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
      case 'startQueue':
        return !!targetQueue && !targetQueue.active && targetQueue.files.length > 0;
      case 'stopQueue':
        return !!targetQueue?.active;
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
      ...downloads.queues.map(
        (queue, index): MenuItem => ({
          label: `Add to ${queue.name}`,
          separatorBefore: index === 0,
          disabled: selectedRows.every((row) => row.queueId === queue.id),
          run: () =>
            void forEachRow(selectedRows, (id) => api.setDownloadQueue(id, queue.id)),
        }),
      ),
      {
        label: 'Remove from queue',
        disabled: selectedRows.every((row) => row.queueId === null),
        run: () => void forEachRow(selectedRows, (id) => api.setDownloadQueue(id, null)),
      },
      {
        label: 'Copy address',
        separatorBefore: true,
        disabled: !single,
        run: () => single && navigator.clipboard.writeText(single.url).catch(showError),
      },
      {
        label: 'Enter login…',
        disabled: single?.state !== 'failed',
        run: () => {
          if (single) dialog = { kind: 'login', id: single.id };
        },
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

  function onDrop(event: DragEvent) {
    event.preventDefault();
    const url = droppedLink(event.dataTransfer);
    if (url && !dialog) submitUrl(url);
  }

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
  ondragover={(event) => event.preventDefault()}
  ondrop={onDrop}
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
    <Sidebar
      groups={sidebarGroups}
      activeKey={filterKey(filter)}
      onchange={(next) => (filter = next)}
    />

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
          onrowdrag={(id) => {
            const ids = dragIds(rows, selection.selected, id);
            if (ids.length > 0) api.dragFiles(ids).catch(showError);
          }}
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
    captureToken={dialog.token}
    suggestedName={dialog.fileName}
    {defaultFolder}
    categories={settings?.categories ?? []}
    queues={downloads.queues}
    onsubmit={addDownload}
    onstream={() => {
      if (dialog?.kind === 'start') dialog = { ...dialog, kind: 'video' };
    }}
    onclose={() => {
      const token = dialog?.kind === 'start' ? dialog.token : undefined;
      closeDialog();
      if (token !== undefined) void api.discardCapture(token);
    }}
  />
{:else if dialog?.kind === 'video'}
  <VideoDialog
    url={dialog.url}
    captureToken={dialog.token}
    suggestedName={dialog.fileName}
    {defaultFolder}
    onsubmit={addVideo}
    onclose={() => {
      const token = dialog?.kind === 'video' ? dialog.token : undefined;
      closeDialog();
      if (token !== undefined) void api.discardCapture(token);
    }}
  />
{:else if dialog?.kind === 'batch'}
  <BatchDialog
    items={dialog.items}
    {defaultFolder}
    queues={downloads.queues}
    onsubmit={(urls, folder, queueId, later) => {
      const token = dialog?.kind === 'batch' ? dialog.token : 0;
      closeDialog();
      api
        .addBatch(token, urls, folder, queueId, later)
        .then(() => downloads.refresh())
        .catch(showError);
    }}
    onclose={() => {
      const token = dialog?.kind === 'batch' ? dialog.token : 0;
      closeDialog();
      void api.discardCapture(token);
    }}
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
      const { id } = completeRow;
      closeDialog();
      api.openFile(id).catch(showError);
    }}
    onfolder={() => {
      const { id } = completeRow;
      closeDialog();
      api.showInFolder(id).catch(showError);
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
    onconfirm={(deleteFiles) => {
      const chosen = [...ids];
      void deleteRows(chosen, deleteFiles);
    }}
    onclose={closeDialog}
  />
{:else if dialog?.kind === 'options' && settings}
  <OptionsDialog
    {settings}
    onsave={(next, proxyPassword) => {
      closeDialog();
      void saveSettings(next, proxyPassword);
    }}
    onexport={exportData}
    onimport={importData}
    onclose={closeDialog}
  />
{:else if dialog?.kind === 'scheduler'}
  <SchedulerDialog
    queues={downloads.queues}
    rows={downloads.rows}
    onsave={async (queue) => {
      const id = await api.saveQueue(queue);
      await downloads.refresh();
      return id;
    }}
    ondelete={(id) => api.deleteQueue(id).catch(showError)}
    onstart={(id) => api.startQueue(id).catch(showError)}
    onstop={(id) => api.stopQueue(id).catch(showError)}
    onreorder={(ids) => api.reorderQueue(ids).catch(showError)}
    onremovefile={(id) => api.setDownloadQueue(id, null).catch(showError)}
    onclose={closeDialog}
  />
{:else if dialog?.kind === 'shutdown'}
  <Modal title="Shutting down" onclose={closeDialog}>
    <p>A queue finished. This computer will shut down in {dialog.seconds} seconds.</p>
    {#snippet footer()}
      <button
        type="button"
        class="primary"
        onclick={() => {
          closeDialog();
          api.cancelShutdown().catch(showError);
        }}
      >
        Cancel shutdown
      </button>
    {/snippet}
  </Modal>
{:else if dialog?.kind === 'login'}
  {@const row = downloads.byId(dialog.id)}
  {#if row}
    <LoginDialog
      {row}
      onsubmit={(username, password, remember) => {
        const { id } = row;
        closeDialog();
        api.loginAndResume(id, username, password, remember).catch(showError);
      }}
      onclose={closeDialog}
    />
  {/if}
{:else if dialog?.kind === 'scan'}
  <Modal title="Virus scan warning" onclose={closeDialog}>
    <p class="error-text">{dialog.problem}</p>
    <p class="note selectable">{dialog.file}</p>
    {#snippet footer()}
      <button type="button" class="primary" onclick={closeDialog}>Close</button>
    {/snippet}
  </Modal>
{:else if dialog?.kind === 'info'}
  <Modal title="Done" onclose={closeDialog}>
    <p>{dialog.message}</p>
    {#snippet footer()}
      <button type="button" class="primary" onclick={closeDialog}>Close</button>
    {/snippet}
  </Modal>
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
