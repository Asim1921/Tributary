#!/usr/bin/env node
// Builds the web versions of the demo video and its stills into public/media.
//   node scripts/make-media.mjs            (DEMO_VIDEO=<path> to use another source)
// Needs ffmpeg on the PATH. The source is the author's own public demo (1920x1080, 64 s).
// Every still was checked by eye for personal data (user names, addresses, emails) before use.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const out = join(root, "public", "media");
const src = resolve(process.env.DEMO_VIDEO ?? join(root, "..", "..", "..", "linkedin", "Tributary-demo.mp4"));

if (!existsSync(src)) {
  console.error(`Demo video not found: ${src}\nSet DEMO_VIDEO to its path.`);
  process.exit(1);
}
mkdirSync(out, { recursive: true });

function ffmpeg(args) {
  const r = spawnSync("ffmpeg", ["-v", "error", "-y", ...args], { stdio: "inherit" });
  if (r.status !== 0) throw new Error(`ffmpeg failed: ffmpeg ${args.join(" ")}`);
}

// 1. The video: H.264, 1280 px wide, no audio (the source track is silent), moov atom first.
ffmpeg([
  "-i", src,
  "-an",
  "-vf", "scale=1280:-2:flags=lanczos,format=yuv420p",
  "-c:v", "libx264", "-preset", "slow", "-crf", "27", "-tune", "stillimage",
  "-maxrate", "900k", "-bufsize", "1800k",
  "-g", "60", "-profile:v", "high", "-level", "4.0",
  "-movflags", "+faststart",
  join(out, "tributary-demo.mp4"),
]);

// 2. The poster: the moment the download runs over several connections (16 s).
ffmpeg(["-ss", "16", "-i", src, "-frames:v", "1", "-vf", "scale=1280:-2:flags=lanczos",
  "-c:v", "libwebp", "-quality", "82", join(out, "tributary-demo-poster.webp")]);

// 3. Stills, cropped above the burned-in caption (which starts at y = 950 in the source).
//    name, time (s), crop w:h:x:y in source pixels
const stills = [
  // Tributary takes a download over from Chrome: the "Download file" dialog.
  ["still-takeover", 8, "815:624:553:228"],
  // The real connection bar: a 1.3 GB video over eight connections, resume supported.
  ["still-connections", 44, "775:316:573:596"],
  // Choosing a video quality, with the app's own terms note.
  ["still-quality", 34.6, "815:806:553:134"],
  // The finished list: the 4K video and the Node.js archive, both complete.
  ["still-finished", 50.5, "1180:250:0:0"],
];
for (const [name, t, crop] of stills) {
  ffmpeg(["-ss", String(t), "-i", src, "-frames:v", "1", "-vf", `crop=${crop}`,
    "-c:v", "libwebp", "-lossless", "0", "-quality", "90", "-compression_level", "6",
    join(out, `${name}.webp`)]);
}

for (const f of ["tributary-demo.mp4", "tributary-demo-poster.webp", ...stills.map(([n]) => `${n}.webp`)]) {
  console.log(`${f.padEnd(30)} ${(statSync(join(out, f)).size / 1024).toFixed(0).padStart(6)} KB`);
}
