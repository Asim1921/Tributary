/**
 * First-load JavaScript per route, gzipped, from a production build.
 *
 *   npm run build; node scripts/bundle-size.mjs          # reads .next
 *   $env:NEXT_DIST_DIR='.next-c'; node scripts/bundle-size.mjs
 *
 * The budget (SPEC section 8) is 250 KB gzipped per route, not counting chunks that are loaded
 * lazily (the 3D scene). Exits with 1 when a route is over budget.
 */
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { gzipSync } from "node:zlib";

const dist = path.resolve(process.env.NEXT_DIST_DIR || ".next");
const budget = Number(process.env.BUDGET_KB || 250) * 1024;

async function gz(file) {
  return gzipSync(await readFile(path.join(dist, file))).length;
}

async function walk(dir) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...(await walk(full)));
    else out.push(full);
  }
  return out;
}

const manifestPath = path.join(dist, "build-manifest.json");
const build = JSON.parse(await readFile(manifestPath, "utf8"));
const shared = new Set([...(build.rootMainFiles || []), ...(build.polyfillFiles || [])]);

// App Router: each page's client chunks are listed in its client-reference manifest.
const serverApp = path.join(dist, "server", "app");
const manifests = (await walk(serverApp)).filter((f) => f.endsWith("_client-reference-manifest.js"));
const rows = [];
let failed = false;

for (const manifest of manifests) {
  const text = await readFile(manifest, "utf8");
  const route =
    "/" +
    path
      .relative(serverApp, manifest)
      .replaceAll("\\", "/")
      .replace(/_client-reference-manifest\.js$/, "")
      .replace(/\/page$|^page$/, "")
      .replace(/\/$/, "");
  if (/\/(_not-found|_global-error)|opengraph-image|sitemap|robots/.test(route)) continue;
  const chunks = new Set(shared);
  for (const m of text.matchAll(/"static\/chunks\/[^"]+\.js"/g)) chunks.add(m[0].slice(1, -1));
  for (const m of text.matchAll(/static\/chunks\/[^"\,\]]+\.js/g)) chunks.add(m[0]);
  const eager = [...chunks].filter((c) => !/three|drei|confluence-canvas/i.test(c));
  let total = 0;
  for (const chunk of eager) {
    try {
      total += await gz(chunk);
    } catch {
      /* a listed chunk that is not on disk is not part of the build */
    }
  }
  rows.push({ route: route === "/" ? "/" : route.replace(/\/$/, ""), kb: total / 1024 });
  if (total > budget) failed = true;
}

rows.sort((a, b) => a.route.localeCompare(b.route));
console.log(`First-load JS, gzipped (budget ${budget / 1024} KB):`);
for (const row of rows) console.log(`  ${row.route.padEnd(16)} ${row.kb.toFixed(1).padStart(7)} KB${row.kb * 1024 > budget ? "  OVER" : ""}`);
if (!rows.length) {
  console.error("No routes found. Run a production build first.");
  process.exit(1);
}
process.exit(failed ? 1 : 0);
