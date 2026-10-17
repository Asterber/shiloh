# Platforms

## Current

- The port (`port/`, Rust stable) is built and run on macOS (Apple Silicon). The desktop window uses SDL2 through
  `pkg-config sdl2`; tests and the web build need no SDL2 (`--no-default-features`).
- The browser front builds for `wasm32-unknown-unknown` ([port/web/README.md](../port/web/README.md)).
- The code has paths for Linux and Windows (settings and cache directories, [port/README.md](../port/README.md)), but
  neither has been tested.
- Optional: `ffmpeg`, only for `xf-prep` (the one-time QDMC music conversion) and the sound tests that compare against
  it. The port decodes the game's video itself.

## Checks

From `port/`:

```sh
cargo test --release --no-default-features
```

See [CONTRIBUTING.md](../CONTRIBUTING.md#build-run-test) (tests skip without game data).
