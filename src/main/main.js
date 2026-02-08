const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('path');
const fsSync = require('fs');

// Import services
const FFmpegService = require('./services/ffmpeg.service');
const ExportService = require('./services/export.service');
const PresetsService = require('./services/presets.service');
const ContextMenuManager = require('./managers/context-menu.manager');
const Updater = require('./managers/update.manager');

// Initialize services
const ffmpegService = new FFmpegService();
const exportService = new ExportService();
const presetsService = new PresetsService(path.join(app.getPath('userData'), 'export-presets.json'));
const contextMenuManager = new ContextMenuManager();
const updater = new Updater();

let mainWindow;

// Capture CLI file path
const cliFilePath = (() => {
    const args = process.argv.slice(app.isPackaged ? 1 : 2);
    const candidate = args.find(arg => !arg.startsWith('-'));
    if (candidate && fsSync.existsSync(candidate)) {
        return path.resolve(candidate);
    }
    return null;
})();

// macOS open-file event handling
let pendingOpenFile = null;
app.on('open-file', (event, filePath) => {
    event.preventDefault();
    if (mainWindow && mainWindow.webContents) {
        mainWindow.webContents.send('open-file', filePath);
    } else {
        pendingOpenFile = filePath;
    }
});

/**
 * Create main application window
 */
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
            preload: path.join(__dirname, '..', 'preload', 'preload.js'),
            sandbox: false,
            webSecurity: false, // Allow file:// protocol
            enableRemoteModule: false,
            enableBlinkFeatures: 'FileSystemAccess',
            allowRunningInsecureContent: false,
            experimentalFeatures: false
        },
        frame: true,
        titleBarStyle: 'default',
        title: 'FFcut',
        icon: path.join(__dirname, '..', '..', 'assets', 'icon.png'),
        show: false, // Don't show until ready
        vibrancy: process.platform === 'darwin' ? 'sidebar' : undefined,
        visualEffectState: process.platform === 'darwin' ? 'active' : undefined
    });

    mainWindow.loadFile(path.join(__dirname, '..', '..', 'index.html'));

    // Show window when ready to prevent visual flash
    mainWindow.once('ready-to-show', () => {
        mainWindow.show();
    });

    mainWindow.removeMenu();

    // Development - uncomment for debugging
    // mainWindow.webContents.openDevTools();

    // Prevent navigation from dropped files
    mainWindow.webContents.on('will-navigate', (event) => {
        event.preventDefault();
    });

    // Handle renderer process crashes
    mainWindow.webContents.on('render-process-gone', (event, details) => {
        console.error('Renderer process crashed:', details);
        if (details.reason === 'crashed') {
            app.relaunch();
            app.exit(0);
        }
    });
}

/**
 * Initialize application
 */
app.whenReady().then(() => {
    console.log('FFcut v1.4.0 starting...');

    // Single instance lock
    const gotLock = app.requestSingleInstanceLock();
    if (!gotLock) {
        app.quit();
        return;
    }

    // Handle second instance
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

    // Set main window for updater
    updater.setMainWindow(mainWindow);

    // Check for updates on startup (silently)
    setTimeout(() => {
        updater.checkForUpdatesOnStartup();
    }, 3000);

    // Check and update context menu path if needed
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

    // Forward CLI/pending file to renderer
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

/**
 * Handle app cleanup on close
 */
app.on('window-all-closed', async () => {
    console.log('All windows closed, cleaning up...');

    // Clean up temp files
    await ffmpegService.cleanupAllTempFiles();

    // Kill any remaining processes
    ffmpegService.cancel();

    if (process.platform !== 'darwin') {
        app.quit();
    }
});

app.on('before-quit', () => {
    console.log('App shutting down...');
});

/**
 * Global error handlers
 */
process.on('uncaughtException', (error) => {
    console.error('Uncaught exception:', error);
    app.quit();
});

process.on('unhandledRejection', (reason, promise) => {
    console.error('Unhandled rejection at:', promise, 'reason:', reason);
});

// =============================================================================
// IPC Handlers - Video Operations
// =============================================================================

// =============================================================================
// IPC Handlers - App Info

ipcMain.handle('get-version', async () => {
    return app.getVersion();
});

// =============================================================================
// IPC Handlers - Video Operations

ipcMain.handle('select-video', async () => {
    const result = await dialog.showOpenDialog({
        properties: ['openFile'],
        filters: [{
            name: 'Video Files',
            extensions: ['mp4', 'mov', 'avi', 'mkv', 'webm', 'flv', 'wmv', 'm4v']
        }]
    });

    return result.canceled ? null : result.filePaths[0];
});

ipcMain.handle('get-video-info', async (event, filePath) => {
    ffmpegService.reset();

    try {
        const metadata = await ffmpegService.getVideoMetadata(filePath);
        const audioStreams = metadata.streams.filter(s => s.codec_type === 'audio');
        const videoStream = metadata.streams.find(s => s.codec_type === 'video');
        const needsPreview = videoStream && !ffmpegService.isCodecSupported(videoStream.codec_name);
        const multiAudio = audioStreams.length > 1;

        if (multiAudio) {
            if (ffmpegService.isCancelled) {
                throw new Error('Transcoding cancelled by user');
            }
            return { metadata, needsPreview, multiAudio: true };
        }

        if (needsPreview) {
            if (ffmpegService.isCancelled) {
                throw new Error('Transcoding cancelled by user');
            }
            const previewPath = await ffmpegService.createVideoPreview(filePath, event);
            return { metadata, needsPreview: true, previewPath };
        }

        return { metadata, needsPreview: false };
    } catch (error) {
        console.error('Error getting video info:', error);
        throw error;
    }
});

ipcMain.handle('prepare-multi-audio-preview', async (event, filePath, metadata) => {
    return ffmpegService.prepareMultiAudioPreview(filePath, event, metadata);
});

ipcMain.handle('cancel-transcode', async (event) => {
    const filePath = ffmpegService.cancel();
    if (filePath) {
        await ffmpegService.cleanupTempFiles(filePath);
    }
    event.sender.send('transcode-progress', { status: 'cancelled' });
    return true;
});

// =============================================================================
// IPC Handlers - Export Operations
// =============================================================================

ipcMain.handle('export-video', async (event, options) => {
    const { inputPath, outputPath, customCommand } = options;
    return exportService.exportVideo(inputPath, outputPath, customCommand, event);
});

ipcMain.handle('cancel-export', async (event) => {
    exportService.cancel(event);
    return true;
});

ipcMain.handle('select-output', async (event, defaultName) => {
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
            { name: formatNames[ext] || 'Video Files', extensions: [ext] },
            { name: 'All Video Files', extensions: ['mp4', 'mov', 'avi', 'mkv', 'webm', 'flv'] }
        ]
    });

    return result.canceled ? null : result.filePath;
});

// =============================================================================
// IPC Handlers - Preset Management
// =============================================================================

ipcMain.handle('load-presets', async () => {
    try {
        return await presetsService.load();
    } catch (err) {
        console.error('Error loading presets:', err);
        return {};
    }
});

ipcMain.handle('save-preset', async (event, { name, settings }) => {
    return presetsService.savePreset(name, settings);
});

ipcMain.handle('delete-preset', async (event, name) => {
    return presetsService.deletePreset(name);
});

ipcMain.handle('rename-preset', async (event, { oldName, newName }) => {
    return presetsService.renamePreset(oldName, newName);
});

// =============================================================================
// IPC Handlers - Context Menu Integration
// =============================================================================

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

// =============================================================================
// IPC Handlers - Updater
// =============================================================================

ipcMain.handle('updater-check-updates', async () => {
    try {
        await updater.checkForUpdates(false);
        return { success: true };
    } catch (err) {
        console.error('Error checking for updates:', err);
        return { success: false, error: err.message };
    }
});

ipcMain.handle('updater-get-info', async () => {
    try {
        const info = updater.getUpdateInfo();
        return { success: true, ...info };
    } catch (err) {
        console.error('Error getting update info:', err);
        return { success: false, error: err.message };
    }
});

ipcMain.handle('updater-download-and-install', async () => {
    try {
        await updater.performUpdate();
        return { success: true };
    } catch (err) {
        console.error('Error downloading/installing update:', err);
        return { success: false, error: err.message };
    }
});
