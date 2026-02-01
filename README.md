# FFcut - Video Editor

Electron wrapper for FFmpeg

> Heavily vibe coded. Expect issues.

## Supported Formats

**Input**: MP4, MOV, AVI, MKV, WEBM, FLV, WMV, M4V

**Output Codecs**:
- Video: H.264, H.265/HEVC, VP8, VP9, AV1, ProRes, DNxHD, and more
- Audio: AAC, MP3, Opus, Vorbis, FLAC, ALAC, PCM

> H.264, AV1, VP8/VP9, and other codecs supported by Chromium work instantly and without any problems. Others, such as HEVC (H.265), ProRes, DNxHD, etc., are automatically converted to H.264 video files solely for the purpose of displaying the video stream in the preview window. These files are stored in `/tmp/ffcut-previews` and are automatically cleared on exit.

## Installation

Choose one of 3 installation options:

### 1. AppImage
 - Download [AppImage](https://github.com/woysful/FFcut/releases/latest/download/FFcut.appimage) file from the release page
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

### 3. Manual building

### Prerequisites

- **FFmpeg**
- **Node.js**

Install Node.js dependencies and build the project:
```bash
git clone https://github.com/woysful/FFcut.git
cd ./FFcut
npm install
npm run build
```

In `./dist` you'll find `AppImage` file and `linux-unpacked` with all the binaries.

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
