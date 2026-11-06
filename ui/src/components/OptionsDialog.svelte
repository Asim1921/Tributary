<script lang="ts">
  import { untrack } from 'svelte';

  import {
    deleteSiteLogin,
    pickFile,
    pickFolder,
    playSound,
    saveSiteLogin,
  } from '../lib/api';
  import { columnById, resolveColumns } from '../lib/columns';
  import { moveItem, resolveToolbar, TOOLBAR_BUTTONS } from '../lib/toolbar';
  import type { ColumnSetting, Settings } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    settings: Settings;
    onsave: (settings: Settings, proxyPassword: string) => void;
    onexport: () => void;
    onimport: () => void;
    onclose: () => void;
  }

  let { settings, onsave, onexport, onimport, onclose }: Props = $props();

  type Tab =
    | 'general'
    | 'downloads'
    | 'connection'
    | 'proxy'
    | 'logins'
    | 'categories'
    | 'security'
    | 'sounds'
    | 'columns'
    | 'toolbar';
  const TABS: { id: Tab; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'downloads', label: 'Downloads' },
    { id: 'connection', label: 'Connection' },
    { id: 'proxy', label: 'Proxy' },
    { id: 'logins', label: 'Logins' },
    { id: 'categories', label: 'Categories' },
    { id: 'security', label: 'Security' },
    { id: 'sounds', label: 'Sounds' },
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
  let proxyPassword = $state('');
  let newSite = $state({ pattern: '', username: '', password: '' });
  let loginProblem = $state('');

  const SOUND_EVENTS = [
    { key: 'complete', label: 'Download finished' },
    { key: 'failed', label: 'Download failed' },
    { key: 'queue', label: 'Queue finished' },
  ] as const;

  async function addSite() {
    const { pattern, username, password } = newSite;
    try {
      await saveSiteLogin(pattern.trim(), username.trim(), password);
      draft.siteLogins = [
        ...draft.siteLogins.filter((site) => site.pattern !== pattern.trim()),
        { pattern: pattern.trim(), username: username.trim() },
      ];
      newSite = { pattern: '', username: '', password: '' };
      loginProblem = '';
    } catch (error) {
      loginProblem = String(error);
    }
  }

  async function removeSite(pattern: string) {
    await deleteSiteLogin(pattern);
    draft.siteLogins = draft.siteLogins.filter((site) => site.pattern !== pattern);
  }

  async function browseSound(key: 'complete' | 'failed' | 'queue') {
    const chosen = await pickFile('Sound', ['wav']);
    if (chosen) draft.sounds[key] = chosen;
  }

  async function browseScanner() {
    const chosen = await pickFile('Program', ['exe']);
    if (chosen) draft.scannerPath = chosen;
  }

  async function browseDownloads() {
    const chosen = await pickFolder(draft.downloadDir);
    if (chosen) draft.downloadDir = chosen;
  }

  async function browseTemp() {
    const chosen = await pickFolder(draft.tempDir ?? undefined);
    if (chosen) draft.tempDir = chosen;
  }

  const splitList = (text: string) =>
    text
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean);

  function addCategory() {
    draft.categories.push({ name: 'New category', folder: null, extensions: [], sites: [] });
  }

  async function browseCategory(index: number) {
    const category = draft.categories[index];
    if (!category) return;
    const chosen = await pickFolder(category.folder ?? draft.downloadDir);
    if (chosen) category.folder = chosen;
  }

  function save() {
    onsave({
      ...$state.snapshot(draft),
      columns: $state.snapshot(columns),
      toolbar: toolbar.filter((entry) => entry.shown).map((entry) => entry.id),
    } as Settings, proxyPassword);
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
      <div class="data-buttons">
        <button type="button" onclick={onexport}>Export settings and list…</button>
        <button type="button" onclick={onimport}>Import…</button>
      </div>
      <p class="note">Exports never include passwords.</p>
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
        <input type="checkbox" bind:checked={draft.rememberLastFolder} />
        Offer the last folder used
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
    {:else if tab === 'connection'}
      <label class="check">
        <input type="checkbox" bind:checked={draft.speedLimitEnabled} />
        Limit download speed
      </label>
      <label class="field">
        Speed limit (KB per second, all downloads together)
        <input
          type="number"
          min="1"
          bind:value={draft.speedLimitKb}
          disabled={!draft.speedLimitEnabled}
        />
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={draft.quotaEnabled} />
        Limit how much is downloaded
      </label>
      <div class="pair">
        <label class="field">
          No more than (MB)
          <input type="number" min="1" bind:value={draft.quotaMb} disabled={!draft.quotaEnabled} />
        </label>
        <label class="field">
          Every (hours)
          <input type="number" min="1" bind:value={draft.quotaHours} disabled={!draft.quotaEnabled} />
        </label>
      </div>
      <p class="note">Downloads wait when the limit is reached, and carry on by themselves.</p>

      <label class="field">
        User agent (blank uses Tributary's own)
        <input type="text" bind:value={draft.userAgent} />
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.ftpPassive} />
        Use passive mode for FTP
      </label>

      <p class="heading">Connections for particular servers</p>
      {#each draft.serverRules as rule, index (index)}
        <div class="rule">
          <input type="text" aria-label="Server" bind:value={rule.pattern} placeholder="*.example.com" />
          <input type="number" aria-label="Connections" min="1" max="32" bind:value={rule.connections} />
          <button type="button" onclick={() => draft.serverRules.splice(index, 1)}>Remove</button>
        </div>
      {/each}
      <button
        type="button"
        onclick={() => draft.serverRules.push({ pattern: '', connections: 1 })}
      >
        Add server
      </button>
    {:else if tab === 'proxy'}
      <label class="field">
        Connect through
        <select bind:value={draft.proxyChoice}>
          <option value="system">Windows proxy settings</option>
          <option value="direct">No proxy</option>
          <option value="manual">This proxy</option>
        </select>
      </label>
      {#if draft.proxyChoice === 'manual'}
        <label class="field">
          Proxy address
          <input
            type="text"
            bind:value={draft.proxyUrl}
            placeholder="http://host:8080 or socks5://host:1080"
          />
        </label>
        <div class="pair">
          <label class="field">
            Proxy user name
            <input type="text" bind:value={draft.proxyUsername} autocomplete="off" />
          </label>
          <label class="field">
            Proxy password
            <input
              type="password"
              bind:value={proxyPassword}
              placeholder="Unchanged"
              autocomplete="off"
            />
          </label>
        </div>
        <label class="field">
          Don't use the proxy for (separated by commas)
          <input type="text" bind:value={draft.proxyExceptions} placeholder="localhost, 192.168.*" />
        </label>
      {/if}
    {:else if tab === 'logins'}
      <p class="note">Passwords are kept in Windows Credential Manager.</p>
      <ul>
        {#each draft.siteLogins as site (site.pattern)}
          <li>
            <span>{site.pattern} ({site.username})</span>
            <button type="button" aria-label="Remove {site.pattern}" onclick={() => removeSite(site.pattern)}>
              Remove
            </button>
          </li>
        {/each}
      </ul>
      <p class="heading">Add a site</p>
      <label class="field">
        Site
        <input type="text" bind:value={newSite.pattern} placeholder="*.example.com" />
      </label>
      <div class="pair">
        <label class="field">
          User name
          <input type="text" bind:value={newSite.username} autocomplete="off" />
        </label>
        <label class="field">
          Password
          <input type="password" bind:value={newSite.password} autocomplete="off" />
        </label>
      </div>
      <button
        type="button"
        disabled={!newSite.pattern.trim() || !newSite.username.trim()}
        onclick={addSite}
      >
        Add login
      </button>
      {#if loginProblem}<p class="error-text">{loginProblem}</p>{/if}
    {:else if tab === 'security'}
      <label class="check">
        <input type="checkbox" bind:checked={draft.scanEnabled} />
        Scan finished downloads for viruses
      </label>
      <div class="block">
        <label class="field" for="options-scanner">Scanner program</label>
        <div class="folder-row">
          <input id="options-scanner" type="text" bind:value={draft.scannerPath} />
          <button type="button" onclick={browseScanner}>Browse…</button>
        </div>
      </div>
      <label class="field">
        Scanner arguments
        <input type="text" bind:value={draft.scannerArguments} />
      </label>
      <p class="note">{'{file}'} stands for the downloaded file. Windows Defender is set up by default.</p>
    {:else if tab === 'sounds'}
      {#each SOUND_EVENTS as item (item.key)}
        <div class="block">
          <label class="field" for="sound-{item.key}">{item.label}</label>
          <div class="folder-row">
            <input
              id="sound-{item.key}"
              type="text"
              bind:value={draft.sounds[item.key]}
              placeholder="No sound"
            />
            <button type="button" onclick={() => browseSound(item.key)}>Browse…</button>
            <button
              type="button"
              disabled={!draft.sounds[item.key]}
              onclick={() => playSound(draft.sounds[item.key])}
            >
              Play
            </button>
          </div>
        </div>
      {/each}
    {:else if tab === 'categories'}
      <div class="categories">
        {#each draft.categories as category, index (index)}
          <div class="category">
            <label class="field">
              Name
              <input type="text" bind:value={category.name} />
            </label>
            <label class="field">
              File types, separated by commas
              <input
                type="text"
                value={category.extensions.join(', ')}
                onchange={(event) => (category.extensions = splitList(event.currentTarget.value))}
              />
            </label>
            <label class="field">
              Sites, such as *.example.com
              <input
                type="text"
                value={category.sites.join(', ')}
                onchange={(event) => (category.sites = splitList(event.currentTarget.value))}
              />
            </label>
            <div class="folder-row">
              <input
                type="text"
                aria-label="Folder for {category.name}"
                value={category.folder ?? ''}
                placeholder="Default folder"
                onchange={(event) => (category.folder = event.currentTarget.value.trim() || null)}
              />
              <button type="button" onclick={() => browseCategory(index)}>Browse…</button>
              <button type="button" onclick={() => draft.categories.splice(index, 1)}>Remove</button>
            </div>
          </div>
        {/each}
      </div>
      <button type="button" onclick={addCategory}>Add category</button>
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
    flex-wrap: wrap;
    gap: 2px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }

  .tabs button {
    padding: 6px 9px;
  }

  .heading {
    margin: 10px 0 6px;
    font-weight: 600;
  }

  .rule {
    display: grid;
    grid-template-columns: 1fr 80px auto;
    gap: 6px;
    margin-bottom: 6px;
  }

  .data-buttons {
    display: flex;
    gap: 8px;
    margin-top: 14px;
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

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .categories {
    max-height: 300px;
    margin-bottom: 8px;
    overflow-y: auto;
  }

  .category {
    margin-bottom: 10px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .category .field {
    margin-bottom: 6px;
    font-weight: 400;
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
