#!/usr/bin/env bash
# Cross-builds FFcut for Windows FROM LINUX  ->  dist/FFcut-<ver>-windows-x64.zip
#
#   ./build-windows.sh                    download libmpv for Windows automatically
#   BUNDLE_FFMPEG=1 ./build-windows.sh    also put ffmpeg.exe + ffprobe.exe into the zip
#   MPV_DIR=/path ./build-windows.sh      use your own libmpv dev folder
#                                         (must contain libmpv.dll.a and libmpv-2.dll)
#
# One-time setup:  rustup target add x86_64-pc-windows-gnu
#                  sudo apt install mingw-w64 p7zip-full unzip curl        (Debian/Ubuntu)
#                  sudo pacman -S mingw-w64-gcc p7zip unzip curl           (Arch)
# The result is built with the MinGW toolchain (target x86_64-pc-windows-gnu).
set -euo pipefail
cd "$(dirname "$0")"

TARGET=x86_64-pc-windows-gnu
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
DIST=dist
CACHE="$DIST/.tools/windows"
OUT="$DIST/FFcut-$VERSION-windows-x64"

need() { command -v "$1" >/dev/null || { echo "Missing: $1  ($2)"; exit 1; }; }
need cargo "install Rust: https://rustup.rs"
need x86_64-w64-mingw32-gcc "install the mingw-w64 package"
need curl curl
rustup target list --installed 2>/dev/null | grep -qx "$TARGET" || { echo "Run once: rustup target add $TARGET"; exit 1; }
mkdir -p "$CACHE"

# ---- libmpv for Windows (the "mpv-dev" package from the shinchiro builds)
if [ -z "${MPV_DIR:-}" ]; then
  MPV_DIR="$CACHE/mpv-dev"
  if [ ! -f "$MPV_DIR/libmpv.dll.a" ]; then
    need 7z "install p7zip-full / p7zip"
    echo "==> downloading libmpv for Windows"
    URL="$(curl -fsSL https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest \
          | grep -o 'https://[^"]*mpv-dev-x86_64-[0-9][^"]*\.7z' | head -1)"
    [ -n "$URL" ] || { echo "Could not find the libmpv package. Download 'mpv-dev-x86_64-*.7z' from https://github.com/shinchiro/mpv-winbuild-cmake/releases, unpack it and run: MPV_DIR=<folder> $0"; exit 1; }
    curl -fSL -o "$CACHE/mpv-dev.7z" "$URL"
    rm -rf "$MPV_DIR"; mkdir -p "$MPV_DIR"
    7z x -y -o"$MPV_DIR" "$CACHE/mpv-dev.7z" >/dev/null
  fi
fi
[ -f "$MPV_DIR/libmpv.dll.a" ] || { echo "$MPV_DIR/libmpv.dll.a not found"; exit 1; }
DLL="$(ls "$MPV_DIR"/*.dll 2>/dev/null | head -1 || true)"
[ -n "$DLL" ] || { echo "no libmpv *.dll in $MPV_DIR"; exit 1; }

echo "==> cargo build --release --target $TARGET"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export RUSTFLAGS="${RUSTFLAGS:-} -L native=$MPV_DIR"
cargo build --release --target "$TARGET" -p ffcut-app

echo "==> assembling $OUT"
rm -rf "$OUT"; mkdir -p "$OUT"
cp "target/$TARGET/release/ffcut.exe" "$OUT/"
cp "$MPV_DIR"/*.dll "$OUT/"

if [ "${BUNDLE_FFMPEG:-0}" = "1" ]; then
  need unzip unzip
  echo "==> downloading ffmpeg for Windows"
  [ -f "$CACHE/ffmpeg.zip" ] || curl -fSL -o "$CACHE/ffmpeg.zip" \
    https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip
  rm -rf "$CACHE/ffmpeg"; mkdir -p "$CACHE/ffmpeg"
  unzip -qo "$CACHE/ffmpeg.zip" '*/bin/ffmpeg.exe' '*/bin/ffprobe.exe' -d "$CACHE/ffmpeg"
  cp "$CACHE"/ffmpeg/*/bin/ffmpeg.exe "$CACHE"/ffmpeg/*/bin/ffprobe.exe "$OUT/"
fi

echo "==> zip"
need zip zip
( cd "$DIST" && rm -f "FFcut-$VERSION-windows-x64.zip" && zip -qr "FFcut-$VERSION-windows-x64.zip" "FFcut-$VERSION-windows-x64" )
echo "Done: $DIST/FFcut-$VERSION-windows-x64.zip"
