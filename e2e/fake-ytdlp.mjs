// Stands in for yt-dlp in the end-to-end tests, so they need no network. It is strict
// about the command line, because the real thing is only ever started the one way.
//
// The page address says what to answer, as \`?case=\`:
//   (none)    a normal video, as separate picture and sound files on the test server
//   drm       a video with a protected format
//   live      a live stream
//   private   a video that needs an account
//   playlist  an address that names a list and no one video, like a playlist's own page
//   broken    a complaint nobody planned for

import { appendFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const args = process.argv.slice(2);
const log = join(dirname(new URL(import.meta.url).pathname.replace(/^\//, '')), 'fake-ytdlp.log');

const required = ['--ignore-config', '--no-playlist', '--dump-single-json'];
const separator = args.indexOf('--');
const properly = required.every((option) => args.includes(option)) && separator === args.length - 2;
const leaky = args.some((argument) => /cookie|--exec|--config/i.test(argument));

if (!properly || leaky) {
  console.error(`ERROR: unexpected command line: ${args.join(' ')}`);
  process.exit(2);
}

const page = new URL(args.at(-1));
appendFileSync(log, `${page.href}\n`);
const base = page.origin;

const fail = (message) => {
  console.error(`ERROR: ${message}`);
  process.exit(1);
};

const picture = {
  format_id: 'pic',
  url: `${base}/static/pic.mp4`,
  protocol: 'https',
  ext: 'mp4',
  vcodec: 'avc1.42001e',
  acodec: 'none',
  height: 240,
  width: 320,
  fps: 15,
  tbr: 300,
  // A cookie here must never reach the server, whatever yt-dlp reports.
  http_headers: { 'User-Agent': 'FakeAgent/1', Referer: page.href, Cookie: 'leak=1', Authorization: 'Bearer x' },
};
const sound = {
  format_id: 'snd',
  url: `${base}/static/snd.m4a`,
  protocol: 'https',
  ext: 'm4a',
  vcodec: 'none',
  acodec: 'mp4a.40.2',
  abr: 64,
  tbr: 64,
  http_headers: { 'User-Agent': 'FakeAgent/1' },
};
// Something a plain download can't fetch, which has to be left out.
const stream = {
  format_id: 'hls',
  url: `${base}/static/real.m3u8`,
  protocol: 'm3u8_native',
  ext: 'mp4',
  vcodec: 'avc1',
  acodec: 'none',
  height: 2160,
  width: 3840,
  tbr: 9000,
};

switch (page.searchParams.get('case')) {
  case 'drm':
    console.log(JSON.stringify({ title: 'Protected', duration: 6, formats: [picture, sound, { ...picture, format_id: 'drm', has_drm: true }] }));
    break;
  case 'live':
    console.log(JSON.stringify({ title: 'Live', is_live: true, live_status: 'is_live', formats: [picture, sound] }));
    break;
  case 'playlist':
    console.log(JSON.stringify({ _type: 'playlist', id: 'PLtest', title: 'A list', entries: [] }));
    break;
  case 'private':
    fail('[youtube] x: Private video. Sign in if you\'ve been granted access to this video');
    break;
  case 'broken':
    fail('[generic] something nobody planned for happened');
    break;
  default:
    console.log(JSON.stringify({ title: 'Fake film: part 1?', duration: 6, availability: 'public', formats: [stream, picture, sound] }));
}
