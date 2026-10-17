# Deploying beyond localhost

The web version is meant for one owner of the game on localhost or a trusted LAN (`port/web/README.md`). This
checklist lists what a public or multi-user deployment has to solve. The model it assumes: the game files are
**served by the server** (players do not load them into the browser, §1) and a public setup keeps the **saves in the
browser** (`xf-serve --saves-in browser`, with export / import in the page, §2).

## 1. Game data (the blocker)

- **The server must not serve the game's files to people who do not own it.** `xf-serve` serves whatever `--data`
  points at to every client; the same goes for the derived files (the QDMC PCM cache and English subtitle pack made by
  `xf-prep`) and for the fonts it takes from the system (Courier New / Arial are Microsoft's). Who may receive the files
  from such a server (licence, access control in front of `xf-serve`) is the operator's question.
- Players do **not** pick their game folder in the browser; `xf-serve --data` serves the files and the page reads them
  from `/data/`.
  - Not implemented: "bring your own files" — the page would ask for the installed game folder
    (`<input webkitdirectory>` / File System Access API), keep it in OPFS / IndexedDB (~3.3 GB, or only what the engine
    reads) and `WebFs` would read from there (`xf_js_fetch` isolates it); it would need a storage-quota prompt,
    `xf-prep`'s conversions in the browser (QDMC → PCM) and free font substitutes.
- State the requirement on the page itself: "you need your own copy of The X-Files Game".

## 2. Saves per player

- **Public setup: saves live in the browser** — run `xf-serve --saves-in browser` (env `XF_SAVES_IN=browser`, H1733):
  no saves dir is opened, `/saves/` is 404 and `/saves-mode` tells the page `browser`, so every visitor keeps their own
  three slots in their browser's IndexedDB, even where the server could store them. The page's **saves** button exports
  one slot (`<NAME>.x`) or all (a zip of `slots.txt` + the `.x` files, the window's saves layout) and imports them back
  (H1734) — the player's backup, and the way to move a game to another browser or to the window.
- `--saves-in server` (one slot set per server, the window's `slots.txt` + `<NAME>.x` in `--saves DIR`, every visitor
  shares it) is for a single-owner, private setup only, which must pass it explicitly; xf-serve exits if that dir is
  not writable. Without `--saves-in` xf-serve uses the server when the dir is writable, else the browser — a public
  deploy must pass `browser` explicitly.
- Per-player server saves (directories behind authentication) are not implemented.
- Limits: `xf-serve` caps 64 saves and 8 MiB per file and checks the HDB header; keep a per-player quota if the server
  stores them.
- Writes from another Origin are refused (`same_origin`, compares Origin with `Host` for http and https); a reverse
  proxy must forward `Host` unchanged.

## 3. TLS and browser features

- **HTTPS is needed** off localhost: without a secure context the browser has no AudioWorklet (the page falls back to a
  deprecated ScriptProcessor on the main thread), no keyboard lock in full screen, and some browsers limit storage.
  Terminate TLS in a reverse proxy (Caddy / nginx) in front of `xf-serve`; `xf-serve` itself speaks plain HTTP/1.1
  only.
- **COOP / COEP are not needed:** the engine uses no `SharedArrayBuffer` or threads (synchronous XHR in a worker,
  OffscreenCanvas, a MessagePort to the AudioWorklet). `Cross-Origin-Embedder-Policy: require-corp` would only start to
  matter if the engine moves to wasm threads.
- Recommended headers at the proxy: `Content-Security-Policy: default-src 'self'; img-src 'self' data:` (the page loads
  nothing from other origins), `X-Content-Type-Options: nosniff` (xf-serve sets it), `Referrer-Policy: same-origin`.

## 4. Caching and bandwidth

- Static files and the wasm are sent with `Cache-Control: no-cache` and no ETag / Last-Modified, so every page load
  downloads the 2 MB wasm again. For a public host: content-hashed file names (`xf_web.<hash>.wasm`) with
  `max-age=31536000, immutable`, or ETags so revalidation answers 304.
- Compression: the wasm compresses well (gzip / brotli at the proxy, `Content-Encoding`). The game files are already
  compressed media; do not compress `/data/` (it breaks Range requests through some proxies).
- Game files go out with `Cache-Control: private, max-age=3600` and HTTP Range; the start reads ~55 MB (60 files), and
  a video with sound downloads its whole file synchronously (the worker stalls while it loads — fine on a LAN,
  noticeable over the internet). A CDN must pass Range through.
- Rate-limit `/data/` per client if the server holds the files (one session reads hundreds of MB over a game).

## 5. Operations

- `xf-serve` is std-only, single-process, one thread per connection, no request size limits beyond the save cap: put it
  behind a proxy with connection / body limits and timeouts.
- Health check: `GET /data-index` (200 when the data dir is readable).
- Logs: `xf-serve` prints to stderr; the page logs engine lines to the browser console only.
- Back up the saves dir if the server keeps saves (`--saves-in server`); in browser mode the players back up their own
  slots with the page's download button (a browser may evict IndexedDB under storage pressure or when site data is
  cleared).
- Browsers to test: Chrome / Edge (all features), Firefox (no keyboard lock: Esc leaves full screen), Safari (check
  OffscreenCanvas 2D and AudioWorklet; the page has fallbacks for both), a phone (touch: tap / long press / drag).
