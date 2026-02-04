const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('path');
const crypto = require('crypto');
const ffmpeg = require('fluent-ffmpeg');
const fs = require('fs').promises;
const fsSync = require('fs');
const os = require('os');
const ContextMenuManager = require('./context-menu-manager');

let mainWindow;

// Initialize context menu manager
const contextMenuManager = new ContextMenuManager();

// Capture the file path passed via CLI on first launch.
// On Linux/Windows: process.argv = [electron, app.js, /path/to/file.mp4]
// On macOS the path arrives via the 'open-file' event instead (handled below).
const cliFilePath = (() => {
    const args = process.argv.slice(app.isPackaged ? 1 : 2);
    const candidate = args.find(arg => !arg.startsWith('-'));
    if (candidate && fsSync.existsSync(candidate)) return path.resolve(candidate);
    return null;
})();

let isTranscodingCancelled = false;
let currentTranscodeProcess = null;
let currentTranscodeFilePath = null;

// Export cancellation state
let isExportingCancelled = false;
let currentExportProcess = null;
let currentExportOutputPath = null;
let currentExportResolve = null;

// Test function for codec detection (development only)
// Uncomment to test codec detection:
// console.log('H.264 supported:', isCodecSupportedByHTML5('h264'));
// console.log('H.265 supported:', isCodecSupportedByHTML5('hevc'));
// console.log('ProRes supported:', isCodecSupportedByHTML5('prores'));
// console.log('DNxHD supported:', isCodecSupportedByHTML5('dnxhd'));

// Clean up temporary files for a specific video file
async function cleanupTempFilesForVideo(videoPath) {
    if (!videoPath) return;

    try {
        const tempDir = path.join(os.tmpdir(), 'ffcut-previews');

        // Clean up single video preview
        const fileName = path.basename(videoPath, path.extname(videoPath));
        const previewPath = path.join(tempDir, `${fileName}_preview.mp4`);
        try {
            await fs.access(previewPath);
            await fs.unlink(previewPath);
            console.log('Cleaned up cancelled preview file:', previewPath);
        } catch (err) {
            // File doesn't exist, continue
        }

        // Clean up multi-audio temp directory
        const dirHash = crypto.createHash('md5').update(videoPath).digest('hex').slice(0, 12);
        const multiAudioDir = path.join(tempDir, dirHash);
        try {
            await fs.access(multiAudioDir);
            await fs.rm(multiAudioDir, { recursive: true, force: true });
            console.log('Cleaned up cancelled multi-audio temp directory:', multiAudioDir);
        } catch (err) {
            // Directory doesn't exist, continue
        }
    } catch (err) {
        console.warn('Failed to clean up temp files for video:', videoPath, err);
    }
}

// Check if codec is natively supported by HTML5 video
function isCodecSupportedByHTML5(codecName) {
    if (!codecName) return false;

    const codec = codecName.toLowerCase();

    // HTML5 supported codecs
    const supportedCodecs = [
        'h264', 'avc', 'avc1', // H.264
        'vp8', 'vp9',          // WebM VP8/VP9
        'av1',                 // AV1
        'theora'               // Ogg Theora
    ];

    return supportedCodecs.some(supported => codec.includes(supported));
}

// Check available hardware encoders
async function checkHardwareEncoder() {
    return new Promise((resolve) => {
        ffmpeg.getAvailableEncoders((err, encoders) => {
            if (err) {
                console.log('Could not get encoders, falling back to CPU');
                resolve('libx264');
                return;
            }

            // Priority: NVENC > QSV > VAAPI > AMF > CPU
            if (encoders['h264_nvenc']) {
                resolve('h264_nvenc');
            } else if (encoders['h264_qsv']) {
                resolve('h264_qsv');
            } else if (encoders['h264_vaapi']) {
                resolve('h264_vaapi');
            } else if (encoders['h264_amf']) {
                resolve('h264_amf');
            } else {
                resolve('libx264');
            }
        });
    });
}

// Create video-only preview for unsupported codecs (no audio)
async function createVideoOnlyPreview(inputPath, event, outputPath) {
    const encoder = await checkHardwareEncoder();
    return new Promise((resolve, reject) => {
        // Set current file path for cleanup on cancellation
        currentTranscodeFilePath = inputPath;
        // Reset cancellation flag
        isTranscodingCancelled = false;

        const command = ffmpeg(inputPath)
            .outputOptions(['-map', '0:v:0', '-an']);

        if (encoder === 'h264_nvenc') {
            command.videoCodec('h264_nvenc').addOption('-preset', 'fast').addOption('-cq', '28');
        } else if (encoder === 'h264_qsv') {
            command.videoCodec('h264_qsv').addOption('-preset', 'fast').addOption('-global_quality', '28');
        } else if (encoder === 'h264_vaapi') {
            command.videoCodec('h264_vaapi').addOption('-qp', '28');
        } else if (encoder === 'h264_amf') {
            command.videoCodec('h264_amf').addOption('-quality', 'speed').addOption('-qp_i', '28');
        } else {
            command.videoCodec('libx264').addOption('-preset', 'veryfast').addOption('-crf', '28');
        }

        command
            .on('start', (cmd) => {
                console.log('FFmpeg command:', cmd);
                currentTranscodeProcess = command.ffmpegProc;
                event.sender.send('transcode-progress', { status: 'started', encoder });
            })
            .on('progress', (progress) => {
                // Check for cancellation
                if (isTranscodingCancelled) {
                    if (currentTranscodeProcess) {
                        currentTranscodeProcess.kill('SIGKILL');
                    }
                    // Clean up partial file
                    if (fsSync.existsSync(outputPath)) {
                        try {
                            fsSync.unlinkSync(outputPath);
                        } catch (cleanupErr) {
                            console.warn('Failed to clean up cancelled transcode file:', cleanupErr);
                        }
                    }
                    event.sender.send('transcode-progress', { status: 'cancelled' });
                    reject(new Error('Transcoding cancelled by user'));
                    return;
                }

                if (progress.percent) {
                    event.sender.send('transcode-progress', {
                        status: 'progress',
                        percent: Math.round(progress.percent),
                        encoder
                    });
                }
            })
            .on('end', () => {
                currentTranscodeProcess = null;
                currentTranscodeFilePath = null;
                event.sender.send('transcode-progress', { status: 'completed', encoder });
                resolve(outputPath);
            })
            .on('error', (err) => {
                currentTranscodeProcess = null;
                currentTranscodeFilePath = null;
                console.error('Transcode error:', err);

                // Clean up partial file on error (unless cancelled)
                if (!isTranscodingCancelled && fsSync.existsSync(outputPath)) {
                    try {
                        fsSync.unlinkSync(outputPath);
                    } catch (cleanupErr) {
                        console.warn('Failed to clean up failed transcode file:', cleanupErr);
                    }
                }

                if (isTranscodingCancelled) {
                    event.sender.send('transcode-progress', { status: 'cancelled' });
                } else {
                    event.sender.send('transcode-progress', { status: 'error', error: err.message });
                }
                reject(err);
            })
            .save(outputPath);
    });
}

// Create temp preview for unsupported codecs (single audio track)
async function createVideoPreview(inputPath, event) {
    const tempDir = path.join(os.tmpdir(), 'ffcut-previews');
    try {
        await fs.mkdir(tempDir, { recursive: true });
    } catch (err) {
        if (err.code !== 'EEXIST') {
            throw err;
        }
    }

    const fileName = path.basename(inputPath, path.extname(inputPath));
    const outputPath = path.join(tempDir, `${fileName}_preview.mp4`);

    // Check if preview file exists and is valid
    try {
        const stats = await fs.stat(outputPath);
        // Quick validation: check file size (> 100KB for a valid preview)
        if (stats.size > 100 * 1024) { // 100KB minimum
            return outputPath;
        } else {
            // File exists but too small, probably corrupted - delete it
            try {
                await fs.unlink(outputPath);
            } catch (err) {
                console.warn('Failed to remove corrupted preview file:', err);
            }
        }
    } catch (err) {
        // File doesn't exist, continue with creation
    }

    const encoder = await checkHardwareEncoder();
    return new Promise((resolve, reject) => {
        // Set current file path for cleanup on cancellation
        currentTranscodeFilePath = inputPath;
        // Reset cancellation flag
        isTranscodingCancelled = false;

        const command = ffmpeg(inputPath)
            .outputOptions(['-map', '0:v:0', '-map', '0:a:0']);

        if (encoder === 'h264_nvenc') {
            command.videoCodec('h264_nvenc').addOption('-preset', 'fast').addOption('-cq', '28');
        } else if (encoder === 'h264_qsv') {
            command.videoCodec('h264_qsv').addOption('-preset', 'fast').addOption('-global_quality', '28');
        } else if (encoder === 'h264_vaapi') {
            command.videoCodec('h264_vaapi').addOption('-qp', '28');
        } else if (encoder === 'h264_amf') {
            command.videoCodec('h264_amf').addOption('-quality', 'speed').addOption('-qp_i', '28');
        } else {
            command.videoCodec('libx264').addOption('-preset', 'veryfast').addOption('-crf', '28');
        }

        command
            .audioCodec('aac')
            .audioBitrate('128k')
            .on('start', (cmd) => {
                console.log('FFmpeg command:', cmd);
                currentTranscodeProcess = command.ffmpegProc;
                event.sender.send('transcode-progress', { status: 'started', encoder });
            })
            .on('progress', (p) => {
                // Check for cancellation
                if (isTranscodingCancelled) {
                    if (currentTranscodeProcess) {
                        currentTranscodeProcess.kill('SIGKILL');
                    }
                    // Clean up partial file
                    if (fsSync.existsSync(outputPath)) {
                        try {
                            fsSync.unlinkSync(outputPath);
                        } catch (cleanupErr) {
                            console.warn('Failed to clean up cancelled transcode file:', cleanupErr);
                        }
                    }
                    event.sender.send('transcode-progress', { status: 'cancelled' });
                    reject(new Error('Transcoding cancelled by user'));
                    return;
                }

                if (p.percent) {
                    event.sender.send('transcode-progress', {
                        status: 'progress',
                        percent: Math.round(p.percent),
                        encoder
                    });
                }
            })
            .on('end', () => {
                currentTranscodeProcess = null;
                currentTranscodeFilePath = null;
                event.sender.send('transcode-progress', { status: 'completed', encoder });
                resolve(outputPath);
            })
            .on('error', (err) => {
                currentTranscodeProcess = null;
                currentTranscodeFilePath = null;
                console.error('Transcode error:', err);

                // Clean up partial file on error (unless cancelled)
                if (!isTranscodingCancelled && fsSync.existsSync(outputPath)) {
                    try {
                        fsSync.unlinkSync(outputPath);
                    } catch (cleanupErr) {
                        console.warn('Failed to clean up failed transcode file:', cleanupErr);
                    }
                }

                if (isTranscodingCancelled) {
                    event.sender.send('transcode-progress', { status: 'cancelled' });
                } else {
                    event.sender.send('transcode-progress', { status: 'error', error: err.message });
                }
                reject(err);
            })
            .save(outputPath);
    });
}

// Split multi-audio file into video-only + one file per audio track (at import, one-time)
async function prepareMultiAudioPreview(filePath, event, metadata) {
    // Set current file path for cleanup on cancellation
    currentTranscodeFilePath = filePath;
    // Reset cancellation flag for new operation
    isTranscodingCancelled = false;

    // Check for cancellation at start
    if (isTranscodingCancelled) {
        throw new Error('Transcoding cancelled by user');
    }

    const audioStreams = metadata.streams.filter(s => s.codec_type === 'audio');
    if (audioStreams.length <= 1) {
        return null;
    }

    const videoStream = metadata.streams.find(s => s.codec_type === 'video');
    const needsPreview = videoStream && !isCodecSupportedByHTML5(videoStream.codec_name);
    const dirHash = crypto.createHash('md5').update(filePath).digest('hex').slice(0, 12);
    const tempDir = path.join(os.tmpdir(), 'ffcut-previews', dirHash);
    const videoPath = path.join(tempDir, 'video.mp4');
    const audioPaths = audioStreams.map((_, i) => path.join(tempDir, `audio_${i}.m4a`));

    // Check if all files exist and are valid
    const allExist = fsSync.existsSync(videoPath) && audioPaths.every(p => fsSync.existsSync(p));
    if (allExist) {
        // Validate file sizes
        const videoStats = fsSync.statSync(videoPath);
        const audioStats = audioPaths.map(p => fsSync.statSync(p));

        // Check video file size (> 100KB) and all audio files (> 10KB)
        const videoValid = videoStats.size > 100 * 1024;
        const audioValid = audioStats.every(stats => stats.size > 10 * 1024);

        if (videoValid && audioValid) {
            return { videoPath, audioPaths };
        } else {
            // Some files are corrupted, clean them up
            try {
                if (!videoValid && fsSync.existsSync(videoPath)) fsSync.unlinkSync(videoPath);
                audioPaths.forEach((audioPath, i) => {
                    if (!audioValid || audioStats[i].size <= 10 * 1024) {
                        if (fsSync.existsSync(audioPath)) fsSync.unlinkSync(audioPath);
                    }
                });
            } catch (err) {
                console.warn('Failed to clean up corrupted multi-audio files:', err);
            }
        }
    }

    if (!fsSync.existsSync(tempDir)) {
        fsSync.mkdirSync(tempDir, { recursive: true });
    }

    if (!fsSync.existsSync(videoPath)) {
        if (needsPreview) {
            await createVideoOnlyPreview(filePath, event, videoPath);
        } else {
            await new Promise((resolve, reject) => {
                ffmpeg(filePath)
                    .outputOptions(['-map', '0:v:0', '-c', 'copy', '-an'])
                    .on('error', reject)
                    .on('end', resolve)
                    .save(videoPath);
            });
        }
    }

    // Check for cancellation before starting audio processing
    if (isTranscodingCancelled) {
        throw new Error('Transcoding cancelled by user');
    }

    await Promise.all(audioPaths.map((audioPath, i) => {
        if (fsSync.existsSync(audioPath)) {
            return Promise.resolve();
        }

        // Check for cancellation at start of each audio track processing
        if (isTranscodingCancelled) {
            return Promise.reject(new Error('Transcoding cancelled by user'));
        }

        return new Promise((resolve, reject) => {
            const audioProcess = ffmpeg(filePath)
                .outputOptions(['-map', `0:a:${i}`])
                .audioCodec('aac')
                .audioBitrate('128k')
                .on('error', (err) => {
                    // Clean up partial audio file on error
                    if (fsSync.existsSync(audioPath)) {
                        try {
                            fsSync.unlinkSync(audioPath);
                        } catch (cleanupErr) {
                            console.warn('Failed to clean up partial audio file:', cleanupErr);
                        }
                    }
                    reject(err);
                })
                .on('end', resolve)
                .save(audioPath);

            // Store process reference for potential cancellation
            // Note: This is simplified - in a full implementation you'd track all processes
        });
    }));

    currentTranscodeFilePath = null;
    return { videoPath, audioPaths };
}

function createWindow() {
    mainWindow = new BrowserWindow({
        width: 1200,
        height: 700,
        minWidth: 1200,
        minHeight: 700,
        backgroundColor: '#0a0a0a',
        webPreferences: {
            nodeIntegration: false,
            contextIsolation: true,
            preload: path.join(__dirname, 'preload.js'),
            sandbox: false,
            webSecurity: false, // Allow file:// protocol and file.path access
            enableRemoteModule: false,
            // Enable native file drag-and-drop
            enableBlinkFeatures: 'FileSystemAccess',
            // Additional security options for Electron 40+
            allowRunningInsecureContent: false,
            experimentalFeatures: false
        },
        frame: true,
        titleBarStyle: 'default',
        title: 'FFcut',
        icon: path.join(__dirname, 'assets/icon.png'),
        show: false, // Don't show until ready-to-show
        // Modern window options
        vibrancy: process.platform === 'darwin' ? 'sidebar' : undefined,
        visualEffectState: process.platform === 'darwin' ? 'active' : undefined
    });

    mainWindow.loadFile('index.html');

    // Show window when ready to prevent visual flash
    mainWindow.once('ready-to-show', () => {
        mainWindow.show();
    });

    mainWindow.removeMenu();

    // Development - enable for debugging
    // mainWindow.webContents.openDevTools();

    // Handle file drop events - Forward dropped files to renderer
    mainWindow.webContents.on('will-navigate', (event, url) => {
        // Prevent navigation from dropped files
        event.preventDefault();
    });

    // Handle dropped files
    mainWindow.webContents.on('ipc-message', (event, channel, ...args) => {
        if (channel === 'file-dropped') {
            console.log('File dropped in main process:', args);
        }
    });

    // Handle uncaught exceptions in renderer
    mainWindow.webContents.on('render-process-gone', (event, details) => {
        console.error('Renderer process crashed:', details);
        // Optionally restart the window
        if (details.reason === 'crashed') {
            app.relaunch();
            app.exit(0);
        }
    });
}

// macOS sends file paths via 'open-file' instead of argv.
// Store it here in case it fires before the window is ready.
let pendingOpenFile = null;
app.on('open-file', (event, filePath) => {
    event.preventDefault();
    if (mainWindow && mainWindow.webContents) {
        mainWindow.webContents.send('open-file', filePath);
    } else {
        pendingOpenFile = filePath;
    }
});

// Handle app ready
app.whenReady().then(() => {
    console.log('FFcut v1.2.0 starting...');

    // Single-instance lock: if a second "FFcut file.mp4" is launched while
    // we are already running, forward the path here and quit the new one.
    const gotLock = app.requestSingleInstanceLock();
    if (!gotLock) {
        app.quit();
        return;
    }

    app.on('second-instance', (event, commandLine, workingDirectory) => {
        if (mainWindow) {
            if (mainWindow.isMinimized()) mainWindow.restore();
            mainWindow.focus();
        }

        const args = commandLine.slice(app.isPackaged ? 1 : 2);
        const candidate = args.find(arg => !arg.startsWith('-'));
        if (candidate) {
            const resolved = path.resolve(workingDirectory, candidate);
            if (fsSync.existsSync(resolved) && mainWindow && mainWindow.webContents) {
                mainWindow.webContents.send('open-file', resolved);
            }
        }
    });

    createWindow();

    // Check and update context menu path if needed (e.g., AppImage moved)
    (async () => {
        try {
            const needsUpdate = await contextMenuManager.needsPathUpdate();
            if (needsUpdate) {
                console.log('Context menu path outdated, updating...');
                const result = await contextMenuManager.updatePath();
                if (result.success) {
                    console.log('Context menu path updated successfully');
                } else {
                    console.error('Failed to update context menu path:', result.error);
                }
            }
        } catch (err) {
            console.error('Error checking context menu path:', err);
        }
    })();

    // Forward the initial CLI path (or a pending macOS open-file path) to the
    // renderer once it is ready to receive IPC messages.
    const fileToOpen = cliFilePath || pendingOpenFile;
    if (fileToOpen && mainWindow) {
        mainWindow.once('ready-to-show', () => {
            mainWindow.webContents.send('open-file', fileToOpen);
        });
    }

    // Handle macOS activation
    app.on('activate', () => {
        if (BrowserWindow.getAllWindows().length === 0) {
            createWindow();
        }
    });
}).catch((error) => {
    console.error('Failed to initialize app:', error);
    app.quit();
});

// Handle window close and cleanup
app.on('window-all-closed', async () => {
    console.log('All windows closed, cleaning up...');

    // Clean up temp preview files asynchronously
    const tempDir = path.join(os.tmpdir(), 'ffcut-previews');
    try {
        if (fsSync.existsSync(tempDir)) {
            await fs.rm(tempDir, { recursive: true, force: true });
            console.log('Cleaned up temp preview files');
        }
    } catch (err) {
        console.error('Error cleaning temp files:', err);
    }

    // Clean up any remaining processes
    if (currentTranscodeProcess) {
        try {
            currentTranscodeProcess.kill('SIGTERM');
        } catch (err) {
            console.warn('Error killing transcode process:', err);
        }
    }

    if (process.platform !== 'darwin') {
        app.quit();
    }
});

// Handle app before quit
app.on('before-quit', () => {
    console.log('App shutting down...');
});

// Handle uncaught exceptions
process.on('uncaughtException', (error) => {
    console.error('Uncaught exception:', error);
    app.quit();
});

process.on('unhandledRejection', (reason, promise) => {
    console.error('Unhandled rejection at:', promise, 'reason:', reason);
});

// Handle file drop events
ipcMain.handle('handle-file-drop', async (event, fileName) => {
    // This is a workaround for contextIsolation blocking file.path
    // We'll need to use a different approach
    return null;
});

// IPC Handlers
ipcMain.handle('select-video', async () => {
    const result = await dialog.showOpenDialog({
        properties: ['openFile'],
        filters: [
            { 
                name: 'Video Files', 
                extensions: ['mp4', 'mov', 'avi', 'mkv', 'webm', 'flv', 'wmv', 'm4v'] 
            }
        ]
    });

    if (!result.canceled && result.filePaths.length > 0) {
        return result.filePaths[0];
    }
    return null;
});

ipcMain.handle('get-video-info', async (event, filePath) => {
    // Reset cancellation flag and file path for new video info request
    isTranscodingCancelled = false;
    currentTranscodeFilePath = null;

    try {
        const metadata = await new Promise((resolve, reject) => {
            ffmpeg.ffprobe(filePath, (err, data) => {
                if (err) reject(err);
                else resolve(data);
            });
        });

        const audioStreams = metadata.streams.filter(s => s.codec_type === 'audio');
        const videoStream = metadata.streams.find(s => s.codec_type === 'video');
        const needsPreview = videoStream && !isCodecSupportedByHTML5(videoStream.codec_name);
        const multiAudio = audioStreams.length > 1;
        
        // Extract pixel format from video stream
        if (videoStream && videoStream.pix_fmt) {
            metadata.pix_fmt = videoStream.pix_fmt;
        }

        if (multiAudio) {
            // Check if operation was cancelled before starting multi-audio processing
            if (isTranscodingCancelled) {
                throw new Error('Transcoding cancelled by user');
            }
            return { metadata, needsPreview, multiAudio: true };
        }

        if (needsPreview) {
            // Check if operation was cancelled before starting transcoding
            if (isTranscodingCancelled) {
                throw new Error('Transcoding cancelled by user');
            }

            const previewPath = await createVideoPreview(filePath, event);
            return { metadata, needsPreview: true, previewPath };
        }

        return { metadata, needsPreview: false };
    } catch (error) {
        console.error('Error getting video info:', error);
        throw error;
    }
});

ipcMain.handle('prepare-multi-audio-preview', async (event, filePath, metadata) => {
    return prepareMultiAudioPreview(filePath, event, metadata);
});

ipcMain.handle('cancel-transcode', async (event) => {
    isTranscodingCancelled = true;
    if (currentTranscodeProcess) {
        currentTranscodeProcess.kill('SIGKILL');
        currentTranscodeProcess = null;
    }

    // Clean up any temporary files for the current operation
    if (currentTranscodeFilePath) {
        cleanupTempFilesForVideo(currentTranscodeFilePath);
        currentTranscodeFilePath = null;
    }

    // Send cancelled status if no active process
    event.sender.send('transcode-progress', { status: 'cancelled' });
    return true;
});

ipcMain.handle('cancel-export', async (event) => {
    isExportingCancelled = true;

    // Resolve the export promise with cancelled result
    if (currentExportResolve) {
        currentExportResolve({ cancelled: true });
        currentExportResolve = null;
    }

    if (currentExportProcess) {
        currentExportProcess.kill('SIGKILL');
        currentExportProcess = null;
    }

    // Clean up partial export file
    if (currentExportOutputPath && fsSync.existsSync(currentExportOutputPath)) {
        try {
            fsSync.unlinkSync(currentExportOutputPath);
        } catch (err) {
            console.warn('Failed to clean up partial export file:', err);
        }
    }
    currentExportOutputPath = null;

    // Send cancelled status
    event.sender.send('export-progress', { status: 'cancelled' });
    return true;
});

ipcMain.handle('export-video', async (event, options) => {
    const { inputPath, outputPath, customCommand } = options;

    return new Promise((resolve, reject) => {
        // Reset export cancellation state
        isExportingCancelled = false;
        currentExportOutputPath = outputPath;
        currentExportResolve = resolve;

        // Check if already cancelled before starting
        if (isExportingCancelled) {
            currentExportResolve = null;
            resolve({ cancelled: true });
            return;
        }

        let command = ffmpeg(inputPath);

        // Parse and apply custom command
        if (customCommand && customCommand.trim()) {
            const args = customCommand.trim().split(/\s+/);

            // Apply arguments
            for (let i = 0; i < args.length; i++) {
                const arg = args[i];

                if (arg === '-ss' && i + 1 < args.length) {
                    command = command.setStartTime(parseFloat(args[++i]));
                } else if (arg === '-t' && i + 1 < args.length) {
                    command = command.setDuration(parseFloat(args[++i]));
                } else if (arg === '-c:v' && i + 1 < args.length) {
                    command = command.videoCodec(args[++i]);
                } else if (arg === '-c:a' && i + 1 < args.length) {
                    command = command.audioCodec(args[++i]);
                } else if (arg === '-b:v' && i + 1 < args.length) {
                    command = command.videoBitrate(args[++i]);
                } else if (arg === '-b:a' && i + 1 < args.length) {
                    command = command.audioBitrate(args[++i]);
                } else if (arg === '-vf' && i + 1 < args.length) {
                    command = command.videoFilters(args[++i].replace(/"/g, ''));
                } else if (arg === '-filter_complex' && i + 1 < args.length) {
                    // Handle filter_complex - remove quotes if present
                    command = command.complexFilter(args[++i].replace(/"/g, ''));
                } else if (arg === '-preset' && i + 1 < args.length) {
                    command = command.addOption('-preset', args[++i]);
                } else if (arg === '-crf' && i + 1 < args.length) {
                    command = command.addOption('-crf', args[++i]);
                } else if (arg === '-cq' && i + 1 < args.length) {
                    command = command.addOption('-cq', args[++i]);
                } else if (arg === '-qp' && i + 1 < args.length) {
                    command = command.addOption('-qp', args[++i]);
                } else if (arg === '-global_quality' && i + 1 < args.length) {
                    command = command.addOption('-global_quality', args[++i]);
                } else if (arg === '-pix_fmt' && i + 1 < args.length) {
                    command = command.addOption('-pix_fmt', args[++i]);
                } else if (arg === '-map' && i + 1 < args.length) {
                    command = command.addOption('-map', args[++i]);
                } else if (arg === '-c' && args[i + 1] === 'copy') {
                    command = command.videoCodec('copy').audioCodec('copy');
                    i++;
                }
            }
        }

        command
            .on('start', (commandLine) => {
                console.log('FFmpeg command:', commandLine);
                currentExportProcess = command.ffmpegProc;
            })
            .on('progress', (progress) => {
                // Check for cancellation
                if (isExportingCancelled) {
                    if (currentExportProcess) {
                        currentExportProcess.kill('SIGKILL');
                    }
                    // Clean up partial file
                    if (fsSync.existsSync(outputPath)) {
                        try {
                            fsSync.unlinkSync(outputPath);
                        } catch (cleanupErr) {
                            console.warn('Failed to clean up partial export file:', cleanupErr);
                        }
                    }
                    return;
                }
                event.sender.send('export-progress', progress);
            })
            .on('end', () => {
                currentExportProcess = null;
                currentExportOutputPath = null;
                currentExportResolve = null;
                resolve({ success: true });
            })
            .on('error', (err) => {
                currentExportProcess = null;
                currentExportOutputPath = null;
                currentExportResolve = null;
                console.error('Export error:', err);

                // Clean up partial file on error (unless cancelled)
                if (!isExportingCancelled && fsSync.existsSync(outputPath)) {
                    try {
                        fsSync.unlinkSync(outputPath);
                    } catch (cleanupErr) {
                        console.warn('Failed to clean up failed export file:', cleanupErr);
                    }
                }

                if (isExportingCancelled) {
                    event.sender.send('export-progress', { status: 'cancelled' });
                } else {
                    reject(err);
                }
            })
            .save(outputPath);
    });
});

ipcMain.handle('select-output', async (event, defaultName) => {
    // Extract extension from defaultName to set appropriate filter
    const ext = path.extname(defaultName).toLowerCase().replace('.', '');
    const formatNames = {
        'mp4': 'MP4 Video',
        'mov': 'QuickTime Movie',
        'avi': 'AVI Video',
        'mkv': 'Matroska Video',
        'webm': 'WebM Video',
        'flv': 'Flash Video'
    };

    const result = await dialog.showSaveDialog({
        defaultPath: defaultName,
        filters: [
            {
                name: formatNames[ext] || 'Video Files',
                extensions: [ext]
            },
            {
                name: 'All Video Files',
                extensions: ['mp4', 'mov', 'avi', 'mkv', 'webm', 'flv']
            }
        ]
    });

    if (!result.canceled) {
        return result.filePath;
    }
    return null;
});

// Preset management
const presetsFilePath = path.join(app.getPath('userData'), 'export-presets.json');

async function loadPresets() {
    try {
        const data = await fs.readFile(presetsFilePath, 'utf8');
        return JSON.parse(data);
    } catch (err) {
        if (err.code === 'ENOENT') {
            return {};
        }
        throw err;
    }
}

async function savePresets(presets) {
    await fs.writeFile(presetsFilePath, JSON.stringify(presets, null, 2), 'utf8');
}

ipcMain.handle('load-presets', async () => {
    try {
        return await loadPresets();
    } catch (err) {
        console.error('Error loading presets:', err);
        return {};
    }
});

ipcMain.handle('save-preset', async (event, { name, settings }) => {
    try {
        const presets = await loadPresets();
        presets[name] = settings;
        await savePresets(presets);
        return { success: true };
    } catch (err) {
        console.error('Error saving preset:', err);
        return { success: false, error: err.message };
    }
});

ipcMain.handle('delete-preset', async (event, name) => {
    try {
        const presets = await loadPresets();
        delete presets[name];
        await savePresets(presets);
        return { success: true };
    } catch (err) {
        console.error('Error deleting preset:', err);
        return { success: false, error: err.message };
    }
});

ipcMain.handle('rename-preset', async (event, { oldName, newName }) => {
    try {
        const presets = await loadPresets();
        if (!presets[oldName]) {
            return { success: false, error: 'Preset not found' };
        }
        if (presets[newName] && oldName !== newName) {
            return { success: false, error: 'Preset with this name already exists' };
        }
        presets[newName] = presets[oldName];
        if (oldName !== newName) {
            delete presets[oldName];
        }
        await savePresets(presets);
        return { success: true };
    } catch (err) {
        console.error('Error renaming preset:', err);
        return { success: false, error: err.message };
    }
});

// Context menu integration handlers
ipcMain.handle('context-menu-is-enabled', async () => {
    try {
        const enabled = await contextMenuManager.isEnabled();
        return { success: true, enabled };
    } catch (err) {
        console.error('Error checking context menu status:', err);
        return { success: false, error: err.message, enabled: false };
    }
});

ipcMain.handle('context-menu-enable', async () => {
    try {
        const result = await contextMenuManager.enable();
        return { success: true, ...result };
    } catch (err) {
        console.error('Error enabling context menu:', err);
        return { success: false, error: err.message };
    }
});

ipcMain.handle('context-menu-disable', async () => {
    try {
        const result = await contextMenuManager.disable();
        return { success: true, ...result };
    } catch (err) {
        console.error('Error disabling context menu:', err);
        return { success: false, error: err.message };
    }
});

ipcMain.handle('context-menu-get-info', async () => {
    try {
        const info = contextMenuManager.getPlatformInfo();
        return { success: true, ...info };
    } catch (err) {
        console.error('Error getting context menu info:', err);
        return { success: false, error: err.message };
    }
});

ipcMain.handle('context-menu-needs-path-update', async () => {
    try {
        const needsUpdate = await contextMenuManager.needsPathUpdate();
        return { success: true, needsUpdate };
    } catch (err) {
        console.error('Error checking if context menu needs path update:', err);
        return { success: false, error: err.message, needsUpdate: false };
    }
});

ipcMain.handle('context-menu-update-path', async () => {
    try {
        const result = await contextMenuManager.updatePath();
        return result;
    } catch (err) {
        console.error('Error updating context menu path:', err);
        return { success: false, error: err.message };
    }
});
