# Running the game

The game runs on the Rust port in `port/`. You need your own installed copy of the game, Rust and SDL2
([CONTRIBUTING.md](../CONTRIBUTING.md#what-you-need)).

```sh
cd port
cargo run --release --bin xf -- --data ../data/InstalledGame
```

`--data` defaults to `XF_DATA`, else `../data/InstalledGame`; it may also name disc images (see
[Running from disc images](#running-from-disc-images)). F10 opens the port's settings screen (scaling, window
size, volume, subtitles, language). Without a window, `--frames N --dump DIR` starts a New Game headless and writes N
PPM frames to DIR.

One-time steps and options are in [port/README.md](../port/README.md):

- `xf-prep` converts the 14 QDMC music tracks with your ffmpeg (otherwise they stay silent);
- `xf-prep --en-subs DIR` takes English subtitles from your English copy;
- `--en-data DIR --game-lang en` plays with the English voice-over and texts.

Browser front: [port/web/README.md](../port/web/README.md) (`port/web/build.sh`, then `xf-serve`).

## Running from disc images

Instead of an installed folder the port can read the game straight from images of your own discs (port feature,
`port/src/iso.rs` + `port/src/vfs.rs`). `--data` may be given several times; each value is an installed folder, a disc
image, or a folder holding the images:

```sh
cd port
cargo run --release --bin xf -- --data ~/XFiles/CD1.iso --data ~/XFiles/CD2.iso ... --data ~/XFiles/CD7.iso
cargo run --release --bin xf -- --data ~/XFiles/images                  # every .iso / .mdf / .bin / .img in it
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
  read-only; files are read from them by offset when the game asks (nothing is extracted or copied).
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
  complete English data set. If a disc keeps these files only inside an InstallShield archive (`DATA1.CAB` of a few
  MB) the port says so: install the game once (or copy the files from an install) and pass that folder first,
  `--data INSTALLED --data CD1.iso ...`; the movies can still come from the images. The RU 1.00 discs' layout was not
  checked against real images (none were at hand); the project's RU data is an installed folder.
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
