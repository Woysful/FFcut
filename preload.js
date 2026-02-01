const { contextBridge, ipcRenderer, webUtils } = require('electron');
const path = require('path');

// Expose protected methods that allow the renderer process to use
// the ipcRenderer without exposing the entire object
contextBridge.exposeInMainWorld('electronAPI', {
  // Video operations
  selectVideo: () => ipcRenderer.invoke('select-video'),
  getVideoInfo: (filePath) => ipcRenderer.invoke('get-video-info', filePath),
  prepareMultiAudioPreview: (filePath, metadata) =>
    ipcRenderer.invoke('prepare-multi-audio-preview', filePath, metadata),
  cancelTranscode: () => ipcRenderer.invoke('cancel-transcode'),
  
  // Export operations
  exportVideo: (options) => ipcRenderer.invoke('export-video', options),
  cancelExport: () => ipcRenderer.invoke('cancel-export'),
  selectOutput: (defaultName) => ipcRenderer.invoke('select-output', defaultName),
  
  // Preset management
  loadPresets: () => ipcRenderer.invoke('load-presets'),
  savePreset: (name, settings) => ipcRenderer.invoke('save-preset', { name, settings }),
  deletePreset: (name) => ipcRenderer.invoke('delete-preset', name),
  renamePreset: (oldName, newName) => ipcRenderer.invoke('rename-preset', { oldName, newName }),
  
  // File operations - use webUtils.getPathForFile for drag-and-drop
  getFilePathFromFile: (file) => {
    try {
      // webUtils.getPathForFile is the official way to get file path with contextIsolation
      return webUtils.getPathForFile(file);
    } catch (error) {
      console.error('Error getting file path:', error);
      return null;
    }
  },
  
  // Event listeners
  onTranscodeProgress: (callback) => {
    ipcRenderer.on('transcode-progress', (event, data) => callback(data));
  },
  onExportProgress: (callback) => {
    ipcRenderer.on('export-progress', (event, data) => callback(data));
  },
  
  // Remove listeners
  removeTranscodeProgressListener: () => {
    ipcRenderer.removeAllListeners('transcode-progress');
  },
  removeExportProgressListener: () => {
    ipcRenderer.removeAllListeners('export-progress');
  },
  
  // Path utilities
  path: {
    basename: (filePath, ext) => path.basename(filePath, ext),
    dirname: (filePath) => path.dirname(filePath),
    extname: (filePath) => path.extname(filePath),
    join: (...paths) => path.join(...paths)
  }
});
