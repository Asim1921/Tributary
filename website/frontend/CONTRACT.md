# Frontend contract (three builders working at once)

Read `../SPEC.md` first; it is binding. This file says who owns which files, and the shared
pieces everyone builds on, so three people can work in the same app without overwriting each
other.

## Ownership

| Owner | Files (create and edit only these) |
|---|---|
| Coordinator (done) | `app/layout.tsx`, `app/globals.css`, `app/icon.svg`, `components/site/*`, `components/ui/*`, `lib/site.ts`, `lib/release.ts`, `next.config.ts`, `tsconfig.json`, `postcss.config.mjs`, `eslint.config.mjs`, `package.json`, `scripts/make-media.mjs`, `public/media/*` |
| A: hero | `components/hero/*` |
| B: landing | `app/page.tsx`, `components/sections/*`, `components/channel/*` |
| C: pages and platform | `app/download/*`, `app/changelog/*`, `app/privacy/*`, `app/not-found.tsx`, `app/opengraph-image.tsx`, `app/sitemap.ts`, `app/robots.ts`, `components/pages/*`, `lib/jsonld.ts`, `Dockerfile`, `.dockerignore`, `playwright.config.ts`, `e2e/*`, `scripts/screenshots.mjs`, `scripts/bundle-size.mjs` |

Never create or edit another owner's file, not even as a stub. If you need a change in a shared
file, add a line to `REQUESTS.md` (`- [A|B|C] what and why`) and work around it meanwhile.
Style with Tailwind utilities and the tokens below; anything more goes in a CSS module beside
your own component. Do not add dependencies without a line in `REQUESTS.md`.

## Building and previewing at the same time

Never run `next dev` or `next build` with the default output folder: three builds in one `.next`
corrupt each other. Each owner uses its own folder and port:

| Owner | Command (from `website/frontend`, PowerShell) |
|---|---|
| A | `$env:NEXT_DIST_DIR='.next-a'; npx next dev -p 3101` |
| B | `$env:NEXT_DIST_DIR='.next-b'; npx next dev -p 3102` |
| C | `$env:NEXT_DIST_DIR='.next-c'; npx next dev -p 3103` (and `next build` with the same dist dir) |

Clear `$env:NODE_TLS_REJECT_UNAUTHORIZED=$null` first, every time. Without the API running,
`getLatestRelease()` returns `null` and pages must still render well (the fallback is part of the
design, not an error state). Screenshot your own work with Playwright using the installed Chrome
(`chromium.launch({ channel: 'chrome' })`), at 1440x900 and 390x844, in dark and light (set
`localStorage['tributary-theme']` to `dark` or `light` before load, or emulate the color
scheme), and look at every screenshot yourself. Put scratch scripts and screenshots under
`D:/tmp/tributary-site-scratch/<A|B|C>/`, never in the repository or the session scratchpad.

## Shared pieces (already written; import them)

- `@/lib/site`: `site` (name, url, downloadPath = `/api/download/latest`),
  `facts` (requirements, proof line numbers, the history milestones from SPEC section 4).
- `@/lib/release`: `type Release`, `getLatestRelease(): Promise<Release | null>`,
  `getReleases(): Promise<Release[]>`, `formatBytes(n)`, `formatDate(iso)`. Server only.
- `@/components/ui/button`: `ButtonLink` (`href`, `variant: 'primary' | 'secondary' | 'quiet'`,
  `size: 'md' | 'lg'`, `icon?`, `download?`, `external?`).
- `@/components/ui/icons`: `DownloadIcon`, `CopyIcon`, `CheckIcon`, `SunIcon`,
  `MoonIcon`, `PlayIcon` (all `className?`, decorative, 1em sized, `aria-hidden`).
- `@/components/ui/copy-button`: `CopyButton` (`value`, `label`, `copiedLabel?`), client.
- `@/components/ui/container`: `Container` (`as?`, `className?`), max width 1200px with the
  side gutter. `Section` (`id`, `labelledBy`, `className?`) gives the standard vertical rhythm.
- `@/components/site/logo`: `Logo` (`className?`, `title?`), the app icon as inline SVG.

## Tokens (CSS variables; Tailwind names in brackets)

Colors: `--riverbed` [bg-riverbed], `--channel` [bg-channel], `--bank` [border-bank],
`--current` [text/bg/border-stream: NOT `-current`, which is Tailwind's built-in currentColor
and silently wins], `--estuary` [text-estuary], `--foam` [text-foam],
`--silt` [text-silt], `--on-current` [text-on-current], and `--ok`, `--warn`, `--fault` for the
simulation only. Dark is the default; light comes from the OS or `data-theme="light"`.

Type: `font-display` (Bricolage Grotesque), `font-text` (Public Sans), `font-code`
(JetBrains Mono, code blocks only). Sizes: `text-caption`, `text-body`, `text-lead`, `text-h4`,
`text-h3`, `text-h2`, `text-h1`, `text-hero`. Headings use `font-display`, weight 620-720,
tracking `-0.02em` to `-0.035em`, `text-wrap: balance` (set globally).

Radius: `rounded-s` (6px: buttons, inputs), `rounded-m` (12px: panels), `rounded-l` (20px:
media frames). Easing: `ease-flow`. Durations: 160ms quick, 280ms base.

Rhythm: sections use `Section` (padding-block `clamp(4.5rem, 9vw, 8rem)`). Running text max
`62ch`. Secondary text in `text-silt`. Hairlines `border-bank`. No card grids, no shadows on
everything, no gradients as decoration (the hero's water is the one exception).

## Landing page (B composes `app/page.tsx` in this order)

1. `Hero` from `@/components/hero/hero` (A), props `{ release: Release | null }`. Server
   component; it loads its client 3D canvas itself with `next/dynamic` and `ssr: false`.
2. `ProofLine` 3. `HowItWorks` (id `how-it-works`) 4. `Features` (id `features`, with the
   channel and branches) 5. `Footage` (id `footage`) 6. `CommandLine` (id `command-line`)
7. `DownloadPanel` (id `download`, props `{ release }`) 8. `Faq` (id `faq`), all from
`@/components/sections/*` (B). `page.tsx` calls `getLatestRelease()` once and passes it down.

The header links to `/#features`, `/#how-it-works`, `/#footage`, `/changelog` and `/download`.
