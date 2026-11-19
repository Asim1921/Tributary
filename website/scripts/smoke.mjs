#!/usr/bin/env node
// Smoke test for the running Tributary website stack. No dependencies (Node 18+).
//   BASE_URL=http://localhost:8080 node scripts/smoke.mjs
// Needs a staged release (see README: scripts/publish-installer.ps1). Takes about a minute:
// it waits for the stats cache to refresh and, if needed, for the download rate limit to refill.
import { createHash, randomBytes } from "node:crypto";

const BASE = (process.env.BASE_URL || "http://localhost:8080").replace(/\/+$/, "");
const UA = `tributary-smoke/${randomBytes(4).toString("hex")}`; // fresh per run, so the counter sees a new visitor
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
let manifest = null;
let baseline;

async function get(path, opts = {}) {
  return fetch(BASE + path, {
    redirect: "manual",
    ...opts,
    headers: { "user-agent": UA, ...(opts.headers || {}) },
    signal: AbortSignal.timeout(opts.timeout ?? 20000),
  });
}

async function check(name, fn) {
  const t0 = Date.now();
  try {
    const detail = await fn();
    results.push({ name, ok: true, detail: detail || "", ms: Date.now() - t0 });
  } catch (e) {
    results.push({ name, ok: false, detail: String(e?.message || e), ms: Date.now() - t0 });
  }
}

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

// GET the installer, waiting out a 429 (the download limit is 10 per minute, burst 5).
async function getInstaller() {
  for (let attempt = 0; attempt < 8; attempt++) {
    const res = await get("/api/download/latest", { timeout: 60000 });
    if (res.status !== 429) return res;
    await res.body?.cancel();
    await sleep(8000);
  }
  throw new Error("still rate limited after waiting (is something else downloading?)");
}

async function totalDownloads() {
  const res = await get(`/api/stats?t=${Date.now()}`);
  assert(res.ok, `/api/stats answered ${res.status}`);
  return (await res.json()).downloads.total;
}

await check("nginx is up (/healthz)", async () => {
  const res = await get("/healthz");
  assert(res.status === 200, `status ${res.status}`);
  return "200";
});

await check("api health reports mongo and redis", async () => {
  const res = await get("/api/health");
  assert(res.status === 200, `status ${res.status}`);
  const j = await res.json();
  assert(j.status === "ok", `status field is ${JSON.stringify(j.status)}`);
  return `mongo=${j.mongo} redis=${j.redis}`;
});

await check("load balancing: more than one api instance answers", async () => {
  const seen = new Map();
  for (let round = 0; round < 4; round++) {
    const batch = await Promise.all(
      Array.from({ length: 5 }, async () => {
        const res = await get("/api/health");
        return (await res.json()).instance;
      }),
    );
    for (const i of batch) seen.set(i, (seen.get(i) || 0) + 1);
  }
  assert(seen.size > 1, `only one instance answered: ${[...seen.keys()].join(", ")}`);
  return [...seen].map(([k, v]) => `${k}x${v}`).join(" ");
});

await check("home page is served by the web upstream", async () => {
  const res = await get("/");
  assert(res.status === 200, `status ${res.status}`);
  assert((res.headers.get("content-type") || "").includes("text/html"), "not HTML");
  assert(res.headers.get("x-upstream") === "web", `X-Upstream is ${res.headers.get("x-upstream")}`);
  const html = await res.text();
  assert(/tributary/i.test(html), "page does not mention Tributary");
  assert(res.headers.get("x-content-type-options") === "nosniff", "security headers missing");
  assert(!/^nginx\/\d/.test(res.headers.get("server") || ""), "server version is exposed");
  return `X-Cache-Status=${res.headers.get("x-cache-status")}`;
});

await check("repeated /_next/static asset is a cache HIT", async () => {
  const home = await (await get("/")).text();
  const m = home.match(/\/_next\/static\/[^"'\s\\)]+\.(?:js|css)/);
  assert(m, "no /_next/static asset found in the home page");
  const first = await get(m[0]);
  assert(first.status === 200, `asset status ${first.status}`);
  await first.arrayBuffer();
  const second = await get(m[0]);
  await second.arrayBuffer();
  const status = second.headers.get("x-cache-status");
  assert(status === "HIT", `second request X-Cache-Status=${status}`);
  assert(/immutable/.test(second.headers.get("cache-control") || ""), "no immutable Cache-Control");
  return `${m[0].slice(0, 48)}... ${status}`;
});

await check("release manifest is available", async () => {
  const res = await get("/api/releases/latest");
  assert(res.status === 200, `status ${res.status} (stage a release first)`);
  manifest = await res.json();
  assert(manifest.sha256 && manifest.file && manifest.size, "manifest lacks sha256, file or size");
  return `${manifest.version} ${manifest.size} bytes`;
});

await check("/api/stats answers (baseline taken)", async () => {
  baseline = await totalDownloads();
  return `total ${baseline}`;
});

await check("installer bytes match the manifest SHA-256", async () => {
  assert(manifest, "no manifest");
  const res = await getInstaller();
  assert(res.status === 200, `status ${res.status}`);
  const cd = res.headers.get("content-disposition") || "";
  assert(/^attachment/i.test(cd), `Content-Disposition is "${cd}"`);
  assert(cd.includes(manifest.file), `Content-Disposition does not name ${manifest.file}`);
  assert(/no-store/.test(res.headers.get("cache-control") || ""), "download response is cacheable");
  assert(!res.headers.get("x-accel-redirect"), "X-Accel-Redirect leaked to the client");
  const buf = Buffer.from(await res.arrayBuffer());
  const sha = createHash("sha256").update(buf).digest("hex");
  assert(buf.length === manifest.size, `got ${buf.length} bytes, manifest says ${manifest.size}`);
  assert(sha === manifest.sha256.toLowerCase(), `sha256 ${sha} differs from the manifest`);
  return `${buf.length} bytes, sha256 ok`;
});

await check("direct request for the installer path is 404", async () => {
  assert(manifest, "no manifest");
  const paths = [`/downloads/${manifest.file}`, `/${manifest.file}`, `/_files/${manifest.file}`, `/srv/downloads/${manifest.file}`];
  const bad = [];
  for (const p of paths) {
    const res = await get(p);
    await res.arrayBuffer();
    if (res.status !== 404) bad.push(`${p} -> ${res.status}`);
  }
  assert(bad.length === 0, bad.join("; "));
  return `${paths.length} paths all 404`;
});

await check("/api/stats counts the download once", async () => {
  // Same visitor (same user agent) downloaded above; one more download must not count twice.
  // The count is cached for up to 30 s by the API plus 5 s by nginx, so poll.
  const second = await getInstaller();
  await second.body?.cancel();
  const deadline = Date.now() + 70000;
  let total = null;
  let stable = 0;
  const startTotal = baseline;
  while (Date.now() < deadline) {
    total = await totalDownloads();
    if (startTotal !== undefined && total >= startTotal + 1) {
      stable++;
      if (stable >= 2) break; // seen twice, a few seconds apart
    }
    await sleep(4000);
  }
  assert(startTotal !== undefined, "no baseline");
  assert(total === startTotal + 1, `total went ${startTotal} -> ${total}, expected +1`);
  return `${startTotal} -> ${total}`;
});

await check("hammering /api/download/latest returns 429", async () => {
  const codes = [];
  for (let i = 0; i < 30; i++) {
    const res = await get("/api/download/latest");
    codes.push(res.status);
    await res.body?.cancel();
    if (res.status === 429) {
      assert((res.headers.get("content-type") || "").includes("json"), "429 body is not JSON");
      break;
    }
  }
  assert(codes.includes(429), `no 429 in ${codes.length} requests: ${codes.join(",")}`);
  return `429 after ${codes.length} requests`;
});

// ---- report
const w = Math.max(...results.map((r) => r.name.length), 4);
console.log(`\nSmoke test against ${BASE}\n`);
for (const r of results) {
  console.log(`${r.ok ? "PASS" : "FAIL"}  ${r.name.padEnd(w)}  ${String(r.ms).padStart(6)} ms  ${r.detail}`);
}
const failed = results.filter((r) => !r.ok).length;
console.log(`\n${results.length - failed} passed, ${failed} failed\n`);
process.exit(failed ? 1 : 0);
