# port — the Rust port

`cargo run --release --bin xf -- --data DIR` opens the game window (SDL2); `port/web/README.md` describes the browser
front (`xf-serve`). `DIR` is your installed copy of the game (`XFiles.hdb`, `XV/`, `XS/`, …). Without `--data` the
programs look for it themselves: `XF_DATA`, bare path arguments (`xf CD.iso`, what a drop onto `xf.exe` does),
`../data/InstalledGame`, the place remembered in the settings file, next to the executable; nothing found, the window
asks with a file / folder dialog and remembers the choice
([docs/running-the-game.md](../docs/running-the-game.md#where-the-game-data-is-found), `src/locate.rs`). `--data` may be repeated and may name your disc images (`.iso` / `.mdf` / `.bin` / `.img`)
or a folder of them instead: [docs/running-the-game.md](../docs/running-the-game.md#running-from-disc-images).
These images work directly, nothing to install first: the English DVD Edition (e.g. as an `.mdf` image; its game
files are plain files on the disc) and the Russian edition as a single DVD `.iso` image, whose database, program and
fonts exist only inside its `Setup.exe`: the port reads them out of the installer archive in memory
(`src/gentee.rs`, `src/ppmd.rs`; no extraction, no cache). The same applies to `xf-serve` and `xf-prep`.

**Settings:** F10 in the window (or "Port settings" under Quit on the game's menu screen) opens the port's settings
screen over the paused game: scaling (with a pixelated / smooth preview of the current picture), window size, full
screen, volume, subtitles, subtitle language, game language, long timers, hotspots, pause when minimised — mouse-driven,
each with its hotkey (F1 / F2 / F3 / F11). They are kept in `~/Library/Application Support/xf-port/settings.txt`
(macOS), `~/.config/xf-port/settings.txt` (Linux), `%APPDATA%\xf-port\settings.txt` (Windows), or `XF_CONFIG` /
`--config FILE`; `--no-config` uses none. Command-line flags override the file for that run. The file's `data = …`
line is where the game data was last found (a dropped path, a copy next to the program, the dialog's choice; a path
list); delete the line to be asked again. `--data` is never written there.

## Music in QDMC format (one-time step, needs ffmpeg)

14 files of the game hold their sound as QDesign Music ('QDMC'), which the port does not decode itself: the New Game
intro music (`XV/64421.xmv`), 8 more `XV/*.xmv`, the four `XS/*.mus` music tracks and `XS/89487.amv`. Without the step
below they stay silent (the log says so once). Every other sound track (IMA4 / PCM) plays without it.

Convert them once with your own ffmpeg (macOS: `brew install ffmpeg`):

```sh
cd port
cargo run --release --bin xf-prep -- --data ../data/InstalledGame
```

`xf-prep` scans the data dir, runs `ffmpeg -ignore_editlist 1 -i FILE -map 0:a:0 -f s16le -` for each QDMC sound
track and writes `qdmc-v1/<dir>/<file>.wav` + `.json` (source size and hash, rate, channels) to the PCM cache dir,
about 160 MB for all 14 files, in a few seconds. Nothing is written into the game's data dir; ffmpeg is not linked
into or shipped with the port. Running it again only converts what is missing or stale (`--force` redoes all).

Options: `--cache DIR` (default `XF_CACHE`, else the user cache dir: macOS `~/Library/Caches/xf`, Linux
`$XDG_CACHE_HOME/xf` or `~/.cache/xf`, Windows `%LOCALAPPDATA%\xf\cache`), `--ffmpeg PATH` (default `XF_FFMPEG`, else
`ffmpeg` from `PATH`), `--data DIR`.

The engine then takes these tracks from the cache and plays them like any other sound track (cut to the track's length,
its edit list applied — `src/media/pcmcache.rs`, `src/media/audio.rs`). `xf` and `xf-serve` find the cache in the same
default place; pass `--cache DIR` to both if you used another one. `xf-serve` serves it to the browser under `/cache/`.

Check: `cargo test --release --no-default-features --test qdmc` converts the tracks into a temporary cache and checks
that the New Game intro music comes out non-silent (skipped without the game data or ffmpeg).

## English subtitles from your English copy (one-time step)

The RU release's movies have no subtitle text (their QuickTime text tracks were removed). The English game has them —
in the DVD Edition (and the retail CDs) every dialogue movie `XV/*.XMV` carries its captions as QuickTime text tracks.
`xf-prep` can take them from **your own** English copy and store them in the cache dir; the port then shows them in the
caption strip under the picture of the RU movie of the same name (same picture and timeline; the cues follow the English
speech, so they are approximate for the RU dub). No ffmpeg needed; nothing is written into either game dir.

```sh
cd port
cargo run --release --bin xf-prep -- --en-subs "/path/to/English game" --data ../data/InstalledGame
```

`--en-subs DIR` = the folder of the English game: the DVD Edition's disc root (its `English/` folder is used, `Francais/`
is ignored) or `English/` itself, an English CD, or an English install. Mount the disc image (or extract it with
`7zz x XFILES1.mdf "English/*"`) first. All movies under DIR are scanned (`XV`, `MININST/XV`, `MEDINST/XV`, …);
the result is `<cache>/subs-v1/en.json` (cues per movie, about 220 KB). The DVD Edition gives subtitles to 1144 of the
1302 RU `XV` movies (2617 cues); the other movies have no captions in the English game either. A movie whose duration
differs from the RU one is left out (listed). Same `--cache` / `XF_CACHE` default as the music step.

Russian subtitles (a translation of the English cues, same movies and timings; your own file, nothing of it is in git).
`xf-prep --ru-subs TABLE.json` turns `en.json` into `<cache>/subs-v1/ru.json`: TABLE.json is a JSON object
`{"English cue text": "Russian text", ...}` (`xf-prep --subs-texts` prints the empty template; cue texts missing in the
table stay English and are listed). On another machine copy `ru-table.json` and `en.json` (or run `--en-subs` there)
and run the same command, or just copy `ru.json`; `xf-serve` serves the cache dir, so the web front
picks it up from there. Neither step needs the game data or ffmpeg.

Showing them:

- `xf --subs en` / `xf --subs ru` — use that pack and switch captions on. `--subs auto` (the default) uses the Russian
  pack when it exists, else the English one, but leaves captions as the game has them (off in a new game): **F3**
  toggles them (the original's closed-captioning switch, which also moves the bottom icon bars down by the strip's
  height). `--subs off` never uses them. `XF_SUBS=en|ru|off|auto` does the same.
- browser front: `xf-serve` serves the cache dir; the bottom bar has a **subtitles** select (auto / English / Русский /
  off, kept in localStorage; `?subs=ru` in the URL preselects it for that load). `auto` = Russian when
  `subs-v1/ru.json` is there, else English. Picking a language switches captions on; **F3** toggles them.
- your own `.srt` files (`--subtitles-dir`) still win over the pack for the movies they cover.

When a movie has several caption tracks (drafts of the same lines in the English data), the one QuickTime would show
on top is taken (`src/media/subs.rs`, RE_NOTES H952).

### Game language: English voice-over and texts (optional)

With **your own** English copy (the DVD Edition's disc root or `English/` folder, its disc image `XFILES1.mdf` / `.iso`, or an installed English
game) the
port plays the English voice-over and movies and shows the game's English texts (dialogue lines, notes, documents,
names) while the game itself keeps running on the RU data (saves stay compatible; RE_NOTES H1830–H1837):

```sh
xf --en-data "/path/to/English game" --game-lang en     # or XF_EN_DATA / XF_GAME_LANG
```

**F4** switches RU ↔ EN live (from the next movie / line; a playing one finishes in its language). Browser front:
`xf-serve --en-data DIR` serves it (`/en-index`, `/en/…`); `?game-lang=en` starts in English, **F4** switches. Nothing
is copied or converted. The subtitle language is a separate setting (`--subs`).

With the English DVD Edition **alone** as the game data (`--data XFILES1.mdf`) none of this is needed: English is
the data's own language, the only one offered (no Russian choices, F4 does nothing), and the subtitles are the
movies' own English captions — no pack, no `xf-prep --en-subs` needed: **F3** (or the setting "Subtitles: on", or
the game's own Alt+C preference) shows them, in the window and in the browser (RE_NOTES H2305, H2310).
