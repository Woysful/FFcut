# FFcut - Video Editor

Basicaly Electron wrapper for FFmpeg
![](https://github.com/user-attachments/assets/bdc0b022-1fb0-4304-a78a-d50e0f80c8fa)
> Heavily vibe coded. Expect issues.

## Features
- Editing:
  - Trim
  - Crop
  - Video/Audio codecs
  - Specific audio track export
  - Custom FFmpeg command
- Environment:
  - Export Presets
  - Hotkeys
  - File info
  - Loop playback/Lock timeline
  - Fullscreen mode
## Supported Formats

**Input**: MP4, MOV, AVI, MKV, WEBM, FLV, WMV, M4V

**Output Codecs**:
- Video: H.264, H.265/HEVC, VP8, VP9, AV1, ProRes, DNxHD, and more
- Audio: AAC, MP3, Opus, Vorbis, FLAC, ALAC, PCM

> H.264, AV1, VP8/VP9, and other codecs supported by Chromium work instantly and without any problems. Others, such as HEVC (H.265), ProRes, DNxHD, etc., are automatically converted to H.264 video files solely for the purpose of displaying the video stream in the preview window. These files are stored in `/tmp/ffcut-previews` and are automatically cleared on exit.

## 🐧 Installation Linux

**4 installation options:**

### 1. AppImage
 - Download [AppImage](https://github.com/woysful/FFcut/releases/latest/download/FFcut-x86_64-linux.AppImage) file from the release page
 - Place it wherever you like
 - Allow it to execute by checking `Execute` checkbox in **Properties -> Permissions**
 
   Or by executing the following command in terminal: `chmod +x ./FFcut.appimage`
> It is recommended to use the Gear Lever application for any AppImage installations

### 2. Unpacked - Quick Install

```bash
git clone https://github.com/woysful/FFcut.git
cd ./FFcut
./install.sh
```

### 3. Manual setup

- Download [`binaries`](https://github.com/woysful/FFcut/releases/latest/download/FFcut-x86_64-linux.tar.gz)
- Unzip it whenewer you like and run `FFcut`

### 4. Manual building

> **Prerequisites**
> - FFmpeg
> - Node.js

Install Node.js dependencies and build the project:
```bash
git clone https://github.com/woysful/FFcut.git
cd ./FFcut
npm install
npm run build:linux
```

In `./dist` you'll find `AppImage` file and `linux-unpacked` with all the binaries.

## 🪟 Installation Windows

> **Prerequisites**
> - FFmpeg - Download [FFmpeg binaries](https://ffmpeg.org/) and place `FFmpeg.exe` in `C:\Windows\`

**3 Installation options:**

### 1. Setup file

- Download [`Setup file`](https://github.com/woysful/FFcut/releases/latest/download/FFcut.Setup-x86_64-windows.exe)
- Install (lol)

### 2. Manual installing

- Download [`binaries`](https://github.com/woysful/FFcut/releases/latest/download/FFcut-x86_64-windows.zip)
- Unzip it whenewer you like and run `FFcut.exe`


### 4. Manual building

> **Prerequisites**
> - Node.js

Install Node.js dependencies and build the project:
```bash
git clone https://github.com/woysful/FFcut.git
cd FFcut
npm install
npm run build:win
```

## Keyboard Shortcuts

| key     | Description             |
|---------|-------------------------|
| Ctrl+N  | Import Video            |
| Ctrl+S  | Export Video            |
| Ctrl+Q  | Quit                    |
| Ctrl+F  | Video Fullscreen        |
| F11     | Application Fullscreen  |   
|         |                         |
| Space   | Play/Pause              |
| ← →     | Previous/Next frame     |
| , .     | Previous/Next frame     |
|         |                         |
| Q       | Set trim start          |
| E       | Set trim end            |
| R       | Reset Trim              |
| C       | Toggle crop             |
| A       | Toggle loop             |
| Shift+A | Toggle timeline lock    |
