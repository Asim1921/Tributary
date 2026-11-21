/**
 * Every word the site shows, in English. The other languages are the same shape, so
 * TypeScript says when one is missing a text. `{name}` is filled in; `<b>`, `<key>` and
 * `<code>` mark words within a sentence (see Rich).
 */
export const en = {
  meta: {
    title: "Tributary: a download manager for Windows",
    description:
      "Tributary splits each download across several connections, resumes after a crash, takes over downloads from Chrome and Edge, and saves video from the pages you watch. Free, for Windows.",
    downloadTitle: "Download",
    downloadDescription:
      "Download Tributary for Windows 10 and 11: the installer and its SHA-256 checksum, how to install it, why SmartScreen warns, and how to connect Chrome or Edge.",
    downloadOgTitle: "Download Tributary for Windows",
    changelogTitle: "Changelog",
    changelogDescription:
      "Every release of Tributary, the download manager for Windows, newest first, and the milestones that built the first one.",
    changelogOgTitle: "Tributary changelog",
    privacyTitle: "Privacy",
    privacyDescription:
      "What Tributary's website keeps (a download count and short server logs, no cookies, no analytics) and what the app keeps: everything stays on your computer.",
    privacyOgTitle: "Tributary privacy",
    notFoundTitle: "Page not found",
  },
  site: {
    skip: "Skip to content",
    main: "Main",
    howItWorks: "How it works",
    features: "Features",
    footage: "See it work",
    changelog: "Changelog",
    download: "Download",
    downloadForWindows: "Download for Windows",
    privacy: "Privacy",
    openMenu: "Open the menu",
    closeMenu: "Close the menu",
    language: "Language",
    toDark: "Switch to the dark theme",
    toLight: "Switch to the light theme",
    copy: "Copy",
    copied: "Copied",
    footerAbout:
      "A free download manager for Windows that splits each file across several connections and resumes after a crash. Built with Rust, Tauri and Svelte.",
    notAffiliated: "Not affiliated with any website it can save from.",
  },
  hero: {
    line1: "Many streams.",
    line2: "One file.",
    lede: "Tributary is a free download manager for Windows that splits each file across several connections and picks up where it left off after a crash, a shutdown or a dropped connection.",
    demo: "Watch the demo",
    version: "Version",
    size: "Download size",
    runsOn: "Runs on",
    windows: "Windows 10 and 11, 64-bit",
  },
  proof: {
    line: "Checked by {rust} Rust tests, {ui} interface and extension tests and {steps} end-to-end steps, with the extension tested in real Chrome and Edge.",
    counted: "Counted on {date}.",
  },
  how: {
    title: "Built for connections that drop",
    lede: "Tributary asks the server for a file in up to eight parts and fetches them at once. A connection that finishes early takes over half of the largest part still going, so one slow connection can't hold up the rest.",
    more: "If a connection drops, or the computer crashes or shuts down, it carries on from where it stopped. If the file changes on the server mid-download, Tributary notices and starts again, rather than join parts of two different files.",
    alt: "Tributary's download window: downloading, 58.6%, 762 MB of 1.3 GB at 3.2 MB/s, 2 minutes 47 seconds left, resume supported, and a Connections bar of eight filled parts with gaps between them.",
    caption: "The real thing: a 1.3 GB video over eight connections, in Tributary's download window.",
  },
  sim: {
    server: "Server",
    computer: "Your computer",
    intro: "<b>Simulation.</b> It runs in your browser and downloads nothing.",
    progress: "{got} of {size} MB, {percent}%",
    complete: "Complete",
    changed: "Changed on the server",
    label: "Simulated download of {name}",
    own: "Downloading its own part",
    tookOver: "Took over from a slower connection",
    reconnecting: "Reconnecting",
    reconnectingOne: "Connection {n} reconnecting",
    play: "Play",
    pause: "Pause",
    playLabel: "Play the simulation",
    pauseLabel: "Pause the simulation",
    drop: "Drop a connection",
    change: "Change the file on the server",
    said: {
      ready: "Paused. The file is split into eight parts, each fetched over its own connection.",
      underWay: "Downloading over eight connections, each fetching its own part of the file.",
      back: "Connection {n} is back, and carried on from where it stopped.",
      tookOver: "Connection {n} finished its part and took over half of what connection {m} had left.",
      complete: "Download complete. Every part arrived, joined as one file.",
      changed:
        "The file changed on the server. Tributary noticed, and starts again rather than join parts of two different files.",
      dropped: "Connection {n} dropped. Tributary reconnects it, and it carries on from where it stopped.",
      restart: "Starting again from the beginning, with the new version of the file.",
      again: "Downloading it again, over eight connections.",
    },
  },
  features: {
    title: "Downloads from wherever you find them",
    lede: "From the browser, from the video you're watching, from a page of photos or a whole website. It all joins one list, and runs when and as fast as you say.",
    takeoverTitle: "Takes over from Chrome and Edge",
    takeover: "Start a download in Chrome or Edge and Tributary takes it, for the file types you choose. Its extension works in both browsers.",
    takeoverKeys:
      "Hold <key>Ctrl</key> as you click to send a download to Tributary, or <key>Alt</key> to leave it with the browser. Both keys can be changed.",
    takeoverMenu: "Right-click a link, a selection, or a page to send all of its links.",
    takeoverClipboard: "It can watch the clipboard for links you copy, and takes links you drag onto its window.",
    takeoverAlt:
      "Tributary's Download file dialog for node-v22.11.0.tar.gz from nodejs.org: 92.4 MB, resume supported, with a name, folder, category and queue to choose, and Cancel, Download later and Start download buttons.",
    takeoverCaption: "A download clicked in Chrome, taken over by Tributary.",
    videoTitle: "Saves video from the page you watch",
    video: "A button appears beside video players. Tributary saves HLS and DASH streams itself and joins picture and sound into one file with ffmpeg, in qualities up to 4K at 60 fps where the site offers them.",
    videoScript:
      "YouTube and sites like it feed their player by script, so there's no file to see. For those, Tributary hands the page to yt-dlp, a free tool it can install for you, checked against its published checksum.",
    videoNote:
      "Saving video may go against a site's terms, and YouTube's forbid it, so it's your decision to make. Protected (DRM or encrypted), live, paid and login-only videos are refused, and yt-dlp is never given your cookies.",
    videoAlt:
      "Tributary's Save video dialog for a YouTube video, listing qualities from 2160p60 at about 1.3 GB down to 144p, with the note that saving video may go against a site's terms and that protected, paid and login-only videos are refused.",
    videoCaption: "Choosing a quality. The dialog says the same about terms.",
    picturesTitle: "Saves every picture on a page",
    pictures:
      "On a page full of photos or posts, choose <b>Save all images and videos on this page</b>. Tributary scrolls the page as you would, takes each picture at the largest size the page offers, and stops at the end or at a login wall.",
    picturesKeep:
      "Then you choose what to keep, and it saves them through a queue, four at a time, into a folder named after the page.",
    picturesNote:
      "The pictures belong to the people who posted them, and some sites' terms, Instagram's and Facebook's among them, forbid collecting them automatically. Tributary never gets around a login wall, a private account or a site's limits.",
    grabberTitle: "Explores a site for you",
    grabber:
      "The site grabber reads a website one page at a time, always following its robots.txt, and gathers the pages and files you ask for. Choose <b>Whole site</b> to keep it for reading offline, with its links pointing at the saved copies.",
    grabberProjects:
      "A project can be stopped and carried on later, scheduled, or repeated to save whatever is new while the app is open.",
    queuesTitle: "Queues, schedules, limits",
    queues: [
      ["Queues", "Several of them, each running as many files at once as you choose."],
      ["Schedules", "Start and stop times, and the computer woken for a scheduled start."],
      ["When a queue finishes", "Shut the computer down, or run a program."],
      ["Speed limit", "A ceiling on how fast Tributary downloads."],
      ["Quotas", "So much per period. Downloads pause when it's used up and carry on by themselves."],
      ["Categories", "The list sorted by kind: compressed, documents, music, programs and video."],
    ],
    loginsTitle: "Keeps your logins off the disk",
    logins:
      "Tributary downloads over HTTP, HTTPS and FTP, signs in to servers that ask, and works through a proxy. The passwords are kept in Windows Credential Manager, never in a file, and cookies from your browser are held in memory only.",
    loginsMore:
      "So after a restart, a download taken from the browser may need its link again. There's no account and no telemetry, and if you like, each finished download is scanned for viruses.",
  },
  footage: {
    title: "See it work on a real download",
    lede: "Sixty-four seconds, recorded in the app: a download taken over from Chrome, the connections filling, a video quality chosen, and the finished list.",
    fallback:
      "Your browser can't play this video. It shows Tributary taking over a download from Chrome and saving a video.",
    caption:
      "The demo is silent. It saves Big Buck Bunny, a short film that is free to copy. Big Buck Bunny (c) Blender Foundation, CC BY 3.0.",
  },
  commandLine: {
    title: "Drive it from a script",
    lede: "Add downloads, start the queue or open the app in the tray from a terminal, a shortcut or a scheduled task. If Tributary is already running, the command goes to the running copy.",
    label: "Command line",
    copy: "Copy the command line syntax",
    options: {
      add: "Adds a download. Without --silent, you're asked first.",
      folder: "Where to save it.",
      name: "What to call it.",
      silent: "Starts with no questions. A video stream saves its best quality.",
      later: "Puts it in the main queue instead of starting it.",
      startQueue: "Starts the main queue.",
      minimized: "Opens in the tray.",
      quitWhenDone:
        "Closes the app when this download finishes. Needs --add and --silent. The app stays open if the download fails, another one is running, or virus scanning is on.",
    },
  },
  faq: {
    title: "Questions people ask",
    items: [
      { id: "free", q: "Is Tributary free?", a: "Yes. There's no price, no account and no advertising." },
      {
        id: "safe",
        q: "Is the installer safe to run?",
        a: "The installer isn't signed, so Windows SmartScreen warns about it. Check that your download's SHA-256 matches the one on this page, then choose <b>More info</b> and <b>Run anyway</b>. Tributary has no account and no telemetry, keeps server passwords in Windows Credential Manager and holds browser cookies in memory only. It can also scan each finished download for viruses, if you turn that on.",
      },
      {
        id: "youtube",
        q: "Does it download YouTube videos?",
        a: "Yes, through yt-dlp, a free tool Tributary can install for you and check against its published checksum. Saving video may go against a site's terms, and YouTube's forbid it, so that's your decision to make. Protected (DRM or encrypted), live, paid and login-only videos are refused, and yt-dlp is never given your cookies.",
      },
      {
        id: "privacy",
        q: "What does it know about me?",
        a: "There's no account and no telemetry. Passwords for servers are kept in Windows Credential Manager, never on disk, and cookies from your browser are held in memory only.",
      },
      {
        id: "browsers",
        q: "Which browsers does it work with?",
        a: "Chrome and Edge, through an extension you add yourself, because browsers don't let other programs install extensions. The steps are in the download section, and in full on the download page.",
      },
      {
        id: "sites",
        q: "Can it save a whole website?",
        a: "Yes. The site grabber reads a site one page at a time and always follows its <code>robots.txt</code>. Choose <b>Whole site</b> to keep the pages for offline reading.",
      },
      {
        id: "update",
        q: "I updated Tributary. Why is a new feature missing?",
        a: "Reload the extension. Open <code>chrome://extensions</code> (or <code>edge://extensions</code>) and press the circular arrow on Tributary's card. The browser often keeps running in the background and still holds the old extension.",
      },
    ],
  },
  panel: {
    title: "Download Tributary {version}",
    lede: "Free, for Windows 10 and 11. One installer, no account.",
    version: "Version",
    size: "Size",
    published: "Published",
    file: "File",
    checksum: "SHA-256 checksum",
    copyChecksum: "Copy the SHA-256 checksum",
    noChecksum:
      "The release details didn't load, so the checksum can't be shown. Reload the page in a minute to see it before you run the installer.",
    need: "What you need",
    install: "Install it",
    steps: [
      ["Download the installer", "Save it anywhere. Check its SHA-256 against the one above if you like."],
      [
        "Open it and get past SmartScreen",
        "The installer isn't signed, so Windows warns about it. Choose More info, then Run anyway.",
      ],
      [
        "Let it install",
        "It installs for your user only and needs no administrator rights. If Microsoft Edge WebView2 is missing, it fetches it.",
      ],
      [
        "Add the browser extension",
        "Chrome and Edge don't let other programs install extensions, so you add Tributary's yourself. The download page has every step.",
      ],
    ],
    guide: "Read the full install guide",
  },
  facts: {
    requirements: [
      "Windows 10 or 11, 64-bit",
      "Installs for your user only, so it needs no administrator rights",
      "Microsoft Edge WebView2, which is part of Windows 11; the installer fetches it if it's missing",
    ],
    history: [
      {
        title: "The download engine",
        detail:
          "Downloads split across several connections, with resume, and the test server that drops connections and changes files mid-download to prove it. The desktop app and its installer.",
      },
      {
        title: "Everything around the downloads",
        detail:
          "Queues, a scheduler, speed limits and categories. FTP, server logins and proxies. Taking over downloads from the browser. Saving HLS and DASH video. The site grabber. Passwords kept off the disk. Tested on real Chrome.",
      },
      {
        title: "Video from the page you're watching",
        detail:
          "A download button beside any video, YouTube included. The right video when it was opened from a playlist, and qualities named the way video sites name them.",
      },
      {
        title: "More sites, and one file per video",
        detail:
          "Players embedded from other sites, and sites that offer only HLS. Picture and sound arrive as one item and one file.",
      },
      {
        title: "Every picture and video on a page",
        detail:
          "Tributary scrolls a page as you would, gathers what it shows at the largest size offered, and saves it all with one click.",
      },
    ],
  },
  download: {
    title: "Download Tributary",
    lede: "Free, for Windows 10 and 11. It installs for your user only, so it doesn't need administrator rights.",
    onThisPage: "On this page",
    sections: {
      requirements: "What you need",
      check: "Check the download",
      install: "Install",
      smartscreen: "If Windows warns you",
      browser: "Connect Chrome or Edge",
      update: "Update",
      uninstall: "Uninstall",
    },
    checkIntro: "A checksum proves the file you have is the one published here, unchanged on the way.",
    checkRun:
      "Open PowerShell and run these two lines. If you saved the installer somewhere other than your Downloads folder, change the folder in the first line.",
    copyCommands: "Copy the PowerShell commands",
    checkHash:
      "The <b>Hash</b> it prints should match the SHA-256 checksum on this page. PowerShell prints it in capital letters, which makes no difference.",
    checkCompare: "Or let PowerShell compare them for you. It prints <b>True</b> when they match.",
    copyComparison: "Copy the PowerShell comparison",
    checkMismatch: "If they don't match, don't run the file. Delete it and download it again.",
    installSteps: {
      downloadTitle: "Download the installer",
      download:
        "Choose <b>Download for Windows</b>. Your browser saves <code>{file}</code>, usually in your Downloads folder.",
      checkLink: "Check that it arrived intact",
      runTitle: "Run it",
      run: "Open the file. Windows SmartScreen will probably warn you, because the installer isn't signed: choose <b>More info</b>, then <b>Run anyway</b>.",
      whyLink: "Why Windows warns you",
      followTitle: "Follow the installer",
      follow:
        "Tributary installs for your user account only, so it doesn't ask for administrator rights. It needs Microsoft Edge WebView2, which is part of Windows 11; if it's missing, the installer fetches it.",
      openTitle: "Open Tributary",
      open: "Start it from the Start menu.",
      connectLink: "Let it take over downloads from your browser",
    },
    smartscreenIntro: "Why SmartScreen stops the installer, and how to go on.",
    smartscreen:
      "Windows SmartScreen warns about programs it doesn't know yet. Tributary's installer isn't signed with a code-signing certificate, so Windows can't confirm who published it and shows <b>Windows protected your PC</b>.",
    smartscreenCheck: "If it matches, choose <b>More info</b>, then <b>Run anyway</b>.",
    smartscreenCheckLink: "First, check the file's SHA-256 checksum.",
    smartscreenMismatch: "If it doesn't match, don't run it. Delete the file and download it again.",
    browserIntro:
      "Once for each browser, so Tributary can take over downloads and save video from the pages you watch.",
    browserWhy: "Chrome and Edge don't let other programs install extensions, so you add Tributary's yourself.",
    browserSteps: {
      copyTitle: "Copy the extension folder",
      copy: "In Tributary, open <b>Options</b>, then the <b>Browser</b> tab, and copy the folder shown there.",
      developerTitle: "Turn on Developer mode",
      developer:
        "Open <code>chrome://extensions</code> in Chrome, or <code>edge://extensions</code> in Edge, and turn on <b>Developer mode</b>.",
      loadTitle: "Load the extension",
      load: "Choose <b>Load unpacked</b> and pick the folder you copied.",
    },
    browserOthers: "Turn off any other download manager's extension. Both would try to take the same downloads.",
    browserKeys:
      "Hold <key>Ctrl</key> while you click a link to send it to Tributary, or <key>Alt</key> to leave it with the browser. You can change both keys in <b>Options</b>.",
    updateIntro:
      "From version 0.2.0, Tributary updates itself: when a newer version is ready, a bar above the list offers to restart and install it. To move up from 0.1.0, follow these steps once. Every version is listed in the changelog.",
    changelogLink: "See the changelog",
    updateSteps: {
      installTitle: "Install the new version",
      install: "Download it from this page and run the installer over the version you have.",
      reloadTitle: "Reload the extension",
      reload:
        "Open <code>chrome://extensions</code> (or <code>edge://extensions</code>) and press the circular arrow on Tributary's card. Chrome often keeps running in the background after its windows close, still holding the old extension, so new features won't appear until you reload it.",
    },
    uninstall:
      "Open Windows <b>Settings</b>, then <b>Apps</b>. Find Tributary in your installed apps and choose <b>Uninstall</b>. To remove your download list, queues and settings too, tick <b>Delete the application data</b> in the uninstaller.",
    uninstallNotes: [
      "Files you downloaded stay where you saved them.",
      "The uninstaller disconnects the browser extension. Remove it from the browser too, at <code>chrome://extensions</code> or <code>edge://extensions</code>.",
      "Passwords you saved for sites stay in Windows Credential Manager, under names that start with <code>Tributary/</code>, until you remove them there.",
    ],
  },
  release: {
    forWindows: "For Windows 10 and 11, 64-bit",
    howToCheck: "How to check your download",
  },
  changelog: {
    title: "Changelog",
    lede: "What each release of Tributary brought, newest first, and how the first one was built.",
    version: "Version {version}",
    latest: "Latest release",
    noNotes:
      "The notes for this release didn't load. Reload the page in a minute to see them; the history below shows what went into it.",
    downloadSized: "Download {version} ({size})",
    downloadPlain: "Download {version}",
    checkChecksum: "Check its checksum",
    builtTitle: "How {version} was built",
    builtIntro: "Milestones from the project's commit history, oldest first.",
    released: "Released as version {version}",
    releasedOn: "Released as version {version} on {date}",
    getIt: "Get it, with its checksum and install steps",
    /** Release notes by version, for the languages that translate them; English comes from the API. */
    notes: {} as Record<string, string[]>,
  },
  privacy: {
    title: "Privacy",
    lede: "This website counts downloads and keeps short server logs. The app keeps everything on your computer, with no account and no telemetry.",
    updated: "Last updated {date}",
    websiteTitle: "What this website keeps",
    websiteIntro: "As little as it can, and only for as long as it needs.",
    howLong: "How long",
    kept: [
      {
        what: "Download counts",
        detail:
          "Each download of the installer is recorded with its version, the date and time, and whether your browser said it runs on Windows. Nothing else is recorded with it.",
        howLong: "For as long as the site runs, as the number of downloads of each version",
      },
      {
        what: "A code that stops double counting",
        detail:
          "So that one person downloading twice in a day counts once, the site makes a one-way code (a salted hash) from your IP address, your browser's user agent and the date. Your IP address itself is not stored.",
        howLong: "One day, then it is deleted",
      },
      {
        what: "Server logs",
        detail:
          "For each request, nginx, the web server in front of the site, logs the time, your IP address, the address you asked for, the response code and size, and how long it took. It doesn't log which browser you use or which page you came from.",
        howLong:
          "Up to three files of 10 MB; when they are full, the oldest is deleted, so how long depends on how busy the site is",
      },
      {
        what: "Your theme and language",
        detail:
          "If you switch between the light and dark theme, your browser remembers the choice on your device. It is never sent to the site. The language is in the page's address.",
        howLong: "Until you clear this site's data in your browser",
      },
    ],
    notDoneTitle: "What it doesn't do",
    notDone: [
      "It sets no cookies.",
      "It runs no analytics, tracking or advertising scripts.",
      "It loads nothing from other companies: the fonts, pictures and video all come from this site.",
    ],
    appTitle: "What the app keeps",
    appIntro:
      "Tributary keeps what it knows on your computer. There is no account, and no telemetry: it doesn't report what you download or how you use it.",
    app: [
      "Your download list, queues and settings are in <code>%APPDATA%\\app.tributary.desktop</code>.",
      "Passwords for sites and servers are kept in Windows Credential Manager, never in a file.",
      "Cookies that your browser passes on for a download are held in memory only and never written to disk, so after a restart such a download may need its link again.",
      "The browser extension talks only to Tributary on your computer.",
      "Tributary connects to the sites you download from, and to the place yt-dlp is published when you ask it to install or update yt-dlp. yt-dlp is never given your cookies or logins.",
      "Once a day, Tributary asks this website whether there's a newer version. It sends only the version it is and that it runs on 64-bit Windows, and you can turn this off under Options, General.",
      "When you press Find in its start dialog, Tributary asks the site you're downloading from for the checksum it publishes beside the file, without your cookies.",
      "Mirrors and Metalink servers you or a site name get the parts of the file they serve. Your cookies and logins only go to the file's own site.",
    ],
  },
  notFound: {
    title: "There's no page at this address.",
    lede: "The link may be out of date, or the address mistyped. Everything on this site is a click away.",
    home: "Go to the home page",
    download: "Download Tributary",
    more: "More pages",
  },
};

/** Any language's words: the same texts as English, each a string. */
type Widen<T> = T extends string
  ? string
  : T extends readonly (infer Item)[]
    ? Widen<Item>[]
    : T extends object
      ? { [K in keyof T]: Widen<T[K]> }
      : T;

export type Words = Widen<typeof en>;
