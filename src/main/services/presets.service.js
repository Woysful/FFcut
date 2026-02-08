const fs = require('fs').promises;
const path = require('path');

/**
 * Presets service for managing export presets
 * Handles loading, saving, deleting, and renaming presets
 */
class PresetsService {
    constructor(presetsFilePath) {
        this.presetsFilePath = presetsFilePath;
    }

    /**
     * Load presets from file
     */
    async load() {
        try {
            const data = await fs.readFile(this.presetsFilePath, 'utf8');
            return JSON.parse(data);
        } catch (err) {
            if (err.code === 'ENOENT') {
                return {};
            }
            throw err;
        }
    }

    /**
     * Save presets to file
     */
    async save(presets) {
        await fs.writeFile(this.presetsFilePath, JSON.stringify(presets, null, 2), 'utf8');
    }

    /**
     * Save a new preset
     */
    async savePreset(name, settings) {
        try {
            const presets = await this.load();
            presets[name] = settings;
            await this.save(presets);
            return { success: true };
        } catch (err) {
            console.error('Error saving preset:', err);
            return { success: false, error: err.message };
        }
    }

    /**
     * Delete a preset
     */
    async deletePreset(name) {
        try {
            const presets = await this.load();
            delete presets[name];
            await this.save(presets);
            return { success: true };
        } catch (err) {
            console.error('Error deleting preset:', err);
            return { success: false, error: err.message };
        }
    }

    /**
     * Rename a preset
     */
    async renamePreset(oldName, newName) {
        try {
            const presets = await this.load();
            
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
            
            await this.save(presets);
            return { success: true };
        } catch (err) {
            console.error('Error renaming preset:', err);
            return { success: false, error: err.message };
        }
    }
}

module.exports = PresetsService;
