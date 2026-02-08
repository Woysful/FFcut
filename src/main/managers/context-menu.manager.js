const { app } = require('electron');
const fs = require('fs').promises;
const fsSync = require('fs');
const path = require('path');
const os = require('os');
const { exec } = require('child_process');
const util = require('util');

const execAsync = util.promisify(exec);

class ContextMenuManager {
    constructor() {
        this.platform = process.platform;
        this.appPath = this.getRealExecutablePath();
        this.appName = 'FFcut';
    }

    /**
     * Get the real executable path, handling AppImage and other edge cases
     */
    getRealExecutablePath() {
        const execPath = app.getPath('exe');
        
        // Check if running as AppImage
        // AppImage sets APPIMAGE env var with the real path
        if (process.env.APPIMAGE && fsSync.existsSync(process.env.APPIMAGE)) {
            console.log('Detected AppImage, using APPIMAGE env var:', process.env.APPIMAGE);
            return process.env.APPIMAGE;
        }
        
        // If execPath contains /tmp/.mount_, it's an AppImage but APPIMAGE var is missing
        // This shouldn't happen in normal circumstances, but handle it gracefully
        if (execPath.includes('/tmp/.mount_')) {
            console.warn('AppImage detected but APPIMAGE env var not set. Path may be incorrect.');
            console.warn('Executable will point to temporary mount:', execPath);
        }
        
        return execPath;
    }

    /**
     * Check if the stored executable path in desktop files needs updating
     * Returns true if update is needed
     */
    async needsPathUpdate() {
        if (!await this.isEnabled()) {
            return false;
        }

        const currentPath = this.getRealExecutablePath();
        
        switch (this.platform) {
            case 'linux':
                return await this.needsPathUpdateLinux(currentPath);
            case 'win32':
                return await this.needsPathUpdateWindows(currentPath);
            case 'darwin':
                return await this.needsPathUpdateMacOS(currentPath);
            default:
                return false;
        }
    }

    /**
     * Update the executable path in existing context menu entries
     */
    async updatePath() {
        if (!await this.isEnabled()) {
            console.log('Context menu not enabled, skipping path update');
            return { success: true, updated: false };
        }

        console.log('Updating context menu executable path to:', this.appPath);
        
        // Simply re-enable to update paths
        try {
            await this.enable();
            return { success: true, updated: true };
        } catch (err) {
            console.error('Failed to update context menu path:', err);
            return { success: false, error: err.message };
        }
    }

    /**
     * Check if context menu integration is enabled
     */
    async isEnabled() {
        try {
            switch (this.platform) {
                case 'win32':
                    return await this.isEnabledWindows();
                case 'linux':
                    return await this.isEnabledLinux();
                case 'darwin':
                    return await this.isEnabledMacOS();
                default:
                    return false;
            }
        } catch (err) {
            console.error('Error checking context menu status:', err);
            return false;
        }
    }

    /**
     * Enable context menu integration
     */
    async enable() {
        try {
            switch (this.platform) {
                case 'win32':
                    return await this.enableWindows();
                case 'linux':
                    return await this.enableLinux();
                case 'darwin':
                    return await this.enableMacOS();
                default:
                    throw new Error('Unsupported platform');
            }
        } catch (err) {
            console.error('Error enabling context menu:', err);
            throw err;
        }
    }

    /**
     * Disable context menu integration
     */
    async disable() {
        try {
            switch (this.platform) {
                case 'win32':
                    return await this.disableWindows();
                case 'linux':
                    return await this.disableLinux();
                case 'darwin':
                    return await this.disableMacOS();
                default:
                    throw new Error('Unsupported platform');
            }
        } catch (err) {
            console.error('Error disabling context menu:', err);
            throw err;
        }
    }

    // Windows implementation
    async isEnabledWindows() {
        try {
            const { stdout } = await execAsync(
                'reg query "HKCU\\Software\\Classes\\SystemFileAssociations\\.mp4\\shell\\FFcut" /ve',
                { encoding: 'utf8' }
            );
            return stdout.includes('Edit with FFcut');
        } catch (err) {
            return false;
        }
    }

    async enableWindows() {
        const extensions = ['.mp4', '.mov', '.avi', '.mkv', '.webm', '.flv', '.m4v', '.wmv', '.mpg', '.mpeg'];
        const exePath = this.appPath.replace(/\\/g, '\\\\');

        for (const ext of extensions) {
            const regCommands = [
                `reg add "HKCU\\Software\\Classes\\SystemFileAssociations\\${ext}\\shell\\FFcut" /ve /d "Edit with FFcut" /f`,
                `reg add "HKCU\\Software\\Classes\\SystemFileAssociations\\${ext}\\shell\\FFcut" /v "Icon" /d "${exePath}" /f`,
                `reg add "HKCU\\Software\\Classes\\SystemFileAssociations\\${ext}\\shell\\FFcut\\command" /ve /d "\\"${exePath}\\" \\"%1\\"" /f`
            ];

            for (const cmd of regCommands) {
                try {
                    await execAsync(cmd);
                } catch (err) {
                    console.error(`Failed to execute: ${cmd}`, err);
                    throw err;
                }
            }
        }

        return { success: true };
    }

    async disableWindows() {
        const extensions = ['.mp4', '.mov', '.avi', '.mkv', '.webm', '.flv', '.m4v', '.wmv', '.mpg', '.mpeg'];

        for (const ext of extensions) {
            try {
                await execAsync(`reg delete "HKCU\\Software\\Classes\\SystemFileAssociations\\${ext}\\shell\\FFcut" /f`);
            } catch (err) {
                // Ignore errors if key doesn't exist
            }
        }

        return { success: true };
    }

    async needsPathUpdateWindows(currentPath) {
        try {
            const { stdout } = await execAsync(
                'reg query "HKCU\\Software\\Classes\\SystemFileAssociations\\.mp4\\shell\\FFcut\\command" /ve',
                { encoding: 'utf8' }
            );
            
            // Extract path from registry value
            // Format: "C:\Path\To\App.exe" "%1"
            const pathMatch = stdout.match(/"([^"]+)"/);
            if (pathMatch && pathMatch[1].replace(/\\\\/g, '\\') !== currentPath.replace(/\\\\/g, '\\')) {
                console.log('Path mismatch detected:', pathMatch[1], '!=', currentPath);
                return true;
            }
            
            return false;
        } catch (err) {
            return false;
        }
    }

    // Linux implementation
    async isEnabledLinux() {
        const desktopFile = path.join(os.homedir(), '.local/share/applications/ffcut-open.desktop');
        try {
            await fs.access(desktopFile);
            return true;
        } catch (err) {
            return false;
        }
    }

    async enableLinux() {
        const desktopFile = path.join(os.homedir(), '.local/share/applications/ffcut-open.desktop');
        const applicationsDir = path.dirname(desktopFile);

        // Ensure applications directory exists
        await fs.mkdir(applicationsDir, { recursive: true });

        // Detect desktop environment
        const desktopEnv = process.env.XDG_CURRENT_DESKTOP || 
                          process.env.DESKTOP_SESSION || 
                          'unknown';

        // Get and install icon
        const iconPath = await this.installIcon();

        const desktopFileContent = `[Desktop Entry]
Type=Application
Name=Edit with FFcut
Exec="${this.appPath}" %f
Icon=${iconPath}
Terminal=false
Categories=AudioVideo;Video;
MimeType=video/mp4;video/x-matroska;video/avi;video/quicktime;video/x-msvideo;video/webm;video/x-flv;video/mpeg;
NoDisplay=true
X-KDE-Protocols=file
`;

        await fs.writeFile(desktopFile, desktopFileContent, 'utf8');
        
        // Make .desktop file executable (required by KDE Plasma 6)
        await fs.chmod(desktopFile, 0o755);
        
        // Mark as trusted for KDE
        try {
            const metadataAttr = 'user.xdg.origin.url';
            await execAsync(`setfattr -n ${metadataAttr} -v "file://${desktopFile}" "${desktopFile}" 2>/dev/null || true`);
        } catch (err) {
            // Ignore if setfattr is not available
            console.warn('Could not set xdg metadata (this is OK):', err.message);
        }

        // Update MIME database
        try {
            await execAsync('update-desktop-database ~/.local/share/applications');
        } catch (err) {
            console.warn('Failed to update desktop database:', err);
        }

        // For KDE Plasma
        if (desktopEnv.toLowerCase().includes('kde') || desktopEnv.toLowerCase().includes('plasma')) {
            try {
                const serviceMenusDir = path.join(os.homedir(), '.local/share/kio/servicemenus');
                await fs.mkdir(serviceMenusDir, { recursive: true });

                const serviceMenuFile = path.join(serviceMenusDir, 'ffcut-open.desktop');
                const serviceMenuContent = `[Desktop Entry]
Type=Service
ServiceTypes=KonqPopupMenu/Plugin
MimeType=video/mp4;video/x-matroska;video/avi;video/quicktime;video/x-msvideo;video/webm;video/x-flv;video/mpeg;
Actions=OpenWithFFcut
X-KDE-Priority=TopLevel

[Desktop Action OpenWithFFcut]
Name=Edit with FFcut
Icon=${iconPath}
Exec="${this.appPath}" %f
`;
                await fs.writeFile(serviceMenuFile, serviceMenuContent, 'utf8');
                // Make service menu file executable as well
                await fs.chmod(serviceMenuFile, 0o755);
                
                // Update KDE service menu cache
                try {
                    // Try kbuildsycoca6 for Plasma 6, fallback to kbuildsycoca5 for Plasma 5
                    await execAsync('kbuildsycoca6 --noincremental 2>/dev/null || kbuildsycoca5 --noincremental 2>/dev/null || true');
                } catch (err) {
                    console.warn('Could not update KDE cache:', err.message);
                }
            } catch (err) {
                console.warn('Failed to create KDE service menu:', err);
            }
        }

        // For GNOME/Nautilus
        if (desktopEnv.toLowerCase().includes('gnome') || desktopEnv.toLowerCase().includes('ubuntu')) {
            try {
                const nautilusScriptsDir = path.join(os.homedir(), '.local/share/nautilus/scripts');
                await fs.mkdir(nautilusScriptsDir, { recursive: true });

                const scriptFile = path.join(nautilusScriptsDir, 'Edit with FFcut');
                const scriptContent = `#!/bin/bash
"${this.appPath}" "$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS"
`;
                await fs.writeFile(scriptFile, scriptContent, 'utf8');
                await fs.chmod(scriptFile, 0o755);
            } catch (err) {
                console.warn('Failed to create Nautilus script:', err);
            }
        }

        return { success: true };
    }

    /**
     * Install icon for context menu integration
     * Returns the icon path/name to use in desktop files
     */
    async installIcon() {
        const iconsDir = path.join(os.homedir(), '.local/share/icons/hicolor/256x256/apps');
        const installedIconPath = path.join(iconsDir, 'ffcut.png');
        
        // Try to find the source icon
        let sourceIconPath = null;
        const possibleIconPaths = [
            // For AppImage
            path.join(path.dirname(this.appPath), '.DirIcon'),
            // For packaged apps
            path.join(path.dirname(this.appPath), 'resources', 'app', 'assets', 'icon.png'),
            path.join(path.dirname(this.appPath), 'resources', 'app.asar.unpacked', 'assets', 'icon.png'),
            path.join(path.dirname(this.appPath), 'assets', 'icon.png'),
            // For development
            path.join(__dirname, 'assets', 'icon.png'),
            path.join(__dirname, '..', 'assets', 'icon.png')
        ];

        // For AppImage, try to extract icon
        if (process.env.APPIMAGE && this.appPath.endsWith('.AppImage')) {
            try {
                // AppImage usually has .DirIcon in the same directory when mounted
                const appImageDir = path.dirname(this.appPath);
                const dirIcon = path.join(appImageDir, '.DirIcon');
                
                // Try to extract icon using AppImage's built-in icon
                if (fsSync.existsSync(dirIcon)) {
                    possibleIconPaths.unshift(dirIcon);
                }
                
                // Also check if AppImage has icon embedded
                const { stdout } = await execAsync(`"${this.appPath}" --appimage-extract-and-run echo 2>/dev/null || echo ""`).catch(() => ({ stdout: '' }));
            } catch (err) {
                console.warn('Could not extract AppImage icon:', err.message);
            }
        }

        // Find existing icon
        for (const iconCheck of possibleIconPaths) {
            if (fsSync.existsSync(iconCheck)) {
                sourceIconPath = iconCheck;
                console.log('Found icon at:', iconCheck);
                break;
            }
        }

        // If we found a source icon, copy it to the icons directory
        if (sourceIconPath) {
            try {
                await fs.mkdir(iconsDir, { recursive: true });
                await fs.copyFile(sourceIconPath, installedIconPath);
                await fs.chmod(installedIconPath, 0o644);
                
                // Update icon cache
                try {
                    await execAsync('gtk-update-icon-cache ~/.local/share/icons/hicolor/ 2>/dev/null || true');
                    await execAsync('xdg-icon-resource forceupdate 2>/dev/null || true');
                } catch (err) {
                    console.warn('Could not update icon cache:', err.message);
                }
                
                console.log('Icon installed to:', installedIconPath);
                // Return just the icon name for better theme integration
                return 'ffcut';
            } catch (err) {
                console.warn('Failed to install icon:', err);
            }
        }

        // Check if icon is already installed from a previous run
        if (fsSync.existsSync(installedIconPath)) {
            console.log('Using previously installed icon');
            return 'ffcut';
        }

        // Fallback to generic video icon
        console.warn('No icon found, using fallback');
        return 'video-x-generic';
    }

    async disableLinux() {
        // Remove .desktop file
        const desktopFile = path.join(os.homedir(), '.local/share/applications/ffcut-open.desktop');
        try {
            await fs.unlink(desktopFile);
        } catch (err) {
            // Ignore if file doesn't exist
        }

        // Remove KDE service menu
        const serviceMenuFile = path.join(os.homedir(), '.local/share/kio/servicemenus/ffcut-open.desktop');
        try {
            await fs.unlink(serviceMenuFile);
        } catch (err) {
            // Ignore if file doesn't exist
        }

        // Remove Nautilus script
        const scriptFile = path.join(os.homedir(), '.local/share/nautilus/scripts/Edit with FFcut');
        try {
            await fs.unlink(scriptFile);
        } catch (err) {
            // Ignore if file doesn't exist
        }

        // Remove installed icon
        const installedIconPath = path.join(os.homedir(), '.local/share/icons/hicolor/256x256/apps/ffcut.png');
        try {
            await fs.unlink(installedIconPath);
            // Update icon cache after removing
            await execAsync('gtk-update-icon-cache ~/.local/share/icons/hicolor/ 2>/dev/null || true');
            await execAsync('xdg-icon-resource forceupdate 2>/dev/null || true');
        } catch (err) {
            // Ignore if icon doesn't exist
        }

        // Update MIME database
        try {
            await execAsync('update-desktop-database ~/.local/share/applications');
        } catch (err) {
            console.warn('Failed to update desktop database:', err);
        }

        // Update KDE cache
        try {
            await execAsync('kbuildsycoca6 --noincremental 2>/dev/null || kbuildsycoca5 --noincremental 2>/dev/null || true');
        } catch (err) {
            // Ignore
        }

        return { success: true };
    }

    async needsPathUpdateLinux(currentPath) {
        const desktopFile = path.join(os.homedir(), '.local/share/applications/ffcut-open.desktop');
        
        try {
            const content = await fs.readFile(desktopFile, 'utf8');
            
            // Extract Exec line
            const execMatch = content.match(/^Exec="?([^"\n]+)"?\s+%f/m);
            if (execMatch && execMatch[1] !== currentPath) {
                console.log('Path mismatch detected:', execMatch[1], '!=', currentPath);
                return true;
            }
            
            return false;
        } catch (err) {
            // File doesn't exist or can't be read
            return false;
        }
    }

    // macOS implementation
    async isEnabledMacOS() {
        // On macOS, we check if our app is registered as a handler for video files
        // This is more complex and typically handled through Info.plist
        // For now, we'll use a marker file approach
        const markerFile = path.join(app.getPath('userData'), '.context-menu-enabled');
        try {
            await fs.access(markerFile);
            return true;
        } catch (err) {
            return false;
        }
    }

    async enableMacOS() {
        // On macOS, context menu integration is typically done through:
        // 1. Registering file type associations in Info.plist (done at build time)
        // 2. Using Services or Quick Actions (requires Automator workflows)
        
        // For a runtime solution, we'll create a marker file and provide instructions
        const markerFile = path.join(app.getPath('userData'), '.context-menu-enabled');
        
        // Create marker file
        await fs.writeFile(markerFile, JSON.stringify({
            enabled: true,
            timestamp: new Date().toISOString()
        }), 'utf8');

        // On macOS, the app needs to be properly built with CFBundleDocumentTypes
        // in Info.plist to appear in "Open With" menu
        // This is typically done during the build process with electron-builder
        
        return { 
            success: true, 
            note: 'macOS integration works through "Open With" menu. Make sure the app is built with proper file associations.'
        };
    }

    async disableMacOS() {
        const markerFile = path.join(app.getPath('userData'), '.context-menu-enabled');
        try {
            await fs.unlink(markerFile);
        } catch (err) {
            // Ignore if file doesn't exist
        }

        return { success: true };
    }

    async needsPathUpdateMacOS(currentPath) {
        // On macOS, path is embedded in app bundle, so it doesn't change
        // unless the entire app is moved, which is handled by the OS
        return false;
    }

    /**
     * Get platform-specific information about context menu integration
     */
    getPlatformInfo() {
        switch (this.platform) {
            case 'win32':
                return {
                    platform: 'Windows',
                    description: 'Adds "Edit with FFcut" to the context menu for video files',
                    requiresAdmin: false
                };
            case 'linux':
                return {
                    platform: 'Linux',
                    description: 'Registers FFcut in file manager context menus (supports KDE, GNOME, and others)',
                    requiresAdmin: false
                };
            case 'darwin':
                return {
                    platform: 'macOS',
                    description: 'FFcut will appear in "Open With" menu for video files',
                    requiresAdmin: false
                };
            default:
                return {
                    platform: 'Unknown',
                    description: 'Context menu integration not supported',
                    requiresAdmin: false
                };
        }
    }
}

module.exports = ContextMenuManager;
