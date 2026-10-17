#!/bin/sh
# Debian / Ubuntu packages for a Linux release build (.github/scripts/package.sh): a C toolchain, cmake, and the
# headers SDL2's own build looks for. The bundled SDL2 opens X11, Wayland, OpenGL / EGL, ALSA, PulseAudio, D-Bus and
# udev with dlopen at run time, so the binaries do not link them; without these headers SDL would build without them.
#   sudo .github/scripts/linux-deps.sh
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -q
apt-get install -y -q --no-install-recommends \
    build-essential cmake pkg-config \
    libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxfixes-dev libxi-dev libxss-dev libxkbcommon-dev \
    libwayland-dev libegl-dev libgl-dev libgles-dev libdrm-dev libgbm-dev \
    libasound2-dev libpulse-dev libdbus-1-dev libudev-dev
