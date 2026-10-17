#!/bin/sh
# Build the web front: the engine as wasm32-unknown-unknown → www/pkg/xf_web.wasm (served by xf-serve).
# Needs: rustup target add wasm32-unknown-unknown. No wasm-bindgen / wasm-pack (plain extern "C" ABI).
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
T="${CARGO_TARGET_DIR:-target}"
mkdir -p www/pkg
cp "$T/wasm32-unknown-unknown/release/xf_web.wasm" www/pkg/xf_web.wasm
ls -l www/pkg/xf_web.wasm
