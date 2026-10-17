#!/usr/bin/env bash
# Build and pack one release archive (used by .github/workflows/release.yml; runs locally too).
#
#   .github/scripts/package.sh TAG PLATFORM RUST_TARGET...   desktop archive: xf, xf-serve, xf-prep + web front
#   .github/scripts/package.sh TAG web                       the web front alone (for hosting it, docs/web-hosting.md)
#
#   TAG        release name, e.g. v0.1.0 (only used in the archive name)
#   PLATFORM   e.g. macos-universal, windows-x86_64, linux-x86_64 (only used in the archive name)
#   RUST_TARGET  one or more Rust targets; several (macOS only) are merged into universal binaries with lipo
#
# Output: $DIST (default: dist/ at the repository root) / shiloh-TAG-PLATFORM.zip (.tar.gz for Linux targets).
# Desktop builds use the `sdl-bundled` feature: SDL2 is compiled from the source in the sdl2-sys crate (needs cmake and
# a C compiler) and linked statically, so the binaries need no SDL2 library and no pkg-config on the user's machine.
# Needs: rustup targets (RUST_TARGET..., wasm32-unknown-unknown), cmake, python3 (python on Windows).
set -euo pipefail

usage() {
    sed -n '2,12p' "$0" >&2
    exit 2
}
[ $# -ge 2 ] || usage
TAG=$1
PLATFORM=$2
shift 2
if [ "$PLATFORM" != web ] && [ $# -eq 0 ]; then
    usage
fi

ROOT=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
DIST=${DIST:-$ROOT/dist}
mkdir -p "$DIST"
DIST=$(cd "$DIST" && pwd)
NAME="shiloh-$TAG-$PLATFORM"
STAGE="$DIST/$NAME"
rm -rf "$STAGE"
mkdir -p "$STAGE"

case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) PY=python ;; # python3 may be the Microsoft Store stub there
    *) PY=python3 ;;
esac

# SDL 2.26's CMakeLists asks for cmake_minimum_required(3.0), which CMake >= 4 refuses without this
export CMAKE_POLICY_VERSION_MINIMUM=${CMAKE_POLICY_VERSION_MINIMUM:-3.5}
# the oldest macOS the binaries run on (SDL's GameController / CoreHaptics frameworks are weak-linked)
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0}

# target dir of a cargo build run in $1 (CARGO_TARGET_DIR may be relative to that dir)
target_dir() {
    case "${CARGO_TARGET_DIR:-}" in
        "") echo "$1/target" ;;
        /*) echo "$CARGO_TARGET_DIR" ;;
        *) echo "$1/$CARGO_TARGET_DIR" ;;
    esac
}

# third-party license file of the crates linked into the build: $1 = cargo project dir, $2 = Rust target,
# $3 = features ("" = default), $4 = output file. Each crate's own LICENSE* / COPYING* / NOTICE* files are copied in.
crate_licenses() {
    (cd "$1" && cargo metadata --format-version 1 --locked --filter-platform "$2" ${3:+--features "$3"}) > "$4.json"
    "$PY" -I - "$4.json" "$4" <<'EOF'
import json, os, re, sys
src, out = sys.argv[1:3]
meta = json.load(open(src, encoding="utf-8"))
os.remove(src)
pkgs = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
root = meta["resolve"]["root"]
# crates linked into the binaries: the normal (not build-, not dev-) dependency closure of the root
seen, todo = set(), [root]
while todo:
    i = todo.pop()
    if i in seen:
        continue
    seen.add(i)
    for d in nodes[i]["deps"]:
        if any(k["kind"] is None for k in d["dep_kinds"]):
            todo.append(d["pkg"])
seen.discard(root)
crates = sorted((pkgs[i] for i in seen), key=lambda p: (p["name"], p["version"]))
bad = [p["name"] for p in crates if not p.get("license") or re.search(r"GPL|MPL|EPL|CDDL|SSPL", p["license"])]
if bad:
    sys.exit("crate_licenses: copyleft or unknown license: " + ", ".join(bad))
lic = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE)", re.I)
with open(out, "w", encoding="utf-8", newline="\n") as f:
    f.write("Third-party Rust crates linked into these binaries\n\n")
    f.write("Generated from Cargo.lock by .github/scripts/package.sh. A summary list, then for each crate its\n")
    f.write("license (SPDX expression), source and the license files shipped in the crate package.\n\n")
    for p in crates:
        f.write("  %s %s  %s\n" % (p["name"], p["version"], p["license"]))
    for p in crates:
        d = os.path.dirname(p["manifest_path"])
        f.write("\n" + "=" * 80 + "\n%s %s (%s)\n" % (p["name"], p["version"], p["license"]))
        f.write((p.get("repository") or "https://crates.io/crates/" + p["name"]) + "\n" + "=" * 80 + "\n")
        files = sorted(x for x in os.listdir(d) if lic.match(x) and os.path.isfile(os.path.join(d, x)))
        if not files:
            f.write("\n(no license file in the crate package; license per its Cargo.toml: %s)\n" % p["license"])
        for x in files:
            f.write("\n--- %s\n\n" % x + open(os.path.join(d, x), encoding="utf-8", errors="replace").read().rstrip() + "\n")
        sdl = os.path.join(d, "SDL", "LICENSE.txt")
        if p["name"] == "sdl2-sys" and os.path.isfile(sdl):
            f.write("\n--- SDL/LICENSE.txt (the SDL2 C library, statically linked into the release binaries)\n\n")
            f.write(open(sdl, encoding="utf-8", errors="replace").read().rstrip() + "\n")
print("crate_licenses: %d crates -> %s" % (len(crates), out), file=sys.stderr)
EOF
}

# the web front: port/web/www with the wasm module in pkg/
build_web() {
    (cd "$ROOT/port/web" && cargo build --release --locked --target wasm32-unknown-unknown)
    "$ROOT/port/web/build.sh"
    [ -f "$ROOT/port/web/www/pkg/xf_web.wasm" ]
    mkdir -p "$1"
    cp -R "$ROOT/port/web/www/." "$1/"
}

docs() {
    cp "$ROOT/README.md" "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$ROOT/port/LICENSE-THIRD-PARTY" "$1/"
}

# zip or tar.gz of $STAGE (with its top-level dir), exec bits kept
pack() {
    "$PY" -I - "$DIST" "$NAME" "$1" <<'EOF'
import os, shutil, sys
dist, name, fmt = sys.argv[1:4]
out = shutil.make_archive(os.path.join(dist, name), fmt, root_dir=dist, base_dir=name)
print(f"package: {out} ({os.path.getsize(out)} bytes)")
EOF
}

if [ "$PLATFORM" = web ]; then
    build_web "$STAGE/web"
    docs "$STAGE"
    crate_licenses "$ROOT/port/web" wasm32-unknown-unknown "" "$STAGE/THIRD-PARTY-CRATES.txt"
    pack zip
    exit 0
fi

EXE=
case "$1" in *windows*) EXE=.exe ;; esac
BINS="xf xf-serve xf-prep"
T=$(target_dir "$ROOT/port")
for target in "$@"; do
    (cd "$ROOT/port" && cargo build --release --locked --features sdl-bundled --target "$target" \
        --bin xf --bin xf-serve --bin xf-prep)
done
for b in $BINS; do
    if [ $# -gt 1 ]; then
        lipo -create -output "$STAGE/$b" $(for t in "$@"; do echo "$T/$t/release/$b"; done)
        lipo -info "$STAGE/$b"
    else
        cp "$T/$1/release/$b$EXE" "$STAGE/"
    fi
done

# the window binary must not need an SDL2 shared library at run time
case "$1" in
    *apple*) LIBS=$(otool -L "$STAGE/xf") ;;
    *linux*) LIBS=$(ldd "$STAGE/xf") ;;
    *) LIBS= ;;
esac
echo "$LIBS"
if echo "$LIBS" | grep -qi sdl; then
    echo "package: xf links a shared SDL2 library" >&2
    exit 1
fi

build_web "$STAGE/web"
docs "$STAGE"
crate_licenses "$ROOT/port" "$1" sdl-bundled "$STAGE/THIRD-PARTY-CRATES.txt"
"$STAGE/xf-serve$EXE" --licenses > /dev/null
case "$1" in
    *linux*) pack gztar ;;
    *) pack zip ;;
esac
