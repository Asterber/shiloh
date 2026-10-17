# Architecture of the port

This is a map of `port/` (the Rust crate `xf`) for someone who wants to change it. Terms such as View, AssetRef or
Trigger are explained in [glossary.md](glossary.md).

## 1. Layers

```
  game files (your copy)                       traces of the original (private)
        |                                                  |
   hdb.rs  media/  pe.rs  io.rs                       replay.rs  e2e.rs
        |       |                                          |
        v       v                                          v
   world/  (World: rules, variables, triggers, actions)  <-- driven by a trace, or by
   world/run.rs (Runner: command queue, media FIFO, timers, hit-testing)   the Runner (closed loop)
        |
   front/  (Session: clock, media playback, 640x480 frame, input mapping, menus, UI drawing)
        |
   bins:  xf (SDL2 window)   xf-serve + port/web (browser)   e2e / replay / explore (tools)
```

Each layer only calls downwards.

- **Data readers** (`hdb.rs`, `neodb.rs`, `media/`, `pe.rs`, `io.rs`). `hdb.rs` reads `XFiles.hdb` (the static game
  database) and any `.gam` / `.x` file (the mutable game state, NeoAccess format, big-endian) into records:
  class id, object id, fields. `media/` turns an AssetRef into a file and decodes movies, pictures and sound. `io.rs`
  is the one place where files are opened. The native build uses `std::fs`; the web build installs a backend that
  fetches byte ranges from `xf-serve`.
- **World** (`world/`). The headless engine core. `World` owns the game state: variables, VCEnabled flags,
  registered TriggerLists, the current Node / Location / ViewPoint / View, shown layouts, the dialogue box, the
  inventory and so on. Its job is to decide: fire an event on an owner, evaluate an action's condition, execute an
  action. It never invents input. Every decision is appended to `World::out` as an `Out` record (`Reg`, `Post`,
  `Exec`, `SetVar`, `Transition`, `Fire`, `Timer`, ...).
- **Runner** (`world/run.rs`). Drives a World without a trace. It keeps the original's single command queue: whatever
  is posted while a command runs is inserted before the older pending commands, in post order (C24). It turns the
  World's outputs back into commands, keeps a one-player media FIFO ("asset started" now, "asset ended" later), runs
  the timers on its clock, applies the scheduler's blocking matrix (`world/sched.rs`, C26), and does hit-testing
  (`Runner::clickables`, `Runner::hit`). It does not know about pixels or wall-clock time.
- **Front** (`front/`). `Session` wraps a Runner with a media clock and everything visible: it decides when a movie
  has ended (from its real duration), composes the 640x480 RGB frame (`Session::render`), maps screen points to
  clickables, and draws the parts of the interface the engine draws natively in the original (the menu screen,
  dialogue box, documents, PDA, computer screens, cursor, captions). Rule: nothing in `front/` decides game logic.
  `front/host.rs` is the platform-independent part of an interactive front (key / mouse / sound mixing), shared by
  the window and the web worker.
- **Bins.**
  - `xf` (`bin/xf.rs`, feature `sdl`): the SDL2 window with an OpenGL upscaler (`gpu/`).
  - `xf-serve` (`bin/xf-serve.rs`) + `port/web/`: the same `Session` compiled to wasm and run in a Web Worker;
    `xf-serve` only serves the page and your game files.
  - `replay`, `e2e`, `explore`, `hdbdump`, `xf-prep`: research and regression tools (section 6).
  - `xfplan`: a private research tool (not in this repository) that links `xf` and plans the bot's clicks on the
    original.

## 2. One frame

The window calls, roughly every 16 ms:

1. `Session::tick(dt)`. If the intro movies play, only the intro advances. If the menu is open, queued commands run but
   the game clock stops. Otherwise `tick_game`:
   - `Runner::advance(now)`: timers whose time is up queue a `Cmd::Timer`;
   - `Runner::run(budget)`: dispatch queued commands until the queue is empty or every remaining command is blocked
     (for example a View's Activation while a video holds the media block);
   - for every media the Runner reports as started, the Session notes its start time; the first one whose duration
     has passed is reported back with `Runner::media_ended`, which queues its Deactivation; then the loop runs the
     queue again (at most 64 rounds per tick);
   - save / load requests from the in-game menu are handled (`save_requests`).
2. `Session::render()` composes the frame: the View's background or the playing movie, layouts and their
   InterfaceItems, icons, the dialogue box, the cursor, captions.
3. `front/host.rs` mixes the sound cues (`Session::audio_cues`) and the platform presents the frame.

Headless code (tests, `e2e`, `xfplan`) does the same with `front::settle` / `front::settle_timers`, which tick a
virtual clock until nothing is queued or playing.

## 3. One click

1. The platform maps the mouse to 640x480 and calls `Session::click_at(x, y)` (or `press_at` / `release_at` for
   separate button events).
2. The Session handles screens it owns (menu, "Try again?" box). Otherwise `Runner::hit(x, y)` returns the topmost
   `Clickable` at that point (nav-hotspot, InterfaceItem, icon, dialogue row, ...).
3. `Runner::click_clickable` queues a command: `Cmd::NavClick` for a nav-hotspot, `Cmd::Click` for other owners.
4. On dispatch, `World::fire(owner, MOUSE_CLICK)` looks up the owner's registered TriggerList and posts the
   ActionList of every Trigger for that event. Each posted action becomes a `Cmd::Action` at the insert point.
5. Each `Cmd::Action` is checked with `World::condition` and run with `World::execute`: set a variable, enable or
   disable an object, play an AssetRef, show a layout, start a timer, call a native procedure (`world/procs.rs`), or
   change View. A View change runs as a sequence of queued steps: Deactivation of the old View, its posts, the old
   nav-hotspots, then Activation of the new ViewPoint and View, its nav-hotspots, Explorables and background (C23).
6. The next `tick` dispatches all of this and the next `render` shows the result.

## 4. Where state lives

| State | Where |
|---|---|
| Static game content (Views, Triggers, Actions, AssetRefs, texts) | `Hdb` of `XFiles.hdb`, read-only, shared |
| Initial / saved mutable state | `Hdb` of `XFiles.gam` or of the loaded `.x` save |
| Variables | `World::vars` (`world/vars.rs`) |
| Enabled / disabled flags | `World` (`enabled`, keyed by class and object id) |
| Registered owners, current View, layouts, dialogue box, inventory, held item | `World` fields |
| Command queue, timers, media FIFO, hover, button state, menu open | `Runner` |
| Clock, movie start times, decoded media caches, player settings, save slots | `Session` |
| Pixels, sound device, window, browser | the platform (`bin/xf.rs`, `port/web`) |

`Session.r` is the Runner and `Session.r.w` is the World, so tests can inspect any level directly.

## 5. Saves

A save of the original is a `.x` file: a byte copy of its working state file `Temp.gam`, in the same NeoAccess format
as `XFiles.gam`. The port reads and writes the same format, so saves move between the port and the original.

- **Save.** The in-game Save screen makes the World output `Out::SaveGame { slot, name }`. `Session::save_requests`
  calls `Session::save_bytes`: `World::save_state` (`world/saving.rs`) collects a `save::SaveState` (variables,
  VCEnabled flags, Node / View, inventory, running timers and looping players, dialogue history, PDA notes, e-mail),
  then `save::write` patches it into the loaded save, or `save::build` creates the save classes from `XFiles.gam`
  when the game was started new (`neodb.rs` edits the NeoAccess containers). `save::Slots` stores the file as
  `<NAME>.x` next to `slots.txt` in the saves directory (`xf::save::default_dir()`, or the browser's IndexedDB on the
  web).
- **Load.** `Out::LoadGame` sets `Session::load_req` to the slot's file. The host (window or web worker) takes it
  with `front::host::request`, opens the file as an `Hdb` and calls `Session::reload(gam, true)`: a fresh Runner and
  World over the save, then `Runner::load_game`, which runs the game start with the save's position (H65:
  Activations fire no triggers while the restore runs). The menu's New is the same call with `XFiles.gam` and
  `load = false`.

## 6. Rules flags

Every behaviour that is not yet confirmed by a trace of the original sits behind a boolean field of
`world::Rules` (`port/src/world/mod.rs`), named after its research id: `h59_timer`, `c296_action_icon_click_off`,
`h2083_ai_icon_pref`, ... Each field has a doc comment saying what it changes and where the evidence is.

- `Rules::default()` = every switch off = confirmed rules only.
- `Rules::all()` = every switch on. The window (`front::rules_hyp()`), the web front, `e2e` and `xfplan` use it.
- `front::rules()` = the small base set (H48, H53), used by `xf --no-hyp` and by `xf --frames N --dump DIR` without
  `--hyp`.

The code checks a switch where the behaviour differs (`if self.rules.h2083_ai_icon_pref { ... }`). Old behaviour
stays reachable with the switch off, which lets a test compare both and lets `replay` / `e2e` measure what a rule
changes.

**Rule for changes:** a change of game behaviour goes behind a new `Rules` flag, with a test in `port/tests/` that
shows the difference (switch on vs off). The flag is added to `Rules::all()`. Its status in the project's research
notes (not in this repository) becomes **confirmed** (a C number) only when a trace of the original shows the
behaviour; until then it is a hypothesis (H number). Port features that are not the original's (settings, subtitles packs, the game language switch) are
marked as port features and do not need a trace, but must not change the game's logic. See
[CONTRIBUTING.md](../CONTRIBUTING.md).

## 7. File map

`port/src/`:

| File | What it is |
|---|---|
| `lib.rs` | crate root, module list |
| `hdb.rs` | reader for `.hdb` / `.gam` / `.x` (NeoAccess 5.x): classes, records, field schemas |
| `neodb.rs` | editing a NeoAccess database (indexes, free list, appends) for the save builder |
| `save.rs` | the `.x` save format: `SaveState`, writer, builder, save slots |
| `io.rs` | the single file-access backend (native `std::fs`, or HTTP ranges on the web) |
| `iso.rs` | read-only ISO 9660 / Joliet disc image reader (port feature) |
| `vfs.rs` | several data sources (installed folder, disc images) merged into one virtual game folder behind `io.rs` |
| `pe.rs` | string tables and cursors from the game's Win32 DLL / EXE resources |
| `exetables.rs` | content tables read at run time from your `XFiles.exe` (RU 1.00 build) |
| `notices.rs` | license notices compiled into every binary (`--licenses`) |
| `replay.rs` | drives a World with the inputs of a trace of the original and compares the decisions |
| `sched_replay.rs` | replays the scheduler's blocking matrix against a trace |
| `e2e.rs` | end-to-end harness: the bot's whole route on the core vs the original |
| `world/mod.rs` | `World`, `Rules`, event and class ids, firing, conditions, the action interpreter, transitions |
| `world/run.rs` | `Runner`: closed-loop command queue, media FIFO, timers, clickables and hit-testing |
| `world/sched.rs` | the scheduler's block matrix (which commands wait while media plays) |
| `world/action.rs` | decoding of a VCAction record (type, condition prefix, operands, procedure calls) |
| `world/vars.rs` | variable store and operand evaluation |
| `world/h76.rs`, `world/h89.rs` | two models of how a View step's follow-up events are queued |
| `world/procs.rs` | native procedures called by type-6 actions (`world/computer.rs` is included from here) |
| `world/computer.rs` | the FBI computer's screens (login, search, mail) |
| `world/devices.rs` | PDA, phone, inventory panel, held objects, right click, key focus |
| `world/icons.rs` | EmotionIcons, IdeaIcons, ActionIcons, inventory items and CharViews as clickables |
| `world/dbox.rs` | the dialogue box: rows, clicks, history panel, scrolling |
| `world/ai.rs` | the "artificial intuition" icon |
| `world/pda.rs` | the PDA note pad |
| `world/moviehot.rs` | hotspots inside movies (gun hits) |
| `world/shootout.rs` | the Smolnikoff shootout's native state |
| `world/saving.rs` | the World's part of a save (`World::save_state`) |
| `front/mod.rs` | `Session`: clock, media timing, frame composition, input, save / load requests |
| `front/host.rs` | the platform-independent part of a front: keys, mouse, sound mixer |
| `front/intro.rs` | cold start: intro movies, then the main menu |
| `front/menu.rs` | the game's menu screen and save / load screens |
| `front/ui.rs` | the dialogue box and other native interface drawing |
| `front/doc.rs` | document texts (`XT/*.xtx`) in close-ups |
| `front/computer.rs`, `front/pda.rs`, `front/ai.rs` | drawing of the computer, PDA and AI icon (state is in `world/`) |
| `front/cursor.rs` | the game cursor |
| `front/light.rs` | View tint, night vision, flashlight |
| `front/rewind.rs` | the game over "Try again?" box |
| `front/altkeys.rs` | the original's Alt hotkeys |
| `front/settings.rs` | the player's settings (port feature) |
| `front/script.rs` | step syntax (`p:X,Y`, `k:VK`, ...) for scripted input in tests and tools |
| `front/avsync.rs` | picture / sound timing |
| `front/text.rs`, `front/font.rs` | TrueType text drawing; a tiny bitmap font for debug overlays |
| `media/mod.rs` | `MediaLib`: AssetRef to clip, frame cache, `MoviePlayer` |
| `media/asset.rs` | AssetRef blob to file references |
| `media/qt.rs` | minimal QuickTime movie reader |
| `media/cinepak2.rs`, `media/rpza2.rs` | video decoders |
| `media/inflate.rs` | zlib / DEFLATE for QuickTime's compressed movie header |
| `media/pict.rs` | `.pff` picture containers and PICT pictures |
| `media/audio.rs`, `media/pcmcache.rs` | sound tracks to PCM; cache of QDMC tracks made by `xf-prep` |
| `media/hot.rs` | a movie's `.HOT` hotspot file |
| `media/captions.rs`, `media/subs.rs` | QuickTime text-track captions; subtitle packs (port feature) |
| `media/lang.rs` | RU / EN game language (port feature) |
| `gpu/mod.rs`, `gpu/gl.rs` | letterbox math; OpenGL upscaler (FSR 1.0 / linear / nearest) |
| `bin/xf.rs` | the SDL2 window |
| `bin/xf-serve.rs` | local HTTP server for the web front |
| `bin/xf-prep.rs` | one-time preparation: QDMC music cache, subtitle packs |
| `bin/replay.rs` | `replay` CLI over `replay.rs` |
| `bin/e2e.rs` | `e2e` CLI over `e2e.rs` |
| `bin/explore.rs` | picks the bot's next click from the core's state |
| `bin/hdbdump.rs` | prints every record of an HDB as text |

Elsewhere in `port/`: `tests/` (integration tests, one file per topic), `examples/fsr.rs` (the FSR upscaler on
a still picture), `web/` (the wasm crate and the page),
`shaders/` (FSR shaders).
