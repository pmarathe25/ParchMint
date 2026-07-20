#!/bin/sh
set -eu

build_dir=${1:-build-release}
app_dir=${2:-"$build_dir/AppDir"}

command -v linuxdeploy >/dev/null 2>&1 || {
  echo "linuxdeploy is required; this script never downloads tools" >&2
  exit 2
}

cmake -S . -B "$build_dir" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr
cmake --build "$build_dir" --parallel
DESTDIR="$app_dir" cmake --install "$build_dir"
linuxdeploy --appdir "$app_dir" \
  --desktop-file packaging/linux/org.parchmint.ParchMint.desktop \
  --icon-file packaging/icons/org.parchmint.ParchMint.svg \
  --output appimage
