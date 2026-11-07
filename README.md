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

Hold **Ctrl** while clicking to force a download into Tributary, or **Alt** to leave it with the browser. Both keys can be changed in Options.

## Command line

```
tributary --add URL [--folder FOLDER] [--name NAME] [--silent] [--later] [--quit-when-done]
tributary --start-queue
tributary --minimized
```

| Option | What it does |
|--------|--------------|
| `--add URL` | Adds a download. Without `--silent`, you're asked first. |
| `--folder FOLDER` | Where to save it. |
| `--name NAME` | What to call it. |
| `--silent` | Starts with no questions. A video stream saves its best quality. |
| `--later` | Puts it in the main queue instead of starting it. |
| `--start-queue` | Starts the main queue. |
| `--minimized` | Opens in the tray. |
| `--quit-when-done` | Closes the app when this download finishes. Needs `--add` and `--silent`. The app stays open if the download fails, another one is running, or virus scanning is on. |

If Tributary is already running, these go to the running copy.

## Video

The extension shows a panel over video players, and lists what it finds in its toolbar popup. Any HLS (`.m3u8`) or DASH (`.mpd`) address also works in **Add URL**.

- **Protected and live videos are refused.** Any stream that declares encryption or DRM won't be saved.
- **Sites that feed their player by script aren't detected**, which includes YouTube. The panel won't appear there.
- **Some videos keep picture and sound apart.** With [ffmpeg](https://ffmpeg.org) installed they're joined into one file. Without it they stay as two. Set its location under **Options**, **Video**, if it isn't found automatically.

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
npm run e2e:browser         # drives the real extension in Microsoft Edge
```

The end-to-end tests use their own data folders, so they run beside a copy you already have open. Set `E2E_SKIP_TIMED=1` to skip the step that waits for a real scheduled queue.

The video steps of `npm run e2e` need `ffmpeg` and `ffprobe` on the `PATH`.
