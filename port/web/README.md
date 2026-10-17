# Web front (prototype)

The port's engine (`xf` core: world / hdb / media / front `Session`) compiled to `wasm32-unknown-unknown` and run in a
Web Worker; a local server, `xf-serve`, serves the page and **your own copy** of the game files. No game file is part
of this repository or of the build output.

## Build and run

```sh
rustup target add wasm32-unknown-unknown         # once
port/web/build.sh                                # → port/web/www/pkg/xf_web.wasm
cd port && cargo run --release --bin xf-serve -- --data ../data/InstalledGame
# open http://127.0.0.1:8640/ and click the start screen (the click also unlocks the sound); a progress bar shows
# the engine download (~2 MB) and the game files the start reads (~55 MB)
```

No wasm-bindgen / wasm-pack: the module uses a plain `extern "C"` ABI with hand-written JS glue (`www/worker.js`).

`xf-serve` options: `--data DIR` (installed game, default `XF_DATA` or `../data/InstalledGame`; repeatable, and may
name disc images or a folder of them — the files are then served by ranges straight from the images, see
[Running from disc images](../../docs/running-the-game.md#running-from-disc-images)), `--cache DIR` (the
PCM cache of the QDMC music made once by `xf-prep`, see `port/README.md`; default `XF_CACHE`, else the user cache dir;
served under `/cache/`), `--listen ADDR:PORT`
(default `127.0.0.1:8640`), `--web DIR` (default `port/web/www`), `--trace T.jsonl` (system-value conditions, default
`traces/fo-009.jsonl` when it exists, else none, as the window), `--fonts DIR` (extra font dir; Courier New / Arial are taken from the system
font dirs, as the native UI does), `--saves DIR` (the save slots, default `XF_SAVES`, else the window's saves dir —
`xf::save::default_dir()`; must be writable, never inside the game data), `--saves-in server|browser` (env
`XF_SAVES_IN`; where the save slots live, see Saves).

## Saves

The game's own save / load screens (menu → Save / Load) work as in the window. Where the slots live is the server's
choice (`--saves-in`, H1733; the page reads it from `/saves-mode`):

- `--saves-in server` — **on the server**, in the `--saves` dir, in the window's format (`slots.txt` + `<NAME>.x`):
  they survive a page refresh, another browser or another machine on the LAN, and a save can be copied between the web
  and the window (both use the same default dir on one machine). xf-serve exits when the dir is not writable. For a single-owner, private setup.
- `--saves-in browser` — **in the browser** (IndexedDB `xf-saves`, this browser only; memory only if IndexedDB is
  unavailable). xf-serve opens no saves dir and `/saves/` is 404, even when it could store them. This is the mode for a
  public / multi-user setup (`docs/web-hosting.md`).
- not given — the server when the saves dir is writable, else the browser (the behaviour before the switch).

The bar's **saves** button (H1734) shows where the slots are and exports / imports them: **download** one slot (its
`<NAME>.x`) or **all slots** (a zip of `slots.txt` + the `.x` files — the window's saves dir layout: unzip it there to
continue in the window); **upload…** a `.x` into a slot (named after the file, A–Z 0–9 space -(), as the game types
them; the same name in another slot leaves that slot, as the game's own save does) or **upload all slots…** (that zip,
a zip of the window's saves folder, or that folder's `slots.txt` + `.x` files picked together; replaces all three
slots). Uploads are checked (the HDB header 5 / 0x501, ≤ 8 MiB); a running game reads the slot names again at once.
Works in both modes (in server mode it reads / writes `/saves/`). The menu's New starts over from XFiles.gam, Quit goes
back to the page's start screen.

**LAN:** `--listen 0.0.0.0:8640` makes the server reachable from other machines. The game files are your licensed
copy — use this on a trusted network only and never expose the port to the internet. (Over plain HTTP on a LAN address
the browser has no AudioWorklet; the page falls back to a ScriptProcessor.)

## Start

As the original (H1660 / H1661, static — see RE_NOTES): after the page's click-to-start gate (audio needs a user
gesture) the four intro movies play (logos, the intro), each skipped by a left or
right click; then the main menu (New / Load / Options / Help / Quit, no game running). `?start=menu` = the menu at
once, `?start=new` = New Game at once (the test mode `?every=MS` always starts with New Game).

## Controls

As the SDL window (`port/src/bin/xf.rs`, shared logic in `port/src/front/host.rs`): left click = click, right click =
right click / skips a playing video (a left click during a video is ignored, H322; not on the save / load screens,
H1680), letters / digits / Enter / Backspace / Tab / Space / Delete go to the game as virtual-key codes, Space / Enter
end the playing media, F1 = hotspot overlay, F2 = pixelated / smooth scaling, F3 = captions on / off, Esc = the game's
menu (H169 / H404). The page's **keys** button lists them. Touch: a tap = left click, a long press (0.5 s) = right
click, a press that moves = a drag (idea icons).

**Full screen** (the button or Alt+Enter) puts only the picture on the screen. Where the browser offers a keyboard lock
(Chrome / Edge) Esc still opens the game's menu and the user holds Esc to leave full screen; elsewhere the browser takes
Esc. **pause when hidden** (H1730): `on` (default) = a hidden tab pauses the game (see Hidden tab); `off` = the game
runs on while the tab is hidden. Volume: percent, default 100 % (the window's `--volume` default). **long timers** (the window's
`--long-timers wine|nt`, C191 / H1203): `wine / 9x` (default) = a timer longer than 65 s never fires, as the original
under Wine (the Gordon's Hauling bomb does not go off); `NT` = it fires, as on Windows NT (the bomb ends the game 120 s
after the manifest). It can be changed at any time (it applies to timers armed from then on); `?long-timers=nt`
preselects it for one page load.

**Settings** (F10, the bar's "settings (F10)" button, or "Port settings" on the game's menu screen; H1750–H1755): a
dialog over the paused game with scaling (and a pixelated / smooth preview of the current picture), full screen, volume,
subtitles (as the game / on / off), subtitle language (auto / English / Russian / off, H956), game language (greyed out until
implemented), long timers, hotspots and pause-when-hidden, each with its hotkey. The keys and values are the window's
(`xf::front::settings`); they are kept in this browser's `localStorage` (`xf-settings`; nothing is kept where storage
is blocked). The byte count of the last start (`xf-start-bytes`) sizes the loading bar.

**Hidden tab** (H1681 / H1730): with **pause when hidden** on (default) the worker stops its loop (no ticks, no drawing —
the game clock stops, timers included, 0 % CPU) and the audio context is suspended; both resume when the tab shows
again, the clock from that moment. With it off (H1731) the worker switches to a 100 ms timer loop that draws nothing:
the game clock follows the real elapsed time (a browser that throttles a hidden tab's timers only makes the steps
coarser — the time is caught up in steps of at most 250 ms, up to 60 s per wake), the sound keeps playing (mixed
~1 s ahead), and on return the picture shows the game where it is now. The window's equivalent is
`xf --pause-when-minimised` (default: the window runs on when minimised). While visible the loop runs at most ~60 frames a second
and a frame equal to the previous one is not drawn again. The status line shows the loop's fps and the engine + draw
time per frame.

## How it works

- `xf-serve` (`port/src/bin/xf-serve.rs`, std only): `/` static files of `www/`, `/data/<path>` game files with HTTP
  Range, `/data-index` the file list (one sub-directory level, what `MediaLib` indexes), `/trace.jsonl`,
  `/fonts/<name>.ttf`, `/saves/` (list) and `/saves/<file>` GET / HEAD / PUT / DELETE — only `slots.txt` and
`<NAME>.x` with a name the game lets one type (`xf::save::is_slot_file`), a `.x` must carry the HDB header and be ≤ 8 MiB,
writes are atomic (temp file + rename) and refused from another Origin, at most 64 saves. Paths are confined to their
root (no `..`, symlinks resolved and checked).
- Save slots: `xf::save::Slots` reads / writes through `xf::io` (`Fs::write` / `Fs::is_file`); `WebFs` sends
  `saves/<file>` to `env.xf_js_save`, which the worker answers with synchronous XHR to `/saves/` or from its IndexedDB
  mirror. After each tick / input the worker polls `front::host::request` (menu New / Quit, a Load) like the window.
- Core file access goes through `xf::io` (`port/src/io.rs`); native = `std::fs` (unchanged). The web backend
  (`src/lib.rs`, `WebFs`) reads with synchronous `XMLHttpRequest` in the worker: whole files, or ranges through a
  256 KiB block cache (64 MiB LRU). With this backend movies are read by ranges (only the `moov` atom and the samples
  decoded); a sound track is still downloaded with its whole file.
- `www/worker.js` drives `xf_tick(dt)` once per animation frame, at most ~60 a second (dt clamped to 250 ms like the
  window), and draws the 640×480 frame (game cursor included) on an OffscreenCanvas when it changed (`xf_tick` returns
  null for an unchanged frame); the page scales it with CSS. `{t:'visible'}` from the page stops / restarts the loop. Audio: the shared mixer
  (`front::host::Mix`) is pulled ~120 ms ahead of the wall clock and posted to an AudioWorklet (`www/audio.js`).
- `www/saves.js` (page side): the saves store (`/saves-mode`, `/saves/` or IndexedDB), the saves panel's export /
  import (a stored zip written by hand, stored or deflated zips read — `DecompressionStream`). After an import the page
  posts `{t:'saves-reload'}`: the worker re-reads IndexedDB into its mirror and calls `xf_saves_reload`.
- Test mode: `http://127.0.0.1:8640/?every=500&frames=60` runs 60 ticks of 500 ms and stops — the picture equals
  `xf --frames 60 --dump DIR --hyp --every 500` frame `00059.ppm` (checked pixel-identical).

## Known gaps

QDMC sound without `xf-prep`'s cache (as natively), a new video with sound stalls the worker while its file downloads
(synchronous XHR; fast on localhost), no FSR upscaling, the wasm is downloaded again on every page load (`no-cache`
without ETag), touch input checked with synthetic events only (no real phone yet). Hosting for other people: see
`docs/web-hosting.md` (game data, per-player saves, TLS, caching).
