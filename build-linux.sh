#!/usr/bin/env bash
# Builds FFcut for Linux.
#   ./build-linux.sh              -> dist/FFcut-<ver>-x86_64.AppImage  (+ dist/ffcut-linux.tar.gz)
#   BUNDLE_FFMPEG=1 ./build-linux.sh   also put ffmpeg+ffprobe from this machine inside the AppImage
set -euo pipefail
cd "$(dirname "$0")"

ARCH="$(uname -m)"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
DIST="dist"
APPDIR="$DIST/AppDir"
TOOLS="$DIST/.tools"

command -v cargo >/dev/null || { echo "Rust not found. Install: https://rustup.rs"; exit 1; }

if command -v pkg-config >/dev/null && ! pkg-config --exists mpv; then
  echo "libmpv development files not found. Install: libmpv-dev (Debian/Ubuntu), mpv (Arch), mpv-devel (Fedora)"; exit 1
fi

echo "==> cargo build --release"
cargo build --release -p ffcut-app

echo "==> assembling AppDir"
rm -rf "$APPDIR"; mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib" "$TOOLS"
install -m755 target/release/ffcut "$APPDIR/usr/bin/ffcut"
install -m644 packaging/ffcut.desktop "$APPDIR/ffcut.desktop"
install -m644 packaging/ffcut.png "$APPDIR/ffcut.png"
ln -sf usr/bin/ffcut "$APPDIR/AppRun"

if [ "${BUNDLE_FFMPEG:-0}" = "1" ]; then
  for t in ffmpeg ffprobe; do
    p="$(command -v $t)" || { echo "$t not found for bundling"; exit 1; }
    install -m755 "$p" "$APPDIR/usr/bin/$t"
  done
  echo "    bundled ffmpeg/ffprobe from $(command -v ffmpeg) (needs its shared libs on the target system)"
fi

# NOTE: libmpv.so.2 is NOT bundled (it drags in half of the system); the target needs libmpv2 installed.
# AppRun must put bundled tools first in PATH
rm -f "$APPDIR/AppRun"
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
export LD_LIBRARY_PATH="$HERE/usr/lib:${LD_LIBRARY_PATH:-}"
exec "$HERE/usr/bin/ffcut" "$@"
RUN
chmod +x "$APPDIR/AppRun"

echo "==> tar.gz"
tar -C "$APPDIR" -czf "$DIST/ffcut-linux-$ARCH.tar.gz" usr ffcut.desktop ffcut.png AppRun

echo "==> AppImage"
TOOL="$TOOLS/appimagetool-$ARCH.AppImage"
if [ ! -x "$TOOL" ]; then
  curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage" \
    || { echo "Could not download appimagetool; tar.gz is ready in $DIST/"; exit 0; }
  chmod +x "$TOOL"
fi
# --appimage-extract-and-run: works without FUSE (containers, CI)
OUT="$DIST/FFcut-$VERSION-$ARCH.AppImage"
for try in 1 2 3; do   # appimagetool downloads its runtime from GitHub; retry on hiccups
  ARCH="$ARCH" "$TOOL" --appimage-extract-and-run "$APPDIR" "$OUT" && [ -x "$OUT" ] && break
  echo "attempt $try failed, retrying..."; sleep 3
done
[ -x "$OUT" ] || { echo "AppImage was NOT created (see errors above). tar.gz is in $DIST/"; exit 1; }
echo "Done: $OUT"
