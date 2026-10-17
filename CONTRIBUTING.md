# Contributing

Shiloh reimplements The X-Files Game (1998) from the behaviour of the original program. The rule behind every change
to game logic: the original game is the reference. A behaviour goes in as **confirmed** when it was observed on the
running original; everything else is a **hypothesis** and sits behind a switch. Read
[docs/architecture.md](docs/architecture.md) for the code and [docs/glossary.md](docs/glossary.md) for the words.

## What you need

- **Your own copy of the game.** An installed game directory (the one holding `XFiles.hdb`, `XFiles.gam`, `XV/`,
  `XS/` ...). The engine is built against the Russian 1.00 release; English data is used for subtitles and the
  optional English game language. No game file is in this repository, and none may be added.
- **Rust** (stable, via rustup).
- **SDL2** for the desktop window, found through `pkg-config sdl2` (macOS: `brew install sdl2 pkg-config`). Not needed
  for tests or the web build (`--no-default-features`).
- Optional: **ffmpeg**, for `xf-prep` (the QDMC music cache) and the sound tests that compare against it.
- Optional: the `wasm32-unknown-unknown` target for the web front (`rustup target add wasm32-unknown-unknown`).

Put the game where the tools look by default, `data/InstalledGame/` at the repository root (it is in `.gitignore`), or
pass `--data DIR` / set `XF_DATA`.

## Build, run, test

All from `port/`:

```sh
cargo build --release                                   # library + window (needs SDL2)
cargo run --release --bin xf -- --data ../data/InstalledGame
cargo test --release --no-default-features              # tests, no SDL2 needed
cargo test --release --no-default-features --test game_data -- --nocapture   # is the test data there?
```

To check rendering without opening a window, dump frames: `cargo run --release --bin xf -- --data
../data/InstalledGame --hyp --frames 30 --dump /tmp/xf-frames` starts a New Game headless and writes 30 PPM frames,
one every 500 ms of game time (`--every MS`).

Web front: `port/web/build.sh`, then `cargo run --release --no-default-features --bin xf-serve -- --data
../data/InstalledGame` and open `http://127.0.0.1:8640/` ([port/web/README.md](port/web/README.md)).

### Tests and missing inputs

Most tests open `data/InstalledGame/XFiles.hdb`, and many load saves from `data/bot/` (saves made on the original).
**When these files are missing, the tests return early and pass**, so a green `cargo test` on a fresh clone proves
little. `tests/game_data.rs` prints a warning when either is missing; run it with `--nocapture` as above. The saves
are not distributed; make your own on the original at the places a test names, or write tests that start from a New
Game or from your own save.

Some tests and the `e2e` harness also read recorded traces of the original (`traces/*.jsonl`) and other research
inputs, which are not published. Those tests skip (return early with a note on stderr) when the traces are missing,
even with the game data present, so they check nothing on a public clone. Replacing them with tests that need only
the game data is welcome work.

## How to propose a change

A change of game behaviour:

1. **Describe the behaviour.** What does the original do, where in the game, and how do you know (you played it on
   the original, a save shows it, a recording)? Give a prediction that someone could check on the original.
2. **Implement it behind a `Rules` flag.** Add a field to `world::Rules` with a doc comment saying what it changes
   and what the evidence is; turn it on in `Rules::all()`. The old behaviour stays reachable with the flag off.
3. **Add a test** in `port/tests/` that shows the difference with the flag on and off.
4. **Run `cargo test`.**

Port features that are not the original's (settings screen, subtitle packs, scaling) need no evidence, but they must
not change game logic and should say "port feature" in their doc comment.

Bug reports are welcome: say where in the game it happens, what Shiloh does and what the original does.

## Commits

- Small commits, one topic each, with a message that says what changed and why.
- Do not rewrite published history.

## What not to commit

- **Game data**: anything from the game (movies, pictures, sounds, the HDB / GAM, saves, DLLs, the EXE).
- **Game frames**: screenshots of the original or of Shiloh, PPM / PNG frame dumps (`--dump` output goes to a temp
  dir).
- **Game text**: dialogue lines, documents, menu labels, logins and similar strings. Tests read the strings they check
  from the user's HDB at test time instead of spelling them out.
- Build output (`port/target/`), large generated files.

## Provenance: describe behaviour, then implement

Shiloh must be the project's own code.

- Do not copy disassembled or decompiled code of the original program, its control flow, its variable names or its
  constant tables into the source.
- Write the behaviour down first (in the `Rules` doc comment or the pull request): "with the preference off, the AI
  icon is removed and a click there hits nothing". Then implement that description in your own structure.
- Do not copy code from other projects unless its license allows it and it is listed in `port/LICENSE-THIRD-PARTY`.

By contributing you agree that your contribution is licensed under the MIT License or the Apache License 2.0, at the
user's option, as described in the README.
