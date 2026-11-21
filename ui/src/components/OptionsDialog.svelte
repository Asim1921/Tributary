<script lang="ts">
  import { mark, sayError, t, tMarkup, useLanguage } from '../lib/locale.svelte';
  import { LANGUAGES } from '../lib/i18n';
  import { untrack } from 'svelte';

  import {
    appVersion,
    checkForUpdate,
    chooseForMagnetLinks,
    deleteSiteLogin,
    extensionFolder,
    installYtdlp,
    newPhoneCode,
    phoneStatus,
    setPhone,
    pickFile,
    pickFolder,
    playSound,
    saveSiteLogin,
    appLanguage,
  } from '../lib/api';
  import { columnById, resolveColumns } from '../lib/columns';
  import { moveItem, resolveToolbar, TOOLBAR_BUTTONS } from '../lib/toolbar';
  import type { ColumnSetting, PhoneStatus, Settings } from '../lib/types';
  import Modal from './Modal.svelte';

  interface Props {
    settings: Settings;
    onsave: (settings: Settings, proxyPassword: string) => void;
    onexport: () => void;
    onimport: () => void;
    onclose: () => void;
  }

  let { settings, onsave, onexport, onimport, onclose }: Props = $props();

  let version = $state('');
  let phone = $state<PhoneStatus | null>(null);
  let phoneBusy = $state(false);
  phoneStatus()
    .then((found) => (phone = found))
    .catch(() => {});

  /** Turns the phone page on or off at once, rather than on Save. */
  async function changePhone(work: () => Promise<PhoneStatus>) {
    phoneBusy = true;
    try {
      phone = await work();
    } catch (error) {
      phone = { running: false, address: null, qrCode: null, problem: sayError(error) };
    } finally {
      phoneBusy = false;
    }
  }
  let looking = $state(false);
  let looked = $state<string | null>(null);
  appVersion()
    .then((found) => (version = found))
    .catch(() => {});

  /** Looks for a newer release now. One found shows in the bar above the list. */
  /** Closes without saving, back in the language the app speaks. */
  function cancel() {
    if (draft.language !== settings.language) void appLanguage().then(useLanguage);
    onclose();
  }

  async function lookForUpdate() {
    looking = true;
    looked = null;
    try {
      const found = await checkForUpdate();
      looked = found
        ? t('Version {version} is ready to install.', { version: found.version })
        : t('You have the newest version.');
    } catch (error) {
      looked = t("Couldn't check: {error}", { error: sayError(error) });
    } finally {
      looking = false;
    }
  }

  type Tab =
    | 'general'
    | 'downloads'
    | 'browser'
    | 'video'
    | 'connection'
    | 'proxy'
    | 'logins'
    | 'categories'
    | 'security'
    | 'phone'
    | 'sounds'
    | 'columns'
    | 'toolbar';
  const TABS: { id: Tab; label: string }[] = [
    { id: 'general', label: mark('General') },
    { id: 'downloads', label: mark('Downloads') },
    { id: 'browser', label: mark('Browser') },
    { id: 'video', label: mark('Video') },
    { id: 'connection', label: mark('Connection') },
    { id: 'proxy', label: mark('Proxy') },
    { id: 'logins', label: mark('Logins') },
    { id: 'categories', label: mark('Categories') },
    { id: 'security', label: mark('Security') },
    { id: 'phone', label: mark('Phone') },
    { id: 'sounds', label: mark('Sounds') },
    { id: 'columns', label: mark('Columns') },
    { id: 'toolbar', label: mark('Toolbar') },
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
  let extensionPath = $state('');

  $effect(() => {
    if (tab === 'browser' && !extensionPath) {
      void extensionFolder().then((path) => (extensionPath = path));
    }
  });

  const KEYS = [
    { value: 'ctrl', label: mark('Ctrl') },
    { value: 'alt', label: mark('Alt') },
    { value: 'shift', label: mark('Shift') },
    { value: 'none', label: mark('None') },
  ];

  const SOUND_EVENTS = [
    { key: 'complete', label: mark('Download finished') },
    { key: 'failed', label: mark('Download failed') },
    { key: 'queue', label: mark('Queue finished') },
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
      loginProblem = sayError(error);
    }
  }

  async function removeSite(pattern: string) {
    await deleteSiteLogin(pattern);
    draft.siteLogins = draft.siteLogins.filter((site) => site.pattern !== pattern);
  }

  async function browseSound(key: 'complete' | 'failed' | 'queue') {
    const chosen = await pickFile(t('Sound'), ['wav']);
    if (chosen) draft.sounds[key] = chosen;
  }

  async function browseScanner() {
    const chosen = await pickFile(t('Program'), ['exe']);
    if (chosen) draft.scannerPath = chosen;
  }

  let installingYtdlp = $state(false);
  let ytdlpMessage = $state<string | null>(null);

  async function installTool() {
    installingYtdlp = true;
    ytdlpMessage = null;
    try {
      await installYtdlp();
      ytdlpMessage = t('yt-dlp is installed and up to date.');
    } catch (error) {
      ytdlpMessage = sayError(error);
    } finally {
      installingYtdlp = false;
    }
  }

  async function browseYtdlp() {
    const chosen = await pickFile(t('Program'), ['exe']);
    if (chosen) draft.ytdlpPath = chosen;
  }

  async function browseFfmpeg() {
    const chosen = await pickFile(t('Program'), ['exe']);
    if (chosen) draft.ffmpegPath = chosen;
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
    draft.categories.push({ name: t('New category'), folder: null, extensions: [], sites: [] });
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

<Modal title={t('Options')} wide onclose={cancel}>
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

  <div class="panel">
    {#if tab === 'general'}
      <div class="about">
        <span>{t('Tributary {version}', { version })}</span>
        <button type="button" disabled={looking} onclick={lookForUpdate}>
          {looking ? t('Looking…') : t('Check for updates')}
        </button>
        {#if looked}<span class="note" role="status">{looked}</span>{/if}
      </div>
      <label class="check">
        <input type="checkbox" bind:checked={draft.checkForUpdates} />
        {t('Check for updates automatically')}
      </label>
      <label class="field">
        {t('Language')}
        <select
          bind:value={draft.language}
          onchange={() => void appLanguage(draft.language).then(useLanguage)}
        >
          <option value="">{t('The same as Windows')}</option>
          {#each LANGUAGES as language (language.code)}
            <option value={language.code} lang={language.code}>{language.name}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        {t('Theme')}
        <select bind:value={draft.theme}>
          <option value="system">{t('Match Windows')}</option>
          <option value="light">{t('Light')}</option>
          <option value="dark">{t('Dark')}</option>
        </select>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.startWithWindows} />
        {t('Start with Windows')}
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.closeToTray} />
        {t('Keep running in the tray when the window is closed')}
      </label>
      <div class="data-buttons">
        <button type="button" onclick={onexport}>{t('Export settings and list…')}</button>
        <button type="button" onclick={onimport}>{t('Import…')}</button>
      </div>
      <p class="note">{t('Exports never include passwords.')}</p>
    {:else if tab === 'downloads'}
      <div class="block">
        <label class="field" for="options-folder">{t('Default folder')}</label>
        <div class="folder-row">
          <input id="options-folder" type="text" bind:value={draft.downloadDir} />
          <button type="button" onclick={browseDownloads}>{t('Browse…')}</button>
        </div>
      </div>

      <div class="block">
        <label class="field" for="options-temp">{t('Temporary folder')}</label>
        <div class="folder-row">
          <input
            id="options-temp"
            type="text"
            value={draft.tempDir ?? ''}
            placeholder={t('Same as the destination')}
            readonly
          />
          <button type="button" onclick={browseTemp}>{t('Browse…')}</button>
          <button type="button" disabled={!draft.tempDir} onclick={() => (draft.tempDir = null)}>
            {t('Clear')}
          </button>
        </div>
        <p class="note">{t('Partial files go here. Applies to new downloads.')}</p>
      </div>

      <label class="field">
        {t('Connections per download')}
        <input type="number" min="1" max="32" bind:value={draft.connections} />
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={draft.rememberLastFolder} />
        {t('Offer the last folder used')}
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.showStartDialog} />
        {t('Ask before starting a download')}
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.showCompleteDialog} />
        {t('Show a message when a download finishes')}
      </label>
      <p class="note">
        {t("To open magnet links in Tributary, choose it for them in Windows' Default apps.")}
        <button type="button" onclick={() => void chooseForMagnetLinks()}>{t('Choose Tributary…')}</button>
      </p>
      <label class="check">
        <input type="checkbox" bind:checked={draft.notifyInBackground} />
        {t('Show a Windows notification when downloads finish while Tributary is in the background')}
      </label>

      <button
        type="button"
        disabled={draft.urlHistory.length === 0}
        onclick={() => (draft.urlHistory = [])}
      >
        {t('Clear address history')}
      </button>
    {:else if tab === 'browser'}
      <label class="check">
        <input type="checkbox" bind:checked={draft.captureEnabled} />
        {t('Take over downloads from the browser')}
      </label>
      <label class="field">
        {t('File types to take over, separated by commas')}
        <textarea
          rows="3"
          value={draft.captureTypes.join(', ')}
          onchange={(event) => (draft.captureTypes = splitList(event.currentTarget.value))}
        ></textarea>
      </label>
      <label class="field">
        {t('Sites and addresses to leave to the browser, separated by commas')}
        <input
          type="text"
          value={draft.captureExclusions.join(', ')}
          placeholder={t('*.example.com')}
          onchange={(event) => (draft.captureExclusions = splitList(event.currentTarget.value))}
        />
      </label>
      <div class="pair">
        <label class="field">
          {t('Hold to always take over')}
          <select bind:value={draft.forceKey}>
            {#each KEYS as key (key.value)}
              <option value={key.value}>{t(key.label)}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          {t('Hold to leave to the browser')}
          <select bind:value={draft.skipKey}>
            {#each KEYS as key (key.value)}
              <option value={key.value}>{t(key.label)}</option>
            {/each}
          </select>
        </label>
      </div>
      <label class="check">
        <input type="checkbox" bind:checked={draft.watchClipboard} />
        {t('Offer to download links copied to the clipboard')}
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.showDropTarget} />
        {t('Show a small window to drop links on')}
      </label>

      <p class="heading">{t('Setting up your browser')}</p>
      <ol class="steps">
        <!-- The app's own text: see tMarkup. -->
        <li>{@html tMarkup('Open <strong>chrome://extensions</strong>, or <strong>edge://extensions</strong> in Edge.')}</li>
        <li>{@html tMarkup('Turn on <strong>Developer mode</strong>.')}</li>
        <li>{@html tMarkup('Choose <strong>Load unpacked</strong> and select this folder:')}</li>
      </ol>
      <p class="selectable path">{extensionPath}</p>
    {:else if tab === 'video'}
      <label class="check">
        <input type="checkbox" bind:checked={draft.videoPanel} />
        {t('Show a download panel over video players')}
      </label>
      <div class="block">
        <label class="field" for="options-ffmpeg">{t('ffmpeg program')}</label>
        <div class="folder-row">
          <input
            id="options-ffmpeg"
            type="text"
            bind:value={draft.ffmpegPath}
            placeholder={t("Found automatically if it's installed")}
          />
          <button type="button" onclick={browseFfmpeg}>{t('Browse…')}</button>
        </div>
      </div>
      <div class="block">
        <label class="field" for="options-ytdlp">{t('yt-dlp program')}</label>
        <div class="folder-row">
          <input
            id="options-ytdlp"
            type="text"
            bind:value={draft.ytdlpPath}
            placeholder={t('The copy Tributary installs, or one on your PATH')}
          />
          <button type="button" onclick={browseYtdlp}>{t('Browse…')}</button>
          <button type="button" disabled={installingYtdlp} onclick={installTool}>
            {installingYtdlp ? t('Installing…') : t('Install or update')}
          </button>
        </div>
        {#if ytdlpMessage}<p class="note" role="status">{ytdlpMessage}</p>{/if}
      </div>
      <p class="note">
        {t("yt-dlp finds the video on sites like YouTube, where it's fed to the player by script. It is only used for the video panel and Add URL on such pages, and never sees your cookies.")}
      </p>
      <p class="note">
        {t("Some videos keep their picture and sound apart. With ffmpeg they're joined into one file. Without it, they stay as two files. Protected and live videos can't be saved.")}
      </p>
    {:else if tab === 'connection'}
      <label class="check">
        <input type="checkbox" bind:checked={draft.speedLimitEnabled} />
        {t('Limit download speed')}
      </label>
      <label class="field">
        {t('Speed limit (KB per second, all downloads together)')}
        <input
          type="number"
          min="1"
          bind:value={draft.speedLimitKb}
          disabled={!draft.speedLimitEnabled}
        />
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={draft.quotaEnabled} />
        {t('Limit how much is downloaded')}
      </label>
      <div class="pair">
        <label class="field">
          {t('No more than (MB)')}
          <input type="number" min="1" bind:value={draft.quotaMb} disabled={!draft.quotaEnabled} />
        </label>
        <label class="field">
          {t('Every (hours)')}
          <input type="number" min="1" bind:value={draft.quotaHours} disabled={!draft.quotaEnabled} />
        </label>
      </div>
      <p class="note">{t('Downloads wait when the limit is reached, and carry on by themselves.')}</p>

      <label class="field">
        {t("User agent (blank uses Tributary's own)")}
        <input type="text" bind:value={draft.userAgent} />
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.ftpPassive} />
        {t('Use passive mode for FTP')}
      </label>

      <p class="heading">{t('Connections for particular servers')}</p>
      {#each draft.serverRules as rule, index (index)}
        <div class="rule">
          <input type="text" aria-label={t('Server')} bind:value={rule.pattern} placeholder={t('*.example.com')} />
          <input type="number" aria-label={t('Connections')} min="1" max="32" bind:value={rule.connections} />
          <button type="button" onclick={() => draft.serverRules.splice(index, 1)}>{t('Remove')}</button>
        </div>
      {/each}
      <button
        type="button"
        onclick={() => draft.serverRules.push({ pattern: '', connections: 1 })}
      >
        {t('Add server')}
      </button>
    {:else if tab === 'proxy'}
      <label class="field">
        {t('Connect through')}
        <select bind:value={draft.proxyChoice}>
          <option value="system">{t('Windows proxy settings')}</option>
          <option value="direct">{t('No proxy')}</option>
          <option value="manual">{t('This proxy')}</option>
        </select>
      </label>
      {#if draft.proxyChoice === 'manual'}
        <label class="field">
          {t('Proxy address')}
          <input
            type="text"
            bind:value={draft.proxyUrl}
            placeholder={t('http://host:8080 or socks5://host:1080')}
          />
        </label>
        <div class="pair">
          <label class="field">
            {t('Proxy user name')}
            <input type="text" bind:value={draft.proxyUsername} autocomplete="off" />
          </label>
          <label class="field">
            {t('Proxy password')}
            <input
              type="password"
              bind:value={proxyPassword}
              placeholder={t('Unchanged')}
              autocomplete="off"
            />
          </label>
        </div>
        <label class="field">
          {t("Don't use the proxy for (separated by commas)")}
          <input type="text" bind:value={draft.proxyExceptions} placeholder={t('localhost, 192.168.*')} />
        </label>
      {/if}
    {:else if tab === 'logins'}
      <p class="note">{t('Passwords are kept in Windows Credential Manager.')}</p>
      <ul>
        {#each draft.siteLogins as site (site.pattern)}
          <li>
            <span>{site.pattern} ({site.username})</span>
            <button type="button" aria-label="Remove {site.pattern}" onclick={() => removeSite(site.pattern)}>
              {t('Remove')}
            </button>
          </li>
        {/each}
      </ul>
      <p class="heading">{t('Add a site')}</p>
      <label class="field">
        {t('Site')}
        <input type="text" bind:value={newSite.pattern} placeholder={t('*.example.com')} />
      </label>
      <div class="pair">
        <label class="field">
          {t('User name')}
          <input type="text" bind:value={newSite.username} autocomplete="off" />
        </label>
        <label class="field">
          {t('Password')}
          <input type="password" bind:value={newSite.password} autocomplete="off" />
        </label>
      </div>
      <button
        type="button"
        disabled={!newSite.pattern.trim() || !newSite.username.trim()}
        onclick={addSite}
      >
        {t('Add login')}
      </button>
      {#if loginProblem}<p class="error-text">{loginProblem}</p>{/if}
    {:else if tab === 'phone'}
      <label class="check">
        <input
          type="checkbox"
          checked={phone?.running ?? false}
          disabled={phoneBusy}
          onchange={(event) => {
            const on = event.currentTarget.checked;
            void changePhone(() => setPhone(on));
          }}
        />
        {t('Let my phone send links over the home network')}
      </label>
      <p class="note">
        {t("Your phone opens a small page from this computer to send it links, which Tributary then asks you about here, and to see how downloads are getting on. Only a phone on your home network with the pairing code in this QR code can use it, and it can't reach your files or settings. Windows may ask whether Tributary can use the network: allow private networks.")}
      </p>
      {#if phone?.problem}<p class="error-text" role="alert">{sayError(phone.problem)}</p>{/if}
      {#if phone?.running && phone.qrCode}
        <div class="qr" aria-label={t('QR code for your phone')}>
          <!-- Made by the app from the address alone, so it's safe to show as it is. -->
          {@html phone.qrCode}
        </div>
        <p class="selectable note">{phone.address}</p>
        <button type="button" disabled={phoneBusy} onclick={() => void changePhone(newPhoneCode)}>
          {t('New pairing code')}
        </button>
      {/if}
    {:else if tab === 'security'}
      <label class="check">
        <input type="checkbox" bind:checked={draft.markDownloads} />
        {t('Mark downloads as coming from the internet, so Windows checks them before they open (recommended)')}
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={draft.scanEnabled} />
        {t('Scan finished downloads for viruses')}
      </label>
      <div class="block">
        <label class="field" for="options-scanner">{t('Scanner program')}</label>
        <div class="folder-row">
          <input id="options-scanner" type="text" bind:value={draft.scannerPath} />
          <button type="button" onclick={browseScanner}>{t('Browse…')}</button>
        </div>
      </div>
      <label class="field">
        {t('Scanner arguments')}
        <input type="text" bind:value={draft.scannerArguments} />
      </label>
      <p class="note">
        {t('{file} stands for the downloaded file. Windows Defender is set up by default.', { file: '{file}' })}
      </p>
    {:else if tab === 'sounds'}
      {#each SOUND_EVENTS as item (item.key)}
        <div class="block">
          <label class="field" for="sound-{item.key}">{t(item.label)}</label>
          <div class="folder-row">
            <input
              id="sound-{item.key}"
              type="text"
              bind:value={draft.sounds[item.key]}
              placeholder={t('No sound')}
            />
            <button type="button" onclick={() => browseSound(item.key)}>{t('Browse…')}</button>
            <button
              type="button"
              disabled={!draft.sounds[item.key]}
              onclick={() => playSound(draft.sounds[item.key])}
            >
              {t('Play')}
            </button>
          </div>
        </div>
      {/each}
    {:else if tab === 'categories'}
      <div class="categories">
        {#each draft.categories as category, index (index)}
          <div class="category">
            <label class="field">
              {t('Name')}
              <input type="text" bind:value={category.name} />
            </label>
            <label class="field">
              {t('File types, separated by commas')}
              <input
                type="text"
                value={category.extensions.join(', ')}
                onchange={(event) => (category.extensions = splitList(event.currentTarget.value))}
              />
            </label>
            <label class="field">
              {t('Sites, such as *.example.com')}
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
                placeholder={t('Default folder')}
                onchange={(event) => (category.folder = event.currentTarget.value.trim() || null)}
              />
              <button type="button" onclick={() => browseCategory(index)}>{t('Browse…')}</button>
              <button type="button" onclick={() => draft.categories.splice(index, 1)}>{t('Remove')}</button>
            </div>
          </div>
        {/each}
      </div>
      <button type="button" onclick={addCategory}>{t('Add category')}</button>
    {:else if tab === 'columns'}
      <ul>
        {#each columns as column, index (column.id)}
          <li>
            <label class="check">
              <input type="checkbox" bind:checked={column.visible} disabled={column.id === 'name'} />
              {t(columnById(column.id)?.label ?? '')}
            </label>
            <span class="moves">
              <button
                type="button"
                aria-label={t('Move up')}
                disabled={index === 0}
                onclick={() => (columns = moveItem(columns, index, -1))}>↑</button
              >
              <button
                type="button"
                aria-label={t('Move down')}
                disabled={index === columns.length - 1}
                onclick={() => (columns = moveItem(columns, index, 1))}>↓</button
              >
            </span>
          </li>
        {/each}
      </ul>
    {:else}
      <label class="field">
        {t('Button size')}
        <select bind:value={draft.toolbarSize}>
          <option value="large">{t('Large, with labels')}</option>
          <option value="small">{t('Small, icons only')}</option>
        </select>
      </label>
      <ul>
        {#each toolbar as entry, index (entry.id)}
          <li>
            <label class="check">
              <input type="checkbox" bind:checked={entry.shown} />
              {t(entry.label)}
            </label>
            <span class="moves">
              <button
                type="button"
                aria-label={t('Move up')}
                disabled={index === 0}
                onclick={() => (toolbar = moveItem(toolbar, index, -1))}>↑</button
              >
              <button
                type="button"
                aria-label={t('Move down')}
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
    <button type="button" onclick={cancel}>{t('Cancel')}</button>
    <button type="button" class="primary" onclick={save}>{t('Save')}</button>
  {/snippet}
</Modal>

<style>
  .qr {
    width: 200px;
    margin: 8px 0;
    padding: 8px;
    background: #fff;
    border-radius: 6px;
  }

  .qr :global(svg) {
    display: block;
    width: 100%;
    height: auto;
  }

  .about {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-bottom: 12px;
  }

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

  .steps {
    margin: 0 0 6px;
    padding-inline-start: 20px;
  }

  .path {
    padding: 6px 8px;
    background: var(--surface-2);
    border-radius: 6px;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
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
