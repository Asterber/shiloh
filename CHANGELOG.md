# Changelog

Each release has a section `## X.Y.Z`; `tools/release/release.sh` takes the release notes from it.

## 0.1.0

First public release.

- The whole game can be played from start to finish, in a native window (SDL2) or in the browser (WebAssembly,
  served from your own machine by `xf-serve`).
- Runs from your own copy of the game: an installed game directory or a disc image (`.iso`, `.mdf` / `.mds`).
  Played through on the English DVD Edition and on the Russian 1.00 release.
- Window scaling with AMD FidelityFX Super Resolution 1.0, linear or nearest; a settings screen (F10).
- English captions from the movies; with an English and a Russian copy, a game-language switch (F4) and English
  subtitles over the Russian release.
- Saves in the original game's format, movable between the browser and the window.

Known issue: the last chase with Mulder cannot be won with normal clicks; use the Artificial Intuition hint icon and
the action it suggests.
