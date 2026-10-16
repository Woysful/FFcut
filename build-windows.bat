@echo off
rem Builds FFcut for Windows -> dist\FFcut-windows-x64.zip
rem Required: set MPV_DIR=C:\libmpv   (folder with mpv.lib and mpv-2.dll from a libmpv dev package)
rem Optional: set FFMPEG_DIR=C:\path\to\ffmpeg\bin  (folder with ffmpeg.exe + ffprobe.exe) to bundle them.
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul || (echo Rust not found. Install from https://rustup.rs & exit /b 1)

if not defined MPV_DIR (echo Set MPV_DIR to the folder with mpv.lib and mpv-2.dll & exit /b 1)
set "LIB=%MPV_DIR%;%LIB%"

echo ==^> cargo build --release
cargo build --release -p ffcut-app || exit /b 1

if exist dist\FFcut rmdir /s /q dist\FFcut
mkdir dist\FFcut || exit /b 1
copy /y target\release\ffcut.exe dist\FFcut\ >nul || exit /b 1

copy /y "%MPV_DIR%\*.dll" dist\FFcut\ >nul || (echo No dll found in %MPV_DIR% & exit /b 1)

if defined FFMPEG_DIR (
  copy /y "%FFMPEG_DIR%\ffmpeg.exe"  dist\FFcut\ >nul || exit /b 1
  copy /y "%FFMPEG_DIR%\ffprobe.exe" dist\FFcut\ >nul || exit /b 1
  echo Bundled ffmpeg from %FFMPEG_DIR%
)

echo ==^> zip
if exist dist\FFcut-windows-x64.zip del dist\FFcut-windows-x64.zip
powershell -NoProfile -Command "Compress-Archive -Path dist\FFcut\* -DestinationPath dist\FFcut-windows-x64.zip" || exit /b 1
echo Done: dist\FFcut-windows-x64.zip
