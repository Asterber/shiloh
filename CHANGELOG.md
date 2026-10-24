# Changelog

Each release has a section `## X.Y.Z`; `tools/release/release.sh` takes the release notes from it.

## 0.3.1

- The finale's action scenes show the original's cursor while a click counts: the fist over the button that traps
  Mulder and in the scenes with Cook, the stiletto over Cook when it is held. Before, the cursor never changed, so it
  was hard to tell when to click. The known issue of the earlier releases ("the last chase with Mulder cannot be won
  with normal clicks") is gone: the finale has been played through with normal clicks.
- The game pauses while the menu's Options, Help, Save or Load screen is shown, as in the original: its movie, sound
  and subtitles no longer go on under that screen.
- The main menu's opening (the X logo forming, then the items fading in) shows alone and takes no mouse input until
  it ends, as in the original; clicks made meanwhile are handled when it ends.

## 0.3.0

- The German 7-CD release: Shiloh now knows its program build (1.00.19), and the game has been played through on it
  from start to finish. Before, after New Game the opening's movies ran one after another instead of together, so for a long time nothing seemed to happen (likely
  also why some movies seemed missing), and the computer searches, phone, cheats and e-mail replies did nothing.
- German game data: German is the only game language, its texts show their umlauts. Its movies carry no subtitles,
  so Settings no longer shows a Subtitles switch that does nothing; with an English subtitle pack (or an English copy
  given with `--en-data`) English subtitles are offered.
- The English retail CDs are recognised as English (their program is the same file as the Russian release's):
  English texts are no longer decoded as Russian, and no Russian subtitle pack is put over them.
- `--data FOLDER` also finds disc images in the folder's sub-folders (e.g. `disk1/*.iso … disk7/*.iso`), and says
  clearly when there are neither game files nor disc images.

## 0.2.1

- Hybrid Mac/PC CDs no longer end in a black screen after the opening movie. Each file on such discs also has a Mac
  copy under the same name, and Shiloh read that instead of the game's file. The German 7-CD release now reaches the
  main menu and the opening; the computer searches, phone, cheats and e-mail replies of its program build don't work
  yet.
- When disc images are missing, Shiloh says which discs, and that disc 1 holds the game's database.

## 0.2.0

Easier start: no command line needed.

- Windows: drag your disc image (`.iso` / `.mdf`) or game folder onto `xf.exe` to play.
- Double-click `xf`: it finds a disc image or game folder placed next to it, or asks for your copy once with a file
  dialog and remembers the choice.
- `xf-serve` finds the game the same way and, started with a double-click, opens the game in your browser.
- Errors after a double-click or a drop show in a message box instead of a console window that closes at once.
- The Russian release as a single DVD `.iso`, whose game files are only inside its installer, now runs directly from
  the image, with no installation.
- Windows: file paths in messages no longer start with `\\?\`.

Known issue (unchanged): the last chase with Mulder cannot be won with normal clicks; use the Artificial Intuition
hint icon and the action it suggests.

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
