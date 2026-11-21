# Tributary

A download manager for Windows, written in Rust and Tauri. It splits files across several connections, resumes after a crash, takes over downloads from the browser, saves video streams, and can save whole websites.

## Installing

Run `Tributary_0.1.0_x64-setup.exe`. It installs for your user only, so it doesn't need administrator rights. The installer isn't signed, so Windows SmartScreen will warn you. Choose **More info**, then **Run anyway**.

## Connecting your browser

Chrome and Edge don't allow outside installs, so this is a one-time step per browser:

1. Open **Options**, then the **Browser** tab, in Tributary, and copy the extension folder shown there.
2. Open `chrome://extensions` or `edge://extensions` and turn on **Developer mode**.
3. Choose **Load unpacked** and pick that folder.

Turn off any other download manager's extension, because both would try to take the same downloads.

**Other browsers.** Brave, Vivaldi, Opera and Chromium load the same extension folder the same way, and Tributary registers itself with each of them.

**Firefox** only runs add-ons Mozilla has signed. `node scripts/build-extension.mjs firefox` makes the Firefox version in `dist/firefox/` and a zip beside it, for its publisher to have signed as an unlisted add-on on addons.mozilla.org. Once installed, it talks to Tributary like the others: the app registers a host for it with its own ID.

**Store versions.** `node scripts/build-extension.mjs store` makes a version for the Chrome Web Store and Edge Add-ons in `dist/store/`. Their rules don't allow downloading YouTube's videos, so it shows no button on YouTube; everything else is the same. Submitting it needs a developer account with each store.

**After updating Tributary, reload the extension.** Open `chrome://extensions` and press the circular arrow on Tributary's card. Chrome often keeps running in the background after its windows are closed, still holding the old extension in memory, so new features such as the YouTube button won't appear until it's reloaded.

Hold **Ctrl** while clicking to force a download into Tributary, or **Alt** to leave it with the browser. Both keys can be changed in Options.

## Command line

```
tributary --add URL [--folder FOLDER] [--name NAME] [--checksum HASH] [--silent] [--later] [--quit-when-done]
tributary --start-queue
tributary --minimized
```

| Option | What it does |
|--------|--------------|
| `--add URL` | Adds a download. Without `--silent`, you're asked first. |
| `--folder FOLDER` | Where to save it. |
| `--name NAME` | What to call it. |
| `--checksum HASH` | The file's MD5, SHA-1, SHA-256 or SHA-512, checked once it's in. `sha256:` in front is optional. |
| `--silent` | Starts with no questions. A video stream saves its best quality. |
| `--later` | Puts it in the main queue instead of starting it. |
| `--start-queue` | Starts the main queue. |
| `--minimized` | Opens in the tray. |
| `--quit-when-done` | Closes the app when this download finishes. Needs `--add` and `--silent`. The app stays open if the download fails, another one is running, or virus scanning is on. |

If Tributary is already running, these go to the running copy.

## Checking downloads

- **Checksums.** Paste the MD5, SHA-1, SHA-256 or SHA-512 a site gives for a file into the **Checksum** box of the start dialog, or press **Find** to have Tributary look for one published beside the file, as `file.iso.sha256` or in a `SHA256SUMS` list. The file is checked once it's in: the list says **Complete, verified**, or **Checksum doesn't match** in red. A checksum can be added or corrected later in **Properties**, and is checked at once. Nothing is sent to the site but plain requests for those lists.
- **Marked as from the internet.** Like a browser, Tributary tags each finished file with where it came from, so Windows SmartScreen and Office's Protected View check it before it opens. **Options > Security** can turn this off. Drives that can't hold the tag, such as FAT32 and exFAT, are left as they are.
- **Notifications.** When downloads finish while Tributary is in the background, Windows shows a notification with **Open** and **Show in folder**. Many files at once, or a whole queue, give one notification rather than one each. A file that doesn't match its checksum, or a download that fails, is said too. **Options > Downloads** can turn this off. Windows only shows them for the installed app, because they go by its Start menu entry.

## When a link expires

Many sites give links that only work for a while. If one stops working before a download finishes, the list says **Link expired**. Right-click it and choose **Get a fresh link**: the page the file came from opens in your browser, and the next time the browser hands Tributary a file of the same name, the download carries on from where it stopped, with the browser's new link and cookies, instead of starting again. If the server says the file itself has changed, it starts again so the result is never a mix. A download added by its address alone doesn't know its page, so Tributary asks you to find it.

## Mirrors and Metalinks

- **Mirrors.** If a file is on more than one server, put the other addresses in **Properties > Mirrors**, one a line. Parts of the file then come from all of them at once, with each new connection going to whichever server is least busy. A mirror that fails, or turns out to hold a file of another size, is left out and the others carry on. Cookies and logins only ever go to the file's own site. Mirrors don't check each other's contents, so add a checksum when you can.
- **Servers that name their mirrors** with a `Link: <…>; rel=duplicate` header (RFC 6249) have them used without asking.
- **Metalinks.** Give Tributary the address of a `.meta4` or `.metalink` file and it downloads the file the Metalink describes, from every server listed, under its own name, and checks it against the Metalink's checksum when it's done.

## Video

The extension shows a panel in the corner of any video player on a page, and lists what it finds in its toolbar popup. Any HLS (`.m3u8`) or DASH (`.mpd`) address also works in **Add URL**, and so does the address of a page with a video on it.

- **YouTube and sites like it** feed their player by script, so there is no file to see. For those, the panel's **Download this video** button gives the page to [yt-dlp](https://github.com/yt-dlp/yt-dlp), a free open-source tool, which finds the video's qualities. Tributary then downloads the one you pick, usually the highest, itself. The first time, the video dialog offers to install yt-dlp for you. It's downloaded from yt-dlp's own releases and checked against their published checksum, and **Options > Video** can update it.
- **Terms of use:** saving video from a site may go against that site's terms, and YouTube's forbid it. That's your decision to make. Tributary says so in the dialog.
- **Players embedded in another site**, as video hosts offer them, are asked about as the page they sit in, and when a player can't be read on its own, the page holding it is read instead. A stream spotted in a player is fetched the way the player asked for it.
- **Sites that offer their video only as HLS**, such as Dailymotion, work too: Tributary reads the stream itself. Where a site turns away anything but a browser at its full list of qualities, each quality is read by its own list instead. Tributary never pretends to be a browser to get past a site's checks.
- **Upright videos**, as phones film them, get the button as well.
- **If nothing is found yet**, press play for a few seconds and use the button again: many players only fetch their video once they start.
- **A site being free to watch doesn't make its videos free to copy.** Sites that stream films without their owners' permission aren't supported. For free films, public-domain collections such as the Internet Archive's work well.
- **Playlists and channels.** Give Tributary a playlist's address, or a channel's Videos tab, and the video dialog offers **Choose videos from this list**. The list is read without reading every video, up to 500 of them, and private, deleted and members-only videos are shown but can't be chosen. The chosen ones go into a folder named after the list, each read from its own page one at a time, a few seconds apart, at its best quality. Any that can't be saved are named together at the end. A channel's own page lists its tabs rather than its videos, so Tributary asks for its Videos tab.
- **The button beside a player** also works in YouTube's miniplayer and on the previews that play on its home page.
- **Only part of a video.** With ffmpeg set up, **Save only part of it** keeps just the time between **From** and **To**, such as `1:30` to `2:45`. The quick cut starts at the nearest keyframe before **From**, which can be a few seconds early. **Cut exactly** starts on the second, because the picture is encoded again, which takes longer. Subtitles are cut to match. The whole video is downloaded first, then cut.
- **Protected, live, paid and login-only videos are refused.** Anything that declares encryption or DRM won't be saved, and neither will members-only or private videos. yt-dlp is never given your cookies or logins.
- **Sound alone** is offered as the last quality, as M4A where the site has it. With ffmpeg set up, it can be made into an MP3 instead.
- **Subtitles and the cover picture.** Where a site lists subtitles, the dialog offers them, written ones in your Windows language first. Automatic captions are offered too, but never chosen for you. **Add the cover picture** puts the video's thumbnail in the file, as music players show it. Both go inside the video with ffmpeg: MP4 and MKV take subtitles and a cover, WebM takes subtitles only, and MP3 and M4A take a cover. Whatever a file can't hold, or everything if ffmpeg isn't set up, is saved beside it, as `Film.en.vtt` and `Film.jpg`. They're fetched when the video is added, without cookies, because their links can expire before a video saved for later starts.
- **Some videos keep picture and sound apart**, as YouTube and Facebook do. The sound is downloaded out of sight, in Tributary's own folder, and joined into the video with [ffmpeg](https://ffmpeg.org): the list shows one item, which reads **Adding sound…** while they're joined, and you get one file. If ffmpeg isn't installed, or can't join them, the sound is put beside the video instead, so nothing is lost. Set ffmpeg's location under **Options**, **Video**, if it isn't found automatically.

## Saving every picture and video on a page

On a page full of photos or posts, such as a public profile, open the extension's toolbar popup and choose **Save all images and videos on this page**. The page's right-click menu has the same thing, and so does **Alt+Shift+S**. If that key does nothing, Chrome hasn't given it to Tributary; choose a key at `chrome://extensions/shortcuts`.

- **Tributary scrolls the page as you would**, a screen at a time. It gathers every picture at the largest size the page offers, every video that has a file of its own, and the posts that hold a video. A small box in the corner counts what it has found. **Stop here** keeps what's been found so far, and **Cancel** gives up. Afterwards the page is put back where it was.
- **It stops by itself** when nothing new has turned up for a few screens (at the end of the page too, as a slow page may still be loading more), or after 300 pictures and videos (or 60 screens).
- **Keep the tab in view while it looks.** A page loads nothing more while it's out of sight, so if you switch tabs, Tributary waits until the tab is back.
- **Small pictures are left out**: icons, avatars and anything under 200 pixels on a side. A photo the page shows at two sizes is listed once, at the bigger.
- **Pages that were already open** when the extension was loaded or updated need reloading first. The popup says so if it can't reach a page.
- **Then you choose.** The list shows each picture and video, with both kinds ticked. **Download all** saves them in a folder named after the page. Pictures go through the **Page media** queue, four at a time. Videos are read from their own pages one at a time, a few seconds apart (even across two pages saved at once), and any that can't be saved are named together in one message at the end. **Stop all** also stops the videos not yet read, and if yt-dlp isn't installed, Tributary says so once rather than for every video.
- **Only what your browser is shown.** Signed out, sites such as Instagram and Facebook show a handful of posts and then ask you to log in, and Tributary stops there. It never gets around a login wall, a private account or a site's limits. Videos' pages are read without your cookies, so a video that needs a login is skipped.
- **Posts aren't opened one by one.** Pictures come at the size the page itself offers, which on some sites is a preview's size.
- **Terms and ownership:** Instagram's and Facebook's terms forbid collecting from them automatically, and the pictures belong to the people who posted them. That's your decision to make, and the dialog says so too.

## Site grabber

**Grabber** on the toolbar explores a website for pages and files. Choose what to get, and it reads the site one page at a time, always following its `robots.txt`.

- **Whole site** saves the pages too, with links pointing at the saved copies, so they open with no connection.
- A project can be stopped, reopened and carried on later.
- A repeating project saves whatever is new, while the app is open.
- Links that only exist once a page's scripts run aren't found.
- Logins can't go in the starting address, because projects are saved as plain text. Add the site under **Options**, **Logins**.

## Where things are kept

- Downloads, queues and settings are in `%APPDATA%\app.tributary.desktop`.
- Passwords are in Windows Credential Manager. Cookies are held in memory only, so a captured download may need its link again after a restart.

## Updates

Tributary looks for a newer version a minute after it starts and once a day after that. A new version is fetched in the background and checked against the signature it was released with, then a bar above the list offers **Restart to update**. It never restarts while files are downloading or a video is being finished. **Options > General** shows the version you have, can check at once, and can turn the daily check off.

After an update, the browser extension reloads itself the next time it talks to Tributary, so the new features appear without visiting `chrome://extensions`.

## Making a release

`website/scripts/release.ps1 -Version X.Y.Z` sets the version everywhere, builds the installer, signs the update with the key in `D:\Tools\tributary-keys\updater.key`, and records it in both `releases.json` files. Keep a copy of that folder somewhere safe: without the key, installed copies can't be updated, and everyone would have to reinstall by hand once. Uploading the installer to Vercel Blob and pushing are left to you.

### Signing the installer

Windows warns about an installer with no publisher. Once you have a code-signing certificate, the build signs everything it makes:

- a `.pfx` file: set `TRIBUTARY_SIGN_CERT` to its path and `TRIBUTARY_SIGN_PASSWORD` to its password;
- Azure Trusted Signing: set `TRIBUTARY_AZURE_SIGN` to `endpoint|account|profile` and sign in with the Azure CLI.

Without either, `src-tauri/scripts/sign.ps1` does nothing and the installer is built unsigned.

## Building

You need Rust, Node.js and the Visual Studio C++ Build Tools.

```
npm install
npx tauri build                                 # the app and its installer
cargo build --release -p testserver -p host     # what the end-to-end tests need
```

## Testing

```
cargo test --workspace      # engine, parsers, grabber and app
npm test                    # interface and extension logic
npm run check               # types
npm run e2e                 # drives the real app
npm run e2e:browser         # drives the real extension in Google Chrome
npm run e2e:edge            # the same in Microsoft Edge
```

The end-to-end tests use their own data folders, so they run beside a copy you already have open. Set `E2E_SKIP_TIMED=1` to skip the step that waits for a real scheduled queue.

The video steps of `npm run e2e` need `ffmpeg` and `ffprobe` on the `PATH`.
