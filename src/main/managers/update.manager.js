const { app, dialog, Notification } = require('electron');
const https = require('https');
const fs = require('fs').promises;
const fsSync = require('fs');
const path = require('path');
const os = require('os');
const crypto = require('crypto');
const { exec, spawn } = require('child_process');
const { promisify } = require('util');

const execAsync = promisify(exec);

class Updater {
    constructor() {
        this.currentVersion = app.getVersion();
        this.repoOwner = 'woysful';
        this.repoName = 'ffcut';
        this.updateCheckInProgress = false;
        this.updateAvailable = false;
        this.latestRelease = null;
        this.downloadProgress = 0;
        this.mainWindow = null;
    }

    setMainWindow(window) {
        this.mainWindow = window;
    }

    // Notify renderer about update availability
    notifyRenderer(event, data) {
        if (this.mainWindow && !this.mainWindow.isDestroyed()) {
            this.mainWindow.webContents.send(event, data);
        }
    }

    // Check for updates silently on startup
    async checkForUpdatesOnStartup() {
        try {
            const hasUpdate = await this.checkForUpdates(true);
            if (hasUpdate) {
                this.updateAvailable = true;
                this.notifyRenderer('update-available', {
                    version: this.latestRelease.tag_name,
                    silent: true
                });
            }
        } catch (error) {
            // Silently fail on startup check
            console.log('Startup update check skipped:', error.message);
        }
    }

    // Check for updates (can be called manually or on startup)
    async checkForUpdates(silent = false) {
        if (this.updateCheckInProgress) {
            if (!silent) {
                this.notifyRenderer('update-check-status', {
                    status: 'checking',
                    message: 'Update check already in progress...'
                });
            }
            return false;
        }

        this.updateCheckInProgress = true;

        try {
            if (!silent) {
                this.notifyRenderer('update-check-status', {
                    status: 'checking',
                    message: 'Checking for updates...'
                });
            }

            const release = await this.getLatestRelease();
            
            if (!release || !release.tag_name) {
                throw new Error('Invalid release data');
            }

            const latestVersion = this.normalizeVersion(release.tag_name);
            const currentVersion = this.normalizeVersion(this.currentVersion);

            if (this.compareVersions(latestVersion, currentVersion) > 0) {
                this.latestRelease = release;
                this.updateAvailable = true;

                this.notifyRenderer('update-available', {
                    version: release.tag_name,
                    currentVersion: this.currentVersion,
                    releaseNotes: release.body,
                    silent
                });

                return true;
            } else {
                this.updateAvailable = false;
                
                if (!silent) {
                    this.notifyRenderer('update-check-status', {
                        status: 'up-to-date',
                        message: 'You are using the latest version!'
                    });
                }

                return false;
            }
        } catch (error) {
            console.error('Update check failed:', error);
            
            if (!silent) {
                this.notifyRenderer('update-check-status', {
                    status: 'error',
                    message: `Failed to check for updates: ${error.message}`
                });
            }
            
            return false;
        } finally {
            this.updateCheckInProgress = false;
        }
    }

    // Fetch latest release from GitHub
    async getLatestRelease() {
        return new Promise((resolve, reject) => {
            const options = {
                hostname: 'api.github.com',
                path: `/repos/${this.repoOwner}/${this.repoName}/releases/latest`,
                method: 'GET',
                headers: {
                    'User-Agent': 'FFcut-Updater'
                }
            };

            const req = https.request(options, (res) => {
                let data = '';

                res.on('data', (chunk) => {
                    data += chunk;
                });

                res.on('end', () => {
                    try {
                        if (res.statusCode === 200) {
                            resolve(JSON.parse(data));
                        } else {
                            reject(new Error(`HTTP ${res.statusCode}: ${data}`));
                        }
                    } catch (error) {
                        reject(error);
                    }
                });
            });

            req.on('error', (error) => {
                reject(error);
            });

            req.setTimeout(10000, () => {
                req.destroy();
                reject(new Error('Request timeout'));
            });

            req.end();
        });
    }

    // Normalize version string (remove 'v' prefix, etc.)
    normalizeVersion(version) {
        return version.replace(/^v/, '').trim();
    }

    // Compare two version strings (returns 1 if v1 > v2, -1 if v1 < v2, 0 if equal)
    compareVersions(v1, v2) {
        const parts1 = v1.split('.').map(Number);
        const parts2 = v2.split('.').map(Number);

        for (let i = 0; i < Math.max(parts1.length, parts2.length); i++) {
            const part1 = parts1[i] || 0;
            const part2 = parts2[i] || 0;

            if (part1 > part2) return 1;
            if (part1 < part2) return -1;
        }

        return 0;
    }

    // Detect installation type
    detectInstallationType() {
        const platform = process.platform;
        const execPath = app.getPath('exe');
        const isPackaged = app.isPackaged;

        // Development mode
        if (!isPackaged) {
            return { type: 'development', platform };
        }

        // AppImage detection
        if (platform === 'linux' && process.env.APPIMAGE) {
            return { 
                type: 'appimage', 
                platform,
                appImagePath: process.env.APPIMAGE
            };
        }

        // Windows installer
        if (platform === 'win32') {
            // Check if installed in Program Files
            const isProgramFiles = execPath.toLowerCase().includes('program files');
            return {
                type: isProgramFiles ? 'windows-installed' : 'windows-portable',
                platform,
                execPath
            };
        }

        // macOS DMG
        if (platform === 'darwin') {
            return {
                type: 'macos',
                platform,
                execPath
            };
        }

        // Linux portable (extracted files)
        if (platform === 'linux') {
            return {
                type: 'linux-portable',
                platform,
                execPath
            };
        }

        return { type: 'unknown', platform, execPath };
    }

    // Get appropriate download asset for current platform
    getAssetForPlatform(assets) {
        const installation = this.detectInstallationType();
        const platform = installation.platform;
        const type = installation.type;

        let assetPattern;

        if (type === 'appimage') {
            assetPattern = /\.AppImage$/i;
        } else if (type === 'windows-installed' || type === 'windows-portable') {
            assetPattern = /\.exe$/i; // NSIS installer
        } else if (type === 'macos') {
            assetPattern = /\.dmg$/i;
        } else if (type === 'linux-portable') {
            assetPattern = /\.tar\.gz$/i;
        } else {
            return null;
        }

        return assets.find(asset => assetPattern.test(asset.name));
    }

    // Download file with progress tracking
    async downloadFile(url, destination) {
        return new Promise((resolve, reject) => {
            const file = fsSync.createWriteStream(destination);
            
            https.get(url, {
                headers: { 'User-Agent': 'FFcut-Updater' }
            }, (response) => {
                if (response.statusCode === 302 || response.statusCode === 301) {
                    // Follow redirect
                    https.get(response.headers.location, {
                        headers: { 'User-Agent': 'FFcut-Updater' }
                    }, (redirectResponse) => {
                        const totalSize = parseInt(redirectResponse.headers['content-length'], 10);
                        let downloadedSize = 0;

                        redirectResponse.on('data', (chunk) => {
                            downloadedSize += chunk.length;
                            this.downloadProgress = (downloadedSize / totalSize) * 100;
                            
                            this.notifyRenderer('update-download-progress', {
                                progress: Math.round(this.downloadProgress),
                                downloaded: this.formatBytes(downloadedSize),
                                total: this.formatBytes(totalSize)
                            });
                        });

                        redirectResponse.pipe(file);

                        file.on('finish', () => {
                            file.close();
                            resolve(destination);
                        });
                    }).on('error', (err) => {
                        fsSync.unlink(destination, () => {});
                        reject(err);
                    });
                } else {
                    const totalSize = parseInt(response.headers['content-length'], 10);
                    let downloadedSize = 0;

                    response.on('data', (chunk) => {
                        downloadedSize += chunk.length;
                        this.downloadProgress = (downloadedSize / totalSize) * 100;
                        
                        this.notifyRenderer('update-download-progress', {
                            progress: Math.round(this.downloadProgress),
                            downloaded: this.formatBytes(downloadedSize),
                            total: this.formatBytes(totalSize)
                        });
                    });

                    response.pipe(file);

                    file.on('finish', () => {
                        file.close();
                        resolve(destination);
                    });
                }
            }).on('error', (err) => {
                fsSync.unlink(destination, () => {});
                reject(err);
            });

            file.on('error', (err) => {
                fsSync.unlink(destination, () => {});
                reject(err);
            });
        });
    }

    // Format bytes to human readable
    formatBytes(bytes) {
        if (bytes === 0) return '0 Bytes';
        const k = 1024;
        const sizes = ['Bytes', 'KB', 'MB', 'GB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
    }

    // Install update based on platform
    async installUpdate(downloadedFile) {
        const installation = this.detectInstallationType();
        
        this.notifyRenderer('update-install-status', {
            status: 'installing',
            message: 'Installing update...'
        });

        try {
            if (installation.type === 'appimage') {
                await this.installAppImageUpdate(downloadedFile, installation.appImagePath);
            } else if (installation.type === 'windows-installed') {
                await this.installWindowsInstalledUpdate(downloadedFile);
            } else if (installation.type === 'windows-portable') {
                await this.installWindowsPortableUpdate(downloadedFile, installation.execPath);
            } else if (installation.type === 'macos') {
                await this.installMacOSUpdate(downloadedFile);
            } else if (installation.type === 'linux-portable') {
                await this.installLinuxPortableUpdate(downloadedFile, installation.execPath);
            } else {
                throw new Error('Unsupported installation type');
            }

            this.notifyRenderer('update-install-status', {
                status: 'completed',
                message: 'Update installed successfully! Restart to apply.'
            });

            // Show notification
            if (Notification.isSupported()) {
                new Notification({
                    title: 'Update Installed',
                    body: 'FFcut has been updated successfully. Please restart the application.',
                    icon: path.join(__dirname, 'assets', 'icon.png')
                }).show();
            }

            return true;
        } catch (error) {
            console.error('Installation failed:', error);
            this.notifyRenderer('update-install-status', {
                status: 'error',
                message: `Installation failed: ${error.message}`
            });
            throw error;
        }
    }

    // Install AppImage update (fixed ETXTBSY issue)
    async installAppImageUpdate(newAppImage, currentAppImage) {
        // Make new AppImage executable
        await fs.chmod(newAppImage, 0o755);

        // Create update script that will run after app closes
        const updateScript = path.join(os.tmpdir(), 'ffcut-update.sh');
        const scriptContent = `#!/bin/bash
# Wait for old process to exit
sleep 2

# Backup old AppImage
cp "${currentAppImage}" "${currentAppImage}.backup"

# Replace with new version
mv "${newAppImage}" "${currentAppImage}"
chmod +x "${currentAppImage}"

# Start new version
"${currentAppImage}" &

# Clean up
rm "${currentAppImage}.backup" 2>/dev/null
rm "$0" 2>/dev/null
`;

        await fs.writeFile(updateScript, scriptContent);
        await fs.chmod(updateScript, 0o755);

        // Launch update script in detached mode
        spawn('bash', [updateScript], {
            detached: true,
            stdio: 'ignore'
        }).unref();

        // Give script time to start, then quit
        setTimeout(() => {
            app.quit();
        }, 500);
    }

    // Install Windows installed version update
    async installWindowsInstalledUpdate(installerPath) {
        // Run NSIS installer silently
        spawn(installerPath, ['/S'], {
            detached: true,
            stdio: 'ignore'
        }).unref();

        // Wait a moment then quit
        setTimeout(() => {
            app.quit();
        }, 1000);
    }

    // Install Windows portable update (fixed implementation)
    async installWindowsPortableUpdate(installerPath, currentExecPath) {
        // For Windows portable, we create a batch script to:
        // 1. Wait for app to close
        // 2. Run installer to temp location
        // 3. Copy files to current location
        // 4. Restart app
        
        const updateBat = path.join(os.tmpdir(), 'ffcut-update.bat');
        const currentDir = path.dirname(currentExecPath);
        const tempInstallDir = path.join(os.tmpdir(), `ffcut-update-${Date.now()}`);
        
        const batContent = `@echo off
REM Wait for app to close
timeout /t 2 /nobreak > nul

REM Run installer to temp directory
"${installerPath}" /S /D=${tempInstallDir}

REM Wait for installer to finish
timeout /t 3 /nobreak > nul

REM Copy files from temp to current directory
xcopy /E /I /Y "${tempInstallDir}\\*" "${currentDir}\\"

REM Start new version
start "" "${currentExecPath}"

REM Clean up
timeout /t 2 /nobreak > nul
rmdir /S /Q "${tempInstallDir}"
del "${installerPath}"
del "%~f0"
`;

        await fs.writeFile(updateBat, batContent);

        // Launch batch script
        spawn('cmd.exe', ['/c', updateBat], {
            detached: true,
            stdio: 'ignore',
            windowsHide: true
        }).unref();

        // Wait then quit
        setTimeout(() => {
            app.quit();
        }, 1000);
    }

    // Install macOS update
    async installMacOSUpdate(dmgPath) {
        // macOS updates typically require manual installation
        // We can open the DMG for the user
        const { shell } = require('electron');
        
        this.notifyRenderer('update-install-status', {
            status: 'manual',
            message: 'Please drag the new version to Applications folder to complete update.'
        });

        await shell.openPath(dmgPath);
    }

    // Install Linux portable update (using external script approach)
    async installLinuxPortableUpdate(tarGzPath, currentExecPath) {
        const installDir = path.dirname(currentExecPath);
        const execName = path.basename(currentExecPath);
        
        console.log('Linux Portable Update Debug:');
        console.log('  Current exec path:', currentExecPath);
        console.log('  Install directory:', installDir);
        console.log('  Tar.gz path:', tarGzPath);
        
        // Create temp extraction directory
        const tempExtractDir = path.join(os.tmpdir(), `ffcut-update-${Date.now()}`);
        await fs.mkdir(tempExtractDir, { recursive: true });

        try {
            // Extract tarball
            console.log('  Extracting to:', tempExtractDir);
            await execAsync(`tar -xzf "${tarGzPath}" -C "${tempExtractDir}"`);

            // Find extracted directory
            const extracted = await fs.readdir(tempExtractDir);
            if (extracted.length === 0) {
                throw new Error('Extraction failed - no files found');
            }
            
            console.log('  Extracted content:', extracted);
            console.log('  Extracted content count:', extracted.length);
            
            // Determine the correct app directory
            let extractedAppDir;
            
            // Check structure
            for (const item of extracted) {
                const itemPath = path.join(tempExtractDir, item);
                const stats = await fs.stat(itemPath);
                console.log(`  - ${item}: ${stats.isDirectory() ? 'DIR' : 'FILE'}`);
            }
            
            if (extracted.length === 1) {
                const singleItem = path.join(tempExtractDir, extracted[0]);
                const stats = await fs.stat(singleItem);
                
                if (stats.isDirectory()) {
                    const resourcesInSingleDir = path.join(singleItem, 'resources');
                    if (fsSync.existsSync(resourcesInSingleDir)) {
                        extractedAppDir = singleItem;
                        console.log('  Structure: single directory with app inside');
                    } else {
                        extractedAppDir = tempExtractDir;
                        console.log('  Structure: single dir but resources at root');
                    }
                } else {
                    throw new Error('Unexpected structure - single file at root');
                }
            } else {
                if (extracted.includes('resources')) {
                    extractedAppDir = tempExtractDir;
                    console.log('  Structure: files extracted directly to root');
                } else {
                    throw new Error('Unexpected structure - multiple items but no resources');
                }
            }
            
            console.log('  Using app dir:', extractedAppDir);
            
            // Verify structure
            const resourcesPath = path.join(extractedAppDir, 'resources');
            const appAsarPath = path.join(resourcesPath, 'app.asar');
            
            console.log('  Looking for resources at:', resourcesPath);
            if (!fsSync.existsSync(resourcesPath)) {
                console.log('  ERROR: resources not found!');
                throw new Error(`Invalid package - resources not found at ${resourcesPath}`);
            }
            
            console.log('  Looking for app.asar at:', appAsarPath);
            if (!fsSync.existsSync(appAsarPath)) {
                console.log('  ERROR: app.asar not found!');
                throw new Error(`Invalid package - app.asar not found at ${appAsarPath}`);
            }
            
            console.log('  ✓ Package verified');
            
            // Create update script that will run after app closes
            const updateScript = path.join(os.tmpdir(), 'ffcut-update-portable.sh');
            const backupDir = installDir + '.backup';
            
            const scriptContent = `#!/bin/bash
# FFcut portable update script
set -e

echo "FFcut Update Script Started"
echo "Waiting for application to close..."

# Wait for process to exit
sleep 3

# Backup current installation
echo "Creating backup..."
if [ -d "${backupDir}" ]; then
    rm -rf "${backupDir}"
fi
cp -r "${installDir}" "${backupDir}"
echo "Backup created at: ${backupDir}"

# Remove old files (but keep directory)
echo "Removing old files..."
cd "${installDir}"
rm -rf *
echo "Old files removed"

# Copy new files
echo "Installing new version..."
cp -r "${extractedAppDir}"/* "${installDir}/"
echo "New files copied"

# Set executable permissions
chmod +x "${installDir}/${execName}"
echo "Permissions set"

# Verify installation
if [ ! -f "${installDir}/resources/app.asar" ]; then
    echo "ERROR: Installation verification failed!"
    echo "Restoring backup..."
    rm -rf "${installDir}"/*
    cp -r "${backupDir}"/* "${installDir}/"
    chmod +x "${installDir}/${execName}"
    echo "Backup restored"
    exit 1
fi

echo "Installation verified"

# Clean up
echo "Cleaning up..."
rm -rf "${tempExtractDir}"
rm -rf "${backupDir}"
rm -f "${tarGzPath}"
echo "Cleanup completed"

# Start new version
echo "Starting new version..."
cd "${installDir}"
./${execName} &

# Remove this script
rm -f "$0"

echo "Update completed successfully!"
`;

            await fs.writeFile(updateScript, scriptContent);
            await fs.chmod(updateScript, 0o755);
            
            console.log('  Update script created at:', updateScript);
            console.log('  Update script will run after app closes');

            // Launch update script in detached mode
            spawn('bash', [updateScript], {
                detached: true,
                stdio: 'ignore'
            }).unref();

            // Notify and quit
            this.notifyRenderer('update-install-status', {
                status: 'restarting',
                message: 'Update ready. Application will restart...'
            });

            // Give script time to start, then quit
            setTimeout(() => {
                app.quit();
            }, 1000);
            
        } catch (error) {
            // Clean up temp directory on error
            try {
                await fs.rm(tempExtractDir, { recursive: true, force: true });
            } catch (cleanupError) {
                console.error('Failed to clean up temp directory:', cleanupError);
            }
            throw error;
        }
    }

    // Helper method to recursively copy directory
    async copyDirectory(source, destination, retries = 3) {
        await fs.mkdir(destination, { recursive: true });
        
        const entries = await fs.readdir(source, { withFileTypes: true });
        
        for (const entry of entries) {
            const srcPath = path.join(source, entry.name);
            const destPath = path.join(destination, entry.name);
            
            if (entry.isDirectory()) {
                await this.copyDirectory(srcPath, destPath, retries);
            } else {
                // Try to copy file with retries for busy files
                let lastError;
                for (let attempt = 0; attempt < retries; attempt++) {
                    try {
                        await fs.copyFile(srcPath, destPath);
                        
                        // Preserve file permissions
                        try {
                            const stats = await fs.stat(srcPath);
                            await fs.chmod(destPath, stats.mode);
                        } catch (chmodError) {
                            // Ignore chmod errors, not critical
                            console.warn('Warning: could not set permissions for', destPath);
                        }
                        
                        lastError = null;
                        break; // Success
                    } catch (error) {
                        lastError = error;
                        
                        // If file doesn't exist or access denied, might be temporary
                        if (error.code === 'ENOENT' || error.code === 'EACCES' || error.code === 'EBUSY') {
                            if (attempt < retries - 1) {
                                // Wait a bit and retry
                                await new Promise(resolve => setTimeout(resolve, 100 * (attempt + 1)));
                                continue;
                            }
                        }
                        
                        // For other errors or final attempt, throw
                        throw error;
                    }
                }
                
                if (lastError) {
                    throw lastError;
                }
            }
        }
    }

    // Main update flow
    async performUpdate() {
        if (!this.latestRelease) {
            throw new Error('No update available');
        }

        const installation = this.detectInstallationType();

        if (installation.type === 'development') {
            throw new Error('Cannot update development version');
        }

        const asset = this.getAssetForPlatform(this.latestRelease.assets);

        if (!asset) {
            throw new Error(`No compatible update found for ${installation.type} on ${installation.platform}`);
        }

        this.notifyRenderer('update-download-status', {
            status: 'downloading',
            message: `Downloading ${asset.name}...`
        });

        // Download update
        const downloadPath = path.join(os.tmpdir(), asset.name);
        
        try {
            await this.downloadFile(asset.browser_download_url, downloadPath);

            this.notifyRenderer('update-download-status', {
                status: 'completed',
                message: 'Download completed!'
            });

            // Install update
            await this.installUpdate(downloadPath);

            return true;
        } catch (error) {
            // Clean up on error
            try {
                if (fsSync.existsSync(downloadPath)) {
                    await fs.unlink(downloadPath);
                }
            } catch (cleanupError) {
                console.error('Failed to clean up download:', cleanupError);
            }

            throw error;
        }
    }

    // Get update info
    getUpdateInfo() {
        return {
            updateAvailable: this.updateAvailable,
            currentVersion: this.currentVersion,
            latestVersion: this.latestRelease?.tag_name || null,
            releaseNotes: this.latestRelease?.body || null,
            installationType: this.detectInstallationType()
        };
    }
}

module.exports = Updater;
