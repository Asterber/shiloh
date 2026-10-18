# Running the game

The game runs on the Rust port in `port/`. You need your own installed copy of the game, Rust and SDL2
([CONTRIBUTING.md](../CONTRIBUTING.md#what-you-need)).

```sh
cd port
cargo run --release --bin xf -- --data ../data/InstalledGame
```

`--data` may also name disc images (see [Running from disc images](#running-from-disc-images)). F10 opens the port's settings screen (scaling, window
size, volume, subtitles, language). Without a window, `--frames N --dump DIR` starts a New Game headless and writes N
PPM frames to DIR.

One-time steps and options are in [port/README.md](../port/README.md):

- `xf-prep` converts the 14 QDMC music tracks with your ffmpeg (otherwise they stay silent);
- `xf-prep --en-subs DIR` takes English subtitles from your English copy;
- `--en-data DIR --game-lang en` plays with the English voice-over and texts.

Browser front: [port/web/README.md](../port/web/README.md) (`port/web/build.sh`, then `xf-serve`).

## Where the game data is found

`xf`, `xf-serve` and `xf-prep` need no flag when they can find the game themselves (`port/src/locate.rs`). First hit
wins:

1. `--data` (repeatable), else `XF_DATA` (a path list). An error here ends the program, as before.
2. Bare path arguments, taken like `--data` (`xf CD1.iso CD2.iso`). This is what dropping a disc image or folder onto
   `xf.exe` does on Windows: Explorer starts it with the dropped paths.
3. The source tree's `data/InstalledGame` when it holds the game (developers and tests).
4. The place remembered in the settings file (`data = …` in `settings.txt`, see [port/README.md](../port/README.md))
   when it still exists. Headless runs (`--frames`) never read it.
5. Next to the program: the executable's own folder (its real path: a binary double-clicked in the macOS Finder runs
   with the home folder as its current folder) and the current folder unless that is the home folder, a drive's root
   or the Windows folder. In each, and in its direct sub-folders only: an installed game folder (it holds
   `XFiles.hdb`) or the disc images (`.iso` / `.mdf` / `.bin` / `.img`) that hold game files. Other images, deeper
   folders and hidden folders are left out. With several copies the most complete one wins (no missing essential
   file), then the English DVD Edition, then the one with more discs; each copy found is logged with the reason.
6. Nothing found: the window asks with a dialog (a disc image, possibly several, or a folder) until the choice holds
   the game, or the player quits. `xf --frames`, `xf-serve` and `xf-prep` print what to do and exit with status 2.

Steps 2, 5 and 6 are written to the settings file once the data has loaded (`xf`, `xf-serve`; not `xf-prep`), so the
next start needs nothing. `--data` and `XF_DATA` are never written: like the other command-line flags they hold for
that run only, so a one-off `--data` never replaces the player's copy. `--no-config` turns the remembered place off
for `xf` (neither read nor written).

Started by a player, with no flags (a double-click or a drop; on Windows also when Explorer made the console for it),
`xf` shows its errors in a message box as well, since the console may close with the program. `xf.exe` stays a console
program, so from a terminal its output is as before. `xf-serve` and `xf-prep` wait for Enter before closing on an error when
Windows made the console for them; `xf-serve` started with no flags also opens its page in the browser (`--open` / `--no-open` decide; the tests never start the binary). A drop
of something that is not the game shows the reason, then the dialog.

## Running from disc images

Instead of an installed folder the port can read the game straight from images of your own discs (port feature,
`port/src/iso.rs` + `port/src/vfs.rs`). `--data` may be given several times; each value is an installed folder, a disc
image, or a folder holding the images:

```sh
cd port
cargo run --release --bin xf -- --data ~/XFiles/CD1.iso --data ~/XFiles/CD2.iso ... --data ~/XFiles/CD7.iso
cargo run --release --bin xf -- --data ~/XFiles/images                  # every .iso / .mdf / .bin / .img in it
cargo run --release --bin xf -- --data ~/X-Files-DE                     # or in its sub-folders (disk1/*.iso … disk7/*.iso)
cargo run --release --bin xf -- --data ../data/InstalledGame --data ~/XFiles/images   # install + discs
cargo run --release --bin xf -- --data ~/en-dvd/XFILES1.mdf              # the EN DVD Edition alone
cargo run --release --bin xf -- --data ../data/InstalledGame --en-data ~/en-dvd/XFILES1.mdf --game-lang en
cargo run --release --no-default-features --bin xf-serve -- --data ~/XFiles/images --en-data ~/en-dvd/XFILES1.mdf
```

Each tool prints on stderr which file set it reads from which image or folder.

`XF_DATA` / `XF_EN_DATA` take the same, several paths separated by `:` (`;` on Windows); `replay`, `explore`, `e2e`
and `xf-prep` (`--data`, `--en-subs IMAGE`) read them too.

- **Images:** ISO 9660 with or without Joliet names; plain 2048-byte sectors (`.iso`, the Alcohol `.mdf` of the EN
  DVD) or raw 2352-byte CD sectors (`.bin` / `.img` / CD `.mdf`, Mode 1 or Mode 2 Form 1). UDF-only, Rock Ridge-only,
  compressed (`.cso`, `.nrg`, `.ccd`/`.sub` sets) and multi-session images are not read. The images are opened
  read-only; files are read from them by offset when the game asks (nothing is extracted or copied). Mac/PC hybrid
  discs (the German CD set, made with Toast) record most files twice under one name, the Macintosh resource fork
  (an ISO 9660 "associated file") first: the port skips those records and reads the data.
- **What is taken from where:** every source is searched for the folders that hold game files: `XFiles.hdb`,
  `XFiles.gam`, `NAV*.NMV`, `X7.PFF`, ... or the media folders `XV`, `XN`, `XS`, `XG`, `XT`. On a disc these are the
  disc root (movies) and the install folders the installer copies from (`MININST`, `MEDINST`). They are layered into
  one game folder: installed folders first, then per disc `MININST`, `MEDINST`, the rest, discs in volume-label order;
  a file comes from the first layer that has it, case-insensitively. A disc with several languages (the EN DVD's
  `English/` and `Francais/`) gives its English folder.
- **What must exist somewhere:** `XFiles.hdb` and `XFiles.gam` (without them the port does not start), and for the
  full game `XFiles.exe` (computer searches, phone, cheats, e-mail replies, cursors), `XFilesT.dll` (interface texts),
  `Jrn.ttr` / `Hcd.ttr` (document fonts) and the movies. On the EN DVD Edition all of them are plain files
  (`English/MININST` holds what an install copies, its `DATA1.CAB` is a 469-byte stub), so the DVD image alone is a
  complete English data set.
- **Game files inside the installer:** on the Russian edition as a single DVD image the movies, `NAV*.NMV` and the
  `.PFF` files are plain files on the disc, but `XFiles.hdb`, `XFiles.gam`, `XFiles.exe`, the `XFiles?.dll` and the
  `.ttr` fonts exist only inside its `Setup.exe` (a Gentee CreateInstall installer). The port then reads them straight
  out of the installer's archive (`port/src/gentee.rs`, PPMd decoder `port/src/ppmd.rs`): when a disc has no
  `XFiles.hdb` as a plain file, every `*.exe` of at least 64 KB in its root (and in its game folders) is checked for
  such an archive; the files of the archive's game group become a layer before the disc's own files. Nothing is
  installed, extracted or cached: stored entries are read from the image by offset, packed ones decoded in memory
  when read (on that DVD all game files are stored; only the installer's own pictures are packed). The files come out
  byte-identical to an installed RU 1.00 copy, so the image alone is the complete RU data set:
  `--data DVD.iso`. The same works for a copy of such a disc in a folder.
- If a disc keeps these files only inside an InstallShield archive (`DATA1.CAB` of a few MB) the port says so: install
  the game once (or copy the files from an install) and pass that folder first, `--data INSTALLED --data CD1.iso ...`;
  the movies can still come from the images. The RU 1.00 CD set's layout was not checked against real images (none
  were at hand). Images of other platforms' versions (the PlayStation discs) are not this game's data.
- **The German CD set** (7 CDs, volumes `XFILES1` … `XFILES7`): CD 1 holds the game's files as plain files in
  `MININST` (`XFILES.HDB`, `XFILES.GAM`, `XFILES.EXE`, the DLLs, the `.TTR` fonts, `X.PFF`, `XT`, `XS`, `XG`) and
  `MEDINST` (`NAVM.NMV`, part of `XV`); its `DATA1.CAB` (64 KB) holds only the installer's own files. Each CD has
  its `NAVn.NMV` and movies at the root. Give all seven images (`--data CD1.iso … --data CD7.iso`, or the folder that
  holds them, also one folder per disc); a missing disc is named at start (its scenes stay black). Its `XFILES.EXE`
  (FileVersion 1.00.19) is a third build; the port reads its tables at their own addresses (RE_NOTES H2320), so the
  computer searches, phone, cheats, e-mail replies and the movie kinds work as on RU 1.00 (without them New Game's
  opening stalled, H2324). German is then the only game language; texts are read as cp1252. The German movies carry
  no subtitles of their own (37 keep a few English sound-effect lines, H2327): the settings offer English subtitles
  only when the English pack is in the cache (`xf-prep --en-subs`) or an English copy is given (`--en-data`, game
  language English), otherwise no "Subtitles" option. Like the EN 2.05 program, 1.00.19 reorders some note-pad days
  (H2321); the port keeps the RU order.
- **The English CD set** (hybrid discs as well): its `XFILES.EXE` and `XFILES.HDB` are byte for byte the RU 1.00 ones,
  only the string table, movies and texts are English (H2329). The game language is taken from the string table, so
  this runs as English data with the movies' own English captions. Checked with CD 1–2 only.
- **The EN DVD Edition alone** (`--data XFILES1.mdf`): the game runs on the English data — its `XFILES.HDB` /
  `XFILES.GAM` and its program `XFILES.EXE`, a later build (FileVersion 2.05.00) than RU 1.00 whose tables the port
  reads at their own addresses (RE_NOTES H2300). English is then the only game language (no `--en-data`, F4 does
  nothing) and subtitles `auto` show the movies' own English captions; the settings screen and the web dialog offer
  no Russian choices. The English DB is RU 1.00's plus about 500 new objects (same objIds otherwise), so RU 1.00
  saves load on it. What the 2.05 program changed in its own code (e.g. the note pad's day order, H2302) is not
  modelled: the port behaves as RU 1.00 there.
- Files the original writes into its folder (`Temp.gam`, `*.FOR`, saves) are not needed; the port's saves go to its
  saves dir (`--saves`), never into the images.
- The window reads each movie file whole when it plays (as from a folder); `xf-serve` sends byte ranges from the
  image.

How the code is organised: [architecture.md](architecture.md).
