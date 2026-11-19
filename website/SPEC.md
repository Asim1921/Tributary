# Tributary website: specification

The public website for Tributary, the free, closed-source download manager for Windows in this
repository. It explains what the app does, earns trust, and hands over the installer.

Everything here is binding for whoever builds a part of the site. Where this file and a
plugin's guidance disagree, this file wins.

## 1. Subject, audience, job

- **Subject**: Tributary 0.1.0, a download manager for Windows written in Rust and Tauri, with a
  Chrome and Edge extension. Its icon is a confluence: two curved streams join one channel that
  ends in a download arrow (`../src-tauri/icons/source.svg`). The name and the engineering are
  the same idea: one file arrives as several streams that merge.
- **Audience**: Windows users who download large files; developers and engineers who will judge
  the quality of the work (the site is also the author's portfolio piece).
- **Primary job**: make clear what Tributary does, earn trust honestly, and get the installer
  downloaded. Secondary: show the demo footage.

## 2. Design plan (frontend-design, two passes; this is the revised plan)

### Color (dark-first; light theme designed with equal care)

Named after the river. Hue-biased toward the icon's teal and deep blue; never a neutral grey or a
tinted #0B0B0B/#111 stand-in for black.

| Token | Dark | Light | Role |
|---|---|---|---|
| `--riverbed` | `#05161B` | `#F1F6F5` | page background |
| `--channel` | `#0B242B` | `#FFFFFF` | raised surfaces (sparingly) |
| `--bank` | `#183A42` | `#D3E2E0` | hairlines, borders |
| `--current` | `#2FC4B2` | `#0F766E` | primary action, the live streams (the app's own accent) |
| `--estuary` | `#4C9BD1` | `#134E6F` | secondary hue: links, the second stream, focus ring |
| `--foam` | `#E4F1F0` | `#0D2A2E` | body text |
| `--silt` | `#8FA9AC` | `#4A6366` | secondary text |
| `--on-current` | `#032622` | `#FFFFFF` | text on `--current` |

Semantic (status in the simulation only): ok `#4ADE80`/`#15803D`, warn `#FDB022`/`#A15C07`,
fault `#F97066`/`#B42318`. All text pairs must reach WCAG AA (4.5:1 body, 3:1 large text and UI).

### Type

- **Display**: Bricolage Grotesque (variable; use its optical size and weight axes). Headlines
  only, set tight (-0.02em to -0.035em), weights 600-750. The type itself is part of the design:
  large, confident, sentence case.
- **Text**: Public Sans (formal, very legible). Body 17-18px, line-height 1.6, measure under 72ch.
- **Code**: JetBrains Mono, only inside code blocks (the command line section, checksum command).
  Never for small labels.
- Scale (major third, 1.25): 13.6 / 17 / 21.25 / 26.6 / 33.2 / 41.5 / 51.9 / 64.8 / 81 px, with
  `clamp()` for the two largest steps. `font-variant-numeric: tabular-nums` wherever digits align.

Load all three with `next/font/google` (self-hosted at build time; no runtime request to Google).

### Layout concept: "the confluence"

The page reads like water flowing downhill. A single main channel, a thin vertical line, runs
down the left gutter of the content column on wide screens. Each feature section joins it with a
curved branch drawn from the section into the channel, in the icon's own curve. The channel ends
at the Download section, where it becomes the icon's arrow and base bar around the download
button. On phones the channel and branches are hidden; nothing depends on them.

Content is left-aligned on a 12-column grid (max width 1200px, gutter 24px, side padding at least
16px). Not everything centered. Sections vary in composition (text with a product visual; a wide
interactive band; a two-column definition list); never a row of identical rounded cards.

```
 nav  [icon Tributary]   Features  How it works  Browser  Changelog        [Download]
 ---------------------------------------------------------------------------------------------
 HERO   H1 (display, 2 lines)                      |  live 3D confluence (streams merge into one
        lede, one sentence                         |  channel, then the arrow). Pointer bends the
        [Download for Windows] [Watch the demo]   |  streams a little. Static frame if reduced
        Version 0.1.0 / 5.2 MB / Windows 10 and 11  |  motion or no WebGL.
        (a small definition list, not a dotted line)|
 ---------------------------------------------------------------------------------------------
 one typographic line of proof (tests, real browsers); no big-number tiles
 ---------------------------------------------------------------------------------------------
 HOW IT WORKS  wide band: a file bar split into connection streams. Buttons: "Drop a connection"
               (it resumes), "Change the file on the server" (it notices and starts again).
               Clearly labelled as a simulation.
 ---------------------------------------------------------------------------------------------
 | branch -> Takes over from Chrome and Edge        (text + real still from the demo)
 | branch -> Saves video from the page you watch    (text + terms note, plainly)
 | branch -> Saves every picture on a page          (text + terms/ownership note)
 | branch -> Explores a site for you                (text)
 | branch -> Queues, schedules, limits              (compact two-column definition list)
 | branch -> Keeps your logins off the disk         (text)
 ---------------------------------------------------------------------------------------------
 REAL FOOTAGE  the 64-second demo (poster, controls, muted, no autoplay with sound) + credit
 COMMAND LINE  code block with the real options
 ---------------------------------------------------------------------------------------------
 DOWNLOAD (the channel becomes the arrow)  version, size, date, SHA-256 with Copy, requirements,
           install steps (a real sequence, so numbered), SmartScreen note, extension setup steps
 FAQ (accordion)
 footer   download, changelog, privacy, licence note, "Built with Rust and Tauri"
```

### Principles

1. **Truth over hype.** Every claim maps to the README, a test, or this file's facts section.
   No invented numbers, testimonials, logos, user counts or speed multipliers.
2. **The water is structural, not decoration.** Curves come from the icon; the channel carries
   the reader to the download.
3. **One bold element.** The live confluence in the hero is the memorable thing. Everything else
   is quiet: no fade-and-slide-up on every section, no hover effects on every card. The only other
   motion is the channel filling as you scroll, and motion answering a person's action.
4. **Enterprise floor, unannounced.** AA contrast in both themes, visible keyboard focus, 44px
   touch targets, `prefers-reduced-motion` honoured everywhere, fast first paint, no layout shift,
   works at 360px wide without sideways scrolling, meaningful alt text, landmarks, skip link.

### Defaults we refuse (the frontend-design list)

No accent on a single word of a headline. No ALL-CAPS labels or eyebrows above headings. No
middle-dot meta strings ("A · B · C"). No "WORD — fragment" labels. No monospace small labels.
No "→" appended to links or buttons. No numbered markers except real sequences (install steps).
No SaaS-card kit (identical rounded cards, same grey shadow, gradient washes). No purple-to-blue
gradients. No emoji as section markers.

## 3. Copy

Plain, specific, sentence case, active voice, written from the reader's side (ux-copy skill). The
README (`../README.md`) is the source of truth for behaviour. Start every CTA with a verb.

- H1 candidates (pick one, or better): "Every file arrives in one piece." / "Many streams. One file."
- Primary CTA: "Download for Windows". Secondary: "Watch the demo".
- Notes that must appear, in the reader's words: saving video may go against a site's terms, and
  YouTube's forbid it; protected (DRM or encrypted), live, paid and login-only videos are refused;
  yt-dlp is never given your cookies; pictures belong to the people who posted them, and some
  sites' terms forbid collecting them automatically; the installer is not signed, so Windows
  SmartScreen warns (More info, then Run anyway).

## 4. Facts (use these; do not invent others)

- Version 0.1.0. Windows 10 and 11, 64-bit. Per-user install, no administrator rights.
  Microsoft Edge WebView2 is needed (part of Windows 11; the installer fetches it if missing).
- Installer: `Tributary_0.1.0_x64-setup.exe`. Size and SHA-256 come from the release manifest at
  runtime (see 6); never hard-code them in components.
- Closed source. Never link to a repository, issue tracker or "view the source"; never say
  "open source" or "source available" about Tributary. Safety rests on the SHA-256 check, no
  account, no telemetry, cookies in memory, passwords in Windows Credential Manager and the
  optional virus scan.
- Built with Rust, Tauri and Svelte; extension for Chrome and Edge (Manifest V3).
- Capabilities (from the README; phrase them for readers):
  - Splits a file across several connections; a connection that finishes takes over part of a
    slow one. Resumes after a crash, a shutdown or a dropped connection. If the server's copy
    changes mid-download, it notices and starts again cleanly.
  - HTTP, HTTPS and FTP; server logins; proxies; a speed limiter; quotas per period that pause
    and resume by themselves.
  - Takes over downloads from Chrome and Edge by file type. Hold Ctrl to force a download into
    Tributary, Alt to leave it with the browser (both keys can be changed). Right-click a link, a
    selection, or all links on a page. Clipboard watching. Drag links onto the window.
  - Video: a button beside video players; HLS and DASH streams; pages whose player is fed by
    script (YouTube and similar) through yt-dlp, a free open-source tool it can install for you,
    checked against its published checksum; picture and sound joined into one file with ffmpeg;
    qualities up to 4K at 60 fps where the site offers them.
  - Save every picture and video on a page: it scrolls the page as you would, takes each picture
    at the largest size the page offers, stops at the end or at a login wall, and saves them
    through a queue four at a time into a folder named after the page.
  - Site grabber: explores a website for pages and files, always following its robots.txt; can
    save a whole site for offline reading; projects can be scheduled and repeated.
  - Queues and a scheduler: several queues, start and stop times, files at once, wake the
    computer for a scheduled start, shut down or run a program when done.
  - Passwords are kept in Windows Credential Manager, never on disk; cookies are held in memory
    only; optional virus scan after each download. No account. No telemetry.
  - Command line: `tributary --add URL [--folder FOLDER] [--name NAME] [--silent] [--later]
    [--quit-when-done]`, `tributary --start-queue`, `tributary --minimized`.
- Proof line (accurate on 2026-11-19): checked by 609 Rust tests, 189 interface and extension
  tests and 66 end-to-end steps, with the extension tested in real Chrome and Edge.
- History (for /changelog), from the git log:
  - 2026-11-06: download engine and its test server; desktop app and installer.
  - 2026-11-07: queues, scheduling, limits and categories; FTP, logins, proxies; browser capture;
    HLS and DASH video; the site grabber; passwords kept off the disk; tested on real Chrome.
  - 2026-11-12: a download button beside any video, YouTube included; the right video from a
    playlist; qualities named the way video sites name them.
  - 2026-11-18: video from more sites, embedded players included; picture and sound as one file.
  - 2026-11-19: save every picture and video on a page.

## 5. Media

- Icon: `../src-tauri/icons/source.svg` (use as logo and favicon; generate PNG sizes from it).
- Real footage: `D:\Work\Reverse_Engineering\linkedin\Tributary-demo.mp4` (1920x1080, 64 s; the
  author's own public demo). Re-encode for the web (H.264, 1280 px wide, faststart, about 3 MB)
  plus a poster frame. Stills from it (crop off the burned-in caption at the bottom): about 8 s
  (download taken over from Chrome), 16 s (several connections), 32 s (choosing a video quality),
  48 s (finished list). Check every still for personal data (usernames, IP addresses, emails)
  and do not use any frame that shows some.
- The demo shows Big Buck Bunny: credit "Big Buck Bunny (c) Blender Foundation, CC BY 3.0" next to
  the video.

## 6. Architecture

```
website/
  SPEC.md                 this file
  README.md               how to run, build, test, publish a release
  docker-compose.yml      the whole stack
  .env.example
  downloads/              installer(s) + releases.json (installers git-ignored)
  scripts/                publish-installer.ps1, smoke.mjs
  nginx/                  nginx.conf (+ conf.d/*)
  backend/                FastAPI app, Dockerfile, tests
  frontend/               Next.js app, Dockerfile, e2e (Playwright)
```

### Containers (docker compose, project name `tributary-site`)

| Service | Image / build | Replicas | Notes |
|---|---|---|---|
| `nginx` | `nginx:1.28-alpine` (or newer stable with upstream `resolve`) | 1 | only public port: `8080:80` |
| `web` | `frontend/Dockerfile` (Next.js standalone, Node 22 alpine) | 2 | port 3000, non-root |
| `api` | `backend/Dockerfile` (Python 3.13 slim, uvicorn) | 2 | port 8000, non-root |
| `mongo` | `mongo:8.0` | 1 | named volume, healthcheck, not published |
| `redis` | `redis:7.4-alpine` | 1 | `maxmemory 128mb`, `allkeys-lru`, healthcheck, not published |

Networks: `edge` (nginx, web, api) and `data` (api, mongo, redis; `internal: true`). Healthchecks
on every service; `depends_on` with `condition: service_healthy`. `restart: unless-stopped`.

### nginx

- Load balancing: `upstream web` and `upstream api` with a shared `zone`, `least_conn`, Docker's
  resolver (`resolver 127.0.0.11 valid=10s ipv6=off;`) and `server web:3000 resolve;` so every
  replica is used and replicas can come and go. Passive health: `max_fails=3 fail_timeout=10s`.
  Keepalive to upstreams.
- Caching (`proxy_cache_path` on a container volume): `/_next/static/` and `/media/` cached long
  (immutable); HTML micro-cached 10 s with `proxy_cache_use_stale updating error timeout` and
  `proxy_cache_lock`; `GET /api/releases*` and `/api/stats` micro-cached 5 s; never cache
  `/api/download/*` or anything with a cookie. Send `X-Cache-Status`.
- Rate limits: `/api/` 20 r/s per IP (burst 40, nodelay); `/api/download/` 10 r/min per IP
  (burst 5). Return 429.
- Downloads: the API answers `/api/download/...` with `X-Accel-Redirect: /_files/<name>`;
  `location /_files/ { internal; alias /srv/downloads/; }` serves the file with sendfile and
  `Content-Disposition: attachment`. Direct requests to installer paths get 404.
- gzip for text types; security headers on every response: `X-Content-Type-Options nosniff`,
  `Referrer-Policy strict-origin-when-cross-origin`, `Permissions-Policy` (camera, microphone,
  geolocation off), `X-Frame-Options DENY`, a Content-Security-Policy that works with Next.js
  (`default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline';
  img-src 'self' data: blob:; media-src 'self'; font-src 'self'; connect-src 'self';
  frame-ancestors 'none'; base-uri 'self'; form-action 'self'`). `server_tokens off`.
  Client body limit small (64k). Access log in JSON.

### API (FastAPI, Python 3.13, pydantic v2, PyMongo's async client, redis.asyncio)

All routes under `/api`. JSON. No CORS needed (same origin through nginx).

| Route | Behaviour |
|---|---|
| `GET /api/health` | `{status, mongo, redis, instance}`; 503 if a dependency is down. `instance` is the container hostname (lets the smoke test see load balancing). |
| `GET /api/releases/latest` | latest release: `{version, file, size, sha256, published, notes[], requirements[]}`. Cached in Redis 60 s. 404 if none. |
| `GET /api/releases` | all releases, newest first. Cached 60 s. |
| `GET /api/download/latest`, `GET /api/download/{version}` | `version` must match `^\d+\.\d+\.\d+$`. Counts the download, then 200 with `X-Accel-Redirect: /_files/<file>`, `Content-Disposition: attachment; filename="<file>"`, `Cache-Control: no-store`. The file name comes only from the release record, never from the request. 404 for unknown versions. |
| `GET /api/stats` | `{downloads: {total, by_version}}`. Counters live in Redis (`INCR`), events in Mongo; rebuilt from Mongo when Redis is empty. Cached 30 s. |

- Counting is privacy-preserving: never store an IP address. A download counts once per
  (salted SHA-256 of IP + user agent + UTC day) using a Redis key with a 1-day TTL; the salt
  comes from the environment. Mongo stores `{version, at, ua_family}` only (`windows`, `other`).
- Releases are seeded at startup from `downloads/releases.json` (upsert by version, idempotent,
  safe with two replicas starting at once), and the file's existence and size are checked.
- Settings from the environment (`pydantic-settings`): `MONGO_URL`, `REDIS_URL`,
  `DOWNLOADS_DIR`, `COUNT_SALT`, `CACHE_TTL_*`.
- Mongo indexes: `releases.version` unique; `downloads.at`.
- Structured JSON logging; graceful startup when Mongo or Redis come up late (retry with backoff).
- Tests (pytest + httpx `ASGITransport`): fakes for Mongo and Redis (in-memory implementations
  behind small repository/cache interfaces, or fakeredis), covering every route, caching,
  invalidation, the counting rules, version validation and path safety. Integration tests marked
  `integration` run against real services in compose.

### Release manifest (`downloads/releases.json`)

```json
{ "releases": [ { "version": "0.1.0", "file": "Tributary_0.1.0_x64-setup.exe",
  "size": 5465210, "sha256": "...", "published": "2026-11-19",
  "notes": ["..."], "requirements": ["Windows 10 or 11, 64-bit", "..."] } ] }
```

`scripts/publish-installer.ps1 [-Version 0.1.0]` copies `../target/release/bundle/nsis/
Tributary_<v>_x64-setup.exe` into `downloads/`, computes size and SHA-256, and upserts the entry.

### Frontend (Next.js App Router, React 19, TypeScript strict)

- Pages: `/` (landing), `/download`, `/changelog`, `/privacy`, `not-found`.
- Server components fetch release data from `INTERNAL_API_URL` (`http://api:8000`) with
  `revalidate: 60` and fall back gracefully (the page still renders, with the download link to
  `/api/download/latest`) when the API is unreachable, including at build time.
- 3D: three.js through `@react-three/fiber` and `@react-three/drei`, loaded with `next/dynamic`
  and `ssr: false`. Particles flow along curves computed in the vertex shader (no per-frame CPU
  work per particle), additive blending, DPR capped at 1.75, paused when off-screen or the tab is
  hidden, a static frame with `prefers-reduced-motion`, and a static SVG fallback without WebGL.
  The canvas is `aria-hidden`; the hero text never depends on it.
- Motion: `motion` (Framer Motion) only for the channel fill on scroll and responses to actions.
- Components: accessible primitives from Radix (Accordion for the FAQ, Tooltip, Toast for
  "Copied") styled entirely with this file's tokens. Tailwind CSS v4 with the tokens as CSS
  variables. Theme: dark by default, light via `prefers-color-scheme`, plus a toggle that
  persists (no flash on load).
- SEO: metadata per page, Open Graph image (generated), `sitemap.xml`, `robots.txt`, JSON-LD
  `SoftwareApplication` with the real version, OS and download URL.
- Quality bar: `next build`, `tsc --noEmit` and lint clean; no console errors; Lighthouse-style
  budgets: first-load JS under 250 KB gzipped excluding the lazily loaded 3D chunk; CLS under 0.05.
- Playwright (`@playwright/test`, using installed Google Chrome via `channel: 'chrome'`; set
  `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`, the C: drive is short of space): screenshots at 1440 and
  390 px wide, both themes, for self-critique; and e2e against the stack at `BASE_URL`
  (default http://localhost:8080): pages load without console errors, no horizontal scroll at
  360/390 px, keyboard focus visible, the download button returns the installer with the
  manifest's SHA-256, `Copy` puts the checksum on the clipboard, reduced motion shows the static
  frame.

## 7. Process rules for builders

- Work only inside `website/` (plus reading the rest of the repository). Do not modify the app.
- Use the user's design plugins as written guidance (read the files):
  - frontend-design: `C:\Users\VICTUS 16\.claude\plugins\synced\0116c990-5796-472d-a463-40fba0468c9a_8a7963de-3907-42dc-b9b4-6c1c027187e6\2ca4e30e-2482-4c94-8644-6c26adf27391\skills\frontend-design\SKILL.md`
  - Design plugin skills (design-system, design-critique, accessibility-review, ux-copy) under
    `...\afe29730-8d0c-470c-801a-a52a510c0510\skills\`
- Clear `NODE_TLS_REJECT_UNAUTHORIZED` in every node/npm command (it is set to 0 in this
  environment). The network is flaky: retry installs. Keep everything on D:.
- Never send the author's email or personal details anywhere. No analytics or third-party
  scripts, fonts or embeds at runtime.
- Do not commit; the coordinator commits.
