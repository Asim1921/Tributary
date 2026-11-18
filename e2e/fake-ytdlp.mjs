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
//
// An embedded player (`/embedded-player`) answers only with `--referer`, as players embedded
// in another site do, unless its case is `stuck`, when it never answers. Its host page
// (`/embed-host`) answers with the player's film, as yt-dlp's generic reader finds it.
//
// Every command line is logged, one JSON list a line, to FAKE_YTDLP_LOG or beside this file.

import { appendFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const args = process.argv.slice(2);
const log = process.env.FAKE_YTDLP_LOG || join(dirname(new URL(import.meta.url).pathname.replace(/^\//, '')), 'fake-ytdlp.log');

const required = ['--ignore-config', '--no-playlist', '--dump-single-json'];
const separator = args.indexOf('--');
// A referer is only ever the value of its option, before the separator.
const at = args.indexOf('--referer');
const referer = at === -1 ? null : args[at + 1];
const refererFits = at === -1 || (at + 1 < separator && /^https?:\/\//.test(referer ?? ''));
const properly =
  required.every((option) => args.includes(option)) && separator === args.length - 2 && refererFits;
const leaky = args.some((argument) => /cookie|--exec|--config/i.test(argument));

if (!properly || leaky) {
  console.error(`ERROR: unexpected command line: ${args.join(' ')}`);
  process.exit(2);
}

const page = new URL(args.at(-1));
appendFileSync(log, `${JSON.stringify(args)}\n`);
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
// Something Tributary can't fetch, which has to be left out. (HLS is read now, and has its
// own cases below.)
const stream = {
  format_id: 'dash',
  url: `${base}/static/real.mpd`,
  protocol: 'http_dash_segments',
  ext: 'mp4',
  vcodec: 'avc1',
  acodec: 'none',
  height: 2160,
  width: 3840,
  tbr: 9000,
};

if (page.pathname === '/embedded-player') {
  if (!referer || page.searchParams.get('case') === 'stuck') {
    fail('[generic] embedded-player: Unable to download webpage: HTTP Error 404: Not Found');
  }
  console.log(JSON.stringify({ title: 'Embedded film', duration: 6, formats: [picture, sound] }));
  process.exit(0);
}
if (page.pathname === '/embed-host') {
  console.log(JSON.stringify({ title: 'Film found on its page', duration: 6, formats: [picture, sound] }));
  process.exit(0);
}

switch (page.searchParams.get('case')) {
  case 'drm':
    console.log(JSON.stringify({ title: 'Protected', duration: 6, formats: [picture, sound, { ...picture, format_id: 'drm', has_drm: true }] }));
    break;
  case 'live':
    console.log(JSON.stringify({ title: 'Live', is_live: true, live_status: 'is_live', formats: [picture, sound] }));
    break;
  case 'hls':
    // Only HLS, as some sites offer: the test server's own stream, with every quality.
    console.log(
      JSON.stringify({
        title: 'Streamed film',
        duration: 24,
        formats: [
          {
            format_id: 'hls-720',
            protocol: 'm3u8_native',
            url: `${base}/media/high.m3u8`,
            manifest_url: `${base}/media/master.m3u8`,
            ext: 'mp4',
            vcodec: 'avc1.4d401f',
            acodec: 'mp4a.40.2',
            height: 720,
            width: 1280,
            http_headers: { 'User-Agent': 'FakeAgent/1', Cookie: 'leak=1' },
          },
        ],
      }),
    );
    break;
  case 'hls-guarded': {
    // As Dailymotion lists it: the list of every quality turns away anything but a browser,
    // while each quality's own list, with its sound in it, is open.
    const quality = (height, list, bitrate) => ({
      format_id: `hls-${height}`,
      protocol: 'm3u8_native',
      url: `${base}/media/${list}.m3u8`,
      manifest_url: `${base}/file/1K.m3u8?fail=403`,
      ext: 'mp4',
      vcodec: 'avc1.4d401f',
      acodec: 'mp4a.40.2',
      height,
      width: Math.round((height * 16) / 9),
      tbr: bitrate,
      http_headers: { 'User-Agent': 'FakeAgent/1' },
    });
    console.log(JSON.stringify({ title: 'Guarded film', duration: 24, formats: [quality(720, 'high', 2400), quality(360, 'low', 800)] }));
    break;
  }
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
