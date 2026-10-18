# Shiloh

**English** | [Русский](README.ru.md)

<p align="center">
  <img width="720" height="546" alt="Shiloh running The X-Files Game in Firefox: the main menu, New Game and the opening scene" src="https://github.com/Asterber/shiloh/raw/assets/trailer.gif" />
</p>

Shiloh is an open-source engine that runs *The X-Files Game* (1998, PC, HyperBole Studios / Fox Interactive) on
current systems, from the player's own copy of the game. It is written in Rust and has two front ends: a native
window (SDL2) and a browser version (WebAssembly, served from your own machine).

It is independently written: the original game was run and traced scene by scene, and the engine was rebuilt to do
what the original does. It contains none of the original program's code; the few tables it needs from the game's
executable are read from your own copy at run time.

**No game data is included. You need your own copy of The X-Files Game** — the original discs, a disc image of
them, or an installed game. Shiloh reads the files from that copy on your machine; it downloads nothing and
distributes nothing of the game.

## Status

- The game can be played from start to finish: complete playthroughs have been done in the browser version on both
  the English DVD Edition and the Russian 1.00 release.
- The engine was developed and checked against the Russian 1.00 release (an installed copy). The retail CDs are
  untested.
- The English DVD Edition runs directly from its disc image (`.iso`, or the Alcohol `.mdf` / `.mds` pair the DVD is
  often imaged as). Its game program is a later build (2.05) than the Russian 1.00 one: Shiloh reads that build's
  tables (computer, phone, notes, cheats, cursors) from your copy, and its data adds about 500 objects to the Russian
  release's. Checked without a window: New Game and the opening, the dialogue box, Russian 1.00 saves loading on the
  English data, and a replay of the whole recorded playthrough (recorded on the Russian release), which matches where
  the two releases' data match. Where the 2.05 program itself behaves differently from 1.00 (for example the order of
  the note pad's days), Shiloh does what 1.00 does.
- The Russian release as a single DVD image (`.iso`) runs directly too. On that disc the game's database, program and
  fonts exist only inside its installer (`Setup.exe`); Shiloh reads them straight out of the installer archive, in
  memory, so nothing has to be installed first and nothing is written next to the image. The files it gets are
  identical to an installed Russian 1.00 copy. Checked without a window: the main menu and New Game with the opening.
- With the English DVD alone, English is the only language: the subtitles are the movies' own English captions
  (F3 or Settings → Subtitles: on; off by default, as in the game), no Russian options.
  With both an English and a Russian copy, Shiloh can switch the game language and show English subtitles over the
  Russian release.
- Platforms: built and played on macOS (Apple Silicon). The Windows build has been run on a Windows PC (starting it
  by dropping a disc image onto `xf.exe`). Linux is untested. The browser version needs a current browser with WebAssembly; played through in Firefox.
- Expect bugs. Where the port's behaviour differs from the original game, a report with the place in the game and
  what the original does there is very welcome.

## Known issues

- The last chase with Mulder near the end of the game cannot currently be won with normal clicks. To get past it,
  click the Artificial Intuition (hint) icon and then the action it suggests.

## Features

- Native window with scaling options (AMD FidelityFX Super Resolution 1.0, linear, nearest), window or full screen,
  and a settings screen (F10).
- Browser version: the engine runs in a Web Worker; a small local server (`xf-serve`) serves the page and your game
  files. Saves can stay on that server or in the browser, and can be moved between the browser and the window.
- Subtitles: English subtitles taken from your own English copy (with the English DVD as the game data, straight
  from its movies). Russian subtitles from a translation table you supply (nothing of it ships with Shiloh). F3
  switches captions on and off.
- Game language switch (F4) between the Russian and the English data, when both are present.
- Saves use the original game's save-file format.
- The game's QDesign (QDMC) music tracks are converted once with your own `ffmpeg` (`xf-prep`); everything else,
  including all video, is decoded by Shiloh itself.
- The original game's hidden cheat codes, logins and debug keys work: see [EASTER-EGGS.md](EASTER-EGGS.md) (spoilers).
- For developers: a hotspot overlay (F1) and a headless mode that writes frames to a directory (`--frames N --dump
  DIR`).

## Downloads

Ready-made builds are on this repository's GitHub Releases page, one archive per platform:
`shiloh-<version>-macos-universal.zip` (Apple Silicon and Intel), `shiloh-<version>-windows-x86_64.zip`,
`shiloh-<version>-linux-x86_64.tar.gz`, and `shiloh-<version>-web.zip` (the browser front alone, for hosting it
yourself). `SHA256SUMS` lists their checksums. You still need your own copy of the game; no archive contains any of it.

Each archive holds `xf` (the window), `xf-serve` (the browser version's local server; its page is in `web/` next to
it), `xf-prep` (the one-time music conversion), and the license files.

**Windows: just drag your disc image (`.iso` / `.mdf`) or game folder onto `xf.exe`.**

Starting the game:

1. Unpack the archive and double-click `xf` (`xf.exe` on Windows). The first time, it asks for your copy of the
   game: choose the disc image (select every disc of a multi-disc set) or the game folder. The choice is
   remembered, so after that a double-click starts the game.
2. Or put the disc image(s) or the installed game folder next to `xf` (or in a folder beside it): it finds them
   there by itself, the English DVD Edition first when there are several copies.
3. On macOS or Linux you can also start it from a terminal with the path: `./xf /path/to/your-dvd.iso`. On macOS,
   double-clicking `xf` in the Finder opens it in Terminal.
4. `--data` still works and wins over everything else: `./xf --data /path/to/your/game` or
   `./xf-serve --data /path/to/your/game`. The options are the same as below.

`xf-serve` looks for the game the same way (no dialog: it prints what to do when it finds nothing) and, started
with a double-click, opens the game's page in your browser. If something goes wrong after a double-click or a drop,
`xf` shows the error in a message box.

- macOS: the builds are not signed. After unpacking, clear the download quarantine once:
  `xattr -dr com.apple.quarantine shiloh-<version>-macos-universal`. Needs macOS 11 or later.
- Windows: the builds are not signed, so SmartScreen may warn on first start ("More info", then "Run anyway").
- Linux: built on Ubuntu 22.04 (glibc 2.35 or newer). SDL2 is built in; the system's X11 or Wayland, OpenGL and
  ALSA / PulseAudio libraries are used at run time.
- SDL2 is linked into the window binary on every platform; nothing else needs installing (`ffmpeg` only for `xf-prep`).

The macOS build is the one played through; the Windows build has been run on a Windows PC. The Linux archive is built by
CI but has not been tried yet.

## What you need

- Your own copy of the game: a disc image (`.iso`, `.mdf`, `.bin` / `.img`) or the disc itself, or an installed game
  directory (the one holding `XFiles.hdb`, `XFiles.gam`, `XV/`, `XS/`). These disc images work on their own, with
  no installation: the English DVD Edition (e.g. as an `.mdf` / `.mds` image) and the Russian release as a DVD `.iso`
  image (see Status). For discs that keep the game files only inside an InstallShield archive, install the game once
  and pass that folder.
- Rust (stable, via [rustup](https://rustup.rs)).
- SDL2 for the native window, found through `pkg-config` (macOS: `brew install sdl2 pkg-config`). Not needed for the
  browser version.
- Optional: `ffmpeg`, for the one-time music conversion (`xf-prep`).
- Optional: the `wasm32-unknown-unknown` target for the browser version (`rustup target add wasm32-unknown-unknown`).

## Build and run

Native window:

```sh
cd port
cargo run --release --bin xf -- --data /path/to/your/game
```

`--data` takes a game directory or a disc image directly, e.g. `--data /path/to/your-dvd.iso` or
`--data /path/to/XFILES1.mdf`.

Music (one time, needs `ffmpeg`; without it the QDMC tracks stay silent):

```sh
cd port
cargo run --release --bin xf-prep -- --data /path/to/your/game
```

Browser version:

```sh
port/web/build.sh                       # builds port/web/www/pkg/xf_web.wasm
cd port
cargo run --release --no-default-features --bin xf-serve -- --data /path/to/your/game
# open http://127.0.0.1:8640/ and click the start screen
```

`xf-serve` listens on 127.0.0.1 only by default. Do not expose it to the internet: it would serve your copy of the
game to other people.

English subtitles, the English game language, the settings file and all options: [port/README.md](port/README.md)
and [port/web/README.md](port/web/README.md). Keys: Esc opens the game's menu (save / load), F1 hotspots, F3
captions, F4 game language, F10 settings, F11 full screen.

## Working on Shiloh

- [CONTRIBUTING.md](CONTRIBUTING.md): building, testing, how to propose a change, what must never be committed.
- [docs/architecture.md](docs/architecture.md): how the code is organised.
- [docs/glossary.md](docs/glossary.md): the engine's terms.

Source comments cite research ids such as `H1234` (a hypothesis) or `C301` (a behaviour confirmed on the original).
They refer to the project's research notes, which are not part of this repository. Many tests need the game data
(`--data` / `XF_DATA`, or `data/InstalledGame/` at the repository root) and skip when it is missing; some also read
traces of the original that are not published, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Disclaimer

Shiloh is an unofficial fan project. It is not affiliated with, endorsed by, or sponsored by The Walt Disney
Company, 20th Century Studios, Twentieth Century Fox, Fox Interactive, HyperBole Studios, or any other rights holder
of The X-Files Game. "The X-Files" is a trademark of its owner; it is used here only to identify the game this
engine is compatible with. The game itself, its video, music, pictures and text are the property of their owners and
are not part of this project.

## Acknowledgements

- The HyperBole Studios team, who made the game and the VirtualCinema engine.
- Zeffuro, for the [X-Files PC enhancement patch](https://github.com/Zeffuro/x-files-pc-enhancement-patch); its save
  container code is the basis of `port/src/neodb.rs` (MIT, see [port/LICENSE-THIRD-PARTY](port/LICENSE-THIRD-PARTY)).
- mthcore, for the [x-files-game](https://github.com/mthcore/x-files-game) research on the game's database format.
- The FFmpeg project and [MultimediaWiki](https://wiki.multimedia.cx), whose format descriptions (QuickTime, Apple
  RPZA, Cinepak, IMA ADPCM) made the video and sound decoders possible.

## License

Shiloh is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option. Third-party notices: [port/LICENSE-THIRD-PARTY](port/LICENSE-THIRD-PARTY); the programs print them
with `--licenses`, and the browser version links them from its bottom bar.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Shiloh by you, as
defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
