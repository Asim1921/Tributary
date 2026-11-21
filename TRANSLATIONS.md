# Translations

Tributary speaks English, Urdu (اردو), Arabic (العربية), Hindi (हिन्दी) and Spanish (Español). The
Urdu, Arabic, Hindi and Spanish were written without a native speaker. They need one to read
them before they're relied on. This file says where the words are, the rules they follow, and
what most needs checking.

## Where the words are

| What | Where |
|---|---|
| The app's window, its Rust side's messages, notifications, the tray and the phone page | `ui/src/locales/{ur,ar,hi,es}.json` |
| The browser extension | `extension/_locales/{en,ur,ar,hi,es}/messages.json` |

In `ui/src/locales`, each entry's key is the English and its value is the translation. The
English isn't in a file of its own: it's the text in the code, found by
`node scripts/text-keys.mjs`, which lists what each language is missing or no longer uses
(`--keys` prints every text). The window and the Rust side read the same files, the Rust side
through `include_str!`.

## The rules a translation follows

- **`{name}`** is filled in by the app: keep each one exactly, in whatever place the sentence
  needs it.
- **`{count, plural, one {…} other {…}}`** picks a form by the number. Each language can use its
  own forms: Arabic has `zero`, `one`, `two`, `few`, `many` and `other`, Urdu, Hindi and
  Spanish `one` and `other`. `other` must always be there. `#` is the number itself.
- **`<strong>…</strong>`** must stay, the same number of times.
- **Names stay as they are:** Tributary, Windows, ffmpeg, yt-dlp, MP3, FTP, Metalink,
  `robots.txt`, `PATH`, and the keys Ctrl, Alt and Shift. So do Chrome's own button names in
  Urdu and Hindi (**Load unpacked**, **Developer mode**), since Chrome may not show them in those
  languages.
- Text the app sends in pieces is said a piece at a time. "the sound couldn't be saved:
  {problem}" is translated, then whatever fills `{problem}` is looked up on its own. A server's
  own words, such as `404 Not Found`, stay as the server sent them.
- In Urdu and Arabic, what fills a `{name}` keeps its own direction (between Unicode isolate
  marks), so a file name or a server's words never come out back to front.

`npm test` fails if any language is missing a text, has one the app no longer uses, or
changes a `{name}`, a plural's `other` or a tag.

## How the language is chosen

**Options > General > Language** chooses one; **The same as Windows** follows Windows' display
language, or English if Tributary doesn't speak it. The choice shows at once, and **Cancel**
undoes it. The extension follows the browser's language, and the phone page the phone's.

## What most needs a native speaker

- **Urdu:** the words for queue (قطار), checksum (چیک سم) and mirror (متبادل سرور), and whether
  Nastaliq would read better than the Naskh-style Segoe UI that Windows uses for it.
- **Arabic:** the plural forms in counts, especially for videos (مقطع فيديو), and the
  Microsoft-style verbal nouns used for buttons (إضافة، حذف، إلغاء).
- **Hindi:** the mix of English loanwords (डाउनलोड, फ़ाइल, प्रॉक्सी) and Hindi words (कतार,
  गति, उपशीर्षक), which should match what Windows uses in Hindi.
- **Spanish:** written for Spain (ordenador, vídeo). Latin America would say computadora and
  video.
- **Everywhere:** messages that count things with no plural form of their own, such as
  "{count} downloads complete", are phrased to read well with any number ("قيد التنزيل:
  {count}").
