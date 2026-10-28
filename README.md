# [<img src="packaging/ffcut.png" width="4%" alt="FFcut">](https://github.com/Woysful/FFcut/releases) FFcut - Video Editor

Small and fast wrapper for FFmpeg.
![preview](assets/preview.webp)
> Heavily vibe coded. Expect issues.

## Features
- Trim
- Crop
- Rotate
- Flip
- Resize
- Presets
- Video effects: colour, vignette, sharpen, denoise, deblock, deband, deflicker, blur, pixelate or cover an area, text blocks, top / bottom caption
- Multi-track audio timeline
- Audio effects: fade in / out, volume keyframes, parametric equalizer, compressor, normalization, de-esser, de-noiser.
- **Any FFmpeg filter** as an audio or video effect: pick it from the list of your ffmpeg, its options become a form
- **Plugins**: LADSPA and LV2 (audio), frei0r (video), run by ffmpeg
- Specific audio track export, or mix all tracks into one
- **Custom FFmpeg command** — the final command is shown and fully editable, and exactly that text is what runs

## Supported Formats

**Input**: anything FFmpeg and mpv can read (MP4, MOV, AVI, MKV, WEBM, FLV, WMV, M4V, ...)

**Output Codecs**:
- Video: H.264, HEVC, VP8, VP9, AV1, ProRes, DNxHD, FFV1, and more (plus hardware encoders)
- Audio: AAC, MP3, Opus, Vorbis, AC-3, FLAC, ALAC, PCM

## Requirements
- **FFmpeg + FFprobe** — found next to the program first, then in `PATH` (or set the paths in *File → Settings*).
- **Linux:** `libmpv2` (`sudo apt install libmpv2` · `sudo pacman -S mpv` · `sudo dnf install mpv-libs`).
- **Windows:** nothing else if you use the zip that includes `libmpv-2.dll` (and FFmpeg if it was bundled).

## 🐧 Installation Linux

### 1. AppImage
 - Download the `.AppImage` from the [release page](https://github.com/woysful/FFcut/releases/latest)
 - Allow it to execute: **Properties -> Permissions -> Execute**, or `chmod +x ./FFcut-*.AppImage`
> It is recommended to use the Gear Lever application for any AppImage installations.
> The AppImage carries its update information, so Gear Lever (or AppImageUpdate) finds and installs new versions by itself.

### 2. Manual setup
- Download `ffcut-linux-x86_64.tar.gz` from the release page
- Unpack it wherever you like and run `./AppRun`

### 3. Manual building

> **Prerequisites**
> - Rust (https://rustup.rs)
> - libmpv headers: `sudo apt install libmpv-dev pkg-config` · `sudo pacman -S mpv` · `sudo dnf install mpv-devel`
> - FFmpeg (to run it)

```bash
git clone https://github.com/woysful/FFcut.git
cd ./FFcut
./build-linux.sh                    # -> dist/FFcut-<version>-x86_64.AppImage (+ .zsync) and dist/ffcut-linux-x86_64.tar.gz
BUNDLE_FFMPEG=1 ./build-linux.sh    # also put ffmpeg/ffprobe from this machine inside
UPDATE_INFO='' ./build-linux.sh     # without update information (forks)
```
The AppImage points Gear Lever at the GitHub releases of the repository in `Cargo.toml`; making the `.zsync` file needs `zsyncmake` (package `zsync`). Upload both `FFcut-<version>-x86_64.AppImage` and `FFcut-<version>-x86_64.AppImage.zsync` to the release.
For a quick run without packaging: `cargo run --release -p ffcut-app`.

#### Wayland and drag & drop
Dragging files into the window does not work on native Wayland (the windowing library, winit, has no support for it yet). Run through XWayland instead: *File → Settings → Display server → X11*, or `ffcut --x11`, or `FFCUT_DISPLAY=x11`. `--wayland` switches back.

## 🪟 Installation Windows

> **Prerequisites**
> - FFmpeg — download [FFmpeg binaries](https://ffmpeg.org/) and place `ffmpeg.exe` and `ffprobe.exe` in `C:\Windows\` or next to `ffcut.exe`,
>   OR put them anywhere and add that folder to `PATH` (skip this if your zip already contains them)

### 1. Manual installing
- Download `FFcut-<version>-windows-x64.zip` from the [release page](https://github.com/woysful/FFcut/releases/latest)
- Unzip it wherever you like and run `ffcut.exe`
- Updates: when a new version is out, *Update now* in the update window downloads the zip, checks its checksum and replaces the files of the folder (the folder must be writable, so not *Program Files*). The release zip is found by its name, `FFcut-<version>-windows-x64.zip`.

### 2. Manual building (on Windows)

> **Prerequisites**
> - Rust with the MSVC toolchain (Visual Studio Build Tools)
> - libmpv dev package (e.g. `mpv-dev-x86_64-*.7z` from [shinchiro builds](https://github.com/shinchiro/mpv-winbuild-cmake/releases)): a folder with `mpv.lib` and `mpv-2.dll`

```bat
git clone https://github.com/woysful/FFcut.git
cd FFcut
set MPV_DIR=C:\libmpv
set FFMPEG_DIR=C:\ffmpeg\bin
build-windows.bat
```
`FFMPEG_DIR` is optional (it bundles ffmpeg/ffprobe into the zip). Result: `dist\FFcut-windows-x64.zip`.

## Plugins

FFcut finds LADSPA and LV2 plugins (audio effects) and frei0r plugins (video effects) in the usual folders of the system and in `LADSPA_PATH`, `LV2_PATH`, `FREI0R_PATH`. More folders can be added in *File → Settings → Plugins*; that page also lists what was found and why a plugin cannot be used. Add a plugin with *+ Add effect → Plugin*. They run inside ffmpeg, so your ffmpeg must be built with `--enable-ladspa`, `--enable-lv2`, `--enable-frei0r`; not every build is (`ffmpeg -filters` lists `ladspa`, `lv2`, `frei0r` when it is), and the plugin list warns when one is missing.

VST, VST3, CLAP, AU and OFX plugins cannot be used: ffmpeg has no host for them.

## Keyboard Shortcuts

| key          | Description                         |
|--------------|-------------------------------------|
| Ctrl+O / N   | Import Video                        |
| Ctrl+S / E   | Export Video                        |
| Ctrl+F       | Video Fullscreen (Esc to leave)     |
| F11          | Application Fullscreen              |
|              |                                     |
| Space        | Play/Pause                          |
| ← →          | Previous/Next frame                 |
| , .          | Previous/Next frame                 |
| Shift+← →    | Seek 5 s                            |
|              |                                     |
| Q            | Set trim start                      |
| E            | Set trim end                        |
| R            | Reset Trim                          |
| C            | Toggle crop                         |
| A            | Toggle loop                         |
|              |                                     |
| Ctrl+Z       | Undo                                |
| Ctrl+Shift+Z / Ctrl+Y | Redo                       |

All shortcuts can be changed in *File → Settings → Hotkeys*; *Help* lists the current ones.

## Development
- `crates/ffcut-core` — all logic without UI: `model/` (export settings: source, video, audio, export), `builder` (settings → ffmpeg argv), `cmdline` (argv ⇄ editable text), `probe` (ffprobe), `run` (background jobs; runs exactly the argv you see: progress, cancel, cleanup), `presets`, `audiofx` / `videofx` (effects → filters), `filters` (ffmpeg's filter list and options), `plugins/` (finding and reading LADSPA, LV2, frei0r plugins), `analysis` (loudness measuring), `waveform`. Tested, including against real ffmpeg.
- `crates/ffcut-app` — the UI (egui + libmpv): `player`, `app` (state and actions), `state` (dialogs, presets, crop…), `theme` (colours, sizes, the egui style), `update` / `install` (update check, installing the Windows zip), `ui/` (one file per area; `ui/widgets.rs` holds the shared building blocks: form rows, value fields, cards, effect stacks, drag handles).
- Changes per version: [CHANGELOG.md](CHANGELOG.md).
- Tests: `cargo test --workspace` (needs `ffmpeg` in `PATH`).
- The version lives in one place: `[workspace.package]` in the root `Cargo.toml`. Release = bump it, build, publish a GitHub Release tagged `v<version>`.
- Debug env: `FFCUT_DEBUG=1`, `FFCUT_MPV_LOG=file`, `FFCUT_NO_AUDIO=1`, `FFCUT_MPV_DR=auto|yes`, `FFCUT_DISPLAY=x11|wayland`, `FFCUT_UPDATE_URL=<url>`, `FFCUT_UPDATE_ASSET=<end of the file name to install>`, `FFCUT_NO_UPDATE=1`.
- Fonts: Manrope and JetBrains Mono (OFL), embedded from `crates/ffcut-app/assets/fonts`.

**Main principle:** on export FFcut runs **exactly the text in the command field**. Nothing is filtered or reinterpreted.
