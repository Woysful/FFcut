// State
let state = {
    videoPath: null,
    videoFilename: null,
    videoElement: null,
    videoMetadata: null,
    audioStreams: [],
    selectedAudioTrack: 0,
    previewVideoPath: null,
    previewAudioPaths: null,
    trimStart: 0,
    trimEnd: 0,
    transformEnabled: false,
    cropEnabled: false,
    crop: {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        originalWidth: 0,
        originalHeight: 0
    },
    rotation: 0, // 0, 90, 180, 270
    flipHorizontal: false,
    flipVertical: false,
    scale: {
        width: -1,  // -1 means auto/original
        height: -1
    },
    isPlaying: false,
    isLooping: false,
    isTimelineLocked: false,
    isScrubbing: false,
    lastTimeInTrimRegion: false,
    wasAudioPlayingBeforeSeek: false,
    isDraggingTrim: null,
    isDraggingCrop: null,
    needsPreview: false,
    exportSettings: {
        videoCodec: 'copy',
        audioCodec: 'copy',
        audioTrack: 'all', // Changed default from '0' to 'all'
        selectedAudioTracks: [], // New: array of selected track indices for pick-export/pick-merge
        qualityMode: 'crf',
        crf: 23,
        videoBitrate: 5000,
        audioBitrate: 192,
        preset: 'medium',
        pixelFormat: 'auto',
        container: 'mp4',
        customCommand: ''
    }
};

// Codec pixel format support mapping
const codecPixelFormatSupport = {
    'libx264': ['yuv420p','yuvj420p','yuv422p','yuvj422p','yuv444p','yuvj444p','nv12','nv16','nv21','yuv420p10le','yuv422p10le','yuv444p10le'],
    'h264_nvenc': ['yuv420p','yuv444p','yuv420p10le','yuv444p10le'], 'h264_qsv': ['yuv420p','nv12'], 'h264_amf': ['yuv420p','nv12'], 'h264_vaapi': ['yuv420p','nv12'],
    'libx265': ['yuv420p','yuvj420p','yuv422p','yuvj422p','yuv444p','yuvj444p','gbrp','yuv420p10le','yuv422p10le','yuv444p10le','gbrp10le','yuv420p12le','yuv422p12le','yuv444p12le','gbrp12le','gray','gray10le','gray12le'],
    'hevc_nvenc': ['yuv420p','nv12','p010le','yuv444p','p016le','yuv444p16le'], 'hevc_qsv': ['yuv420p','yuv420p10le','nv12'], 'hevc_amf': ['yuv420p','yuv420p10le','nv12'], 'hevc_vaapi': ['yuv420p','yuv420p10le','nv12'],
    'libvpx': ['yuv420p'], 'libvpx-vp9': ['yuv420p','yuva420p','yuv422p','yuv440p','yuv444p','yuv420p10le','yuv422p10le','yuv440p10le','yuv444p10le','yuv420p12le','yuv422p12le','yuv440p12le','yuv444p12le','gbrp','gbrp10le','gbrp12le'],
    'vp8_vaapi': ['yuv420p','nv12'], 'vp9_vaapi': ['yuv420p','nv12'], 'vp9_qsv': ['yuv420p','yuv420p10le'],
    'libaom-av1': ['yuv420p','yuv422p','yuv444p','yuv420p10le','yuv422p10le','yuv444p10le'], 'libsvtav1': ['yuv420p','yuv420p10le'],
    'av1_nvenc': ['yuv420p','yuv420p10le'], 'av1_qsv': ['yuv420p','yuv420p10le'], 'av1_amf': ['yuv420p','yuv420p10le'], 'av1_vaapi': ['yuv420p','yuv420p10le'],
    'prores': ['yuv422p10le','yuv444p10le'], 'prores_ks': ['yuv422p10le','yuv444p10le'],
    'mpeg4': ['yuv420p'], 'mpeg2video': ['yuv420p','yuv422p'], 'mpeg2_vaapi': ['yuv420p','nv12'], 'libtheora': ['yuv420p','yuv422p','yuv444p'], 'dnxhd': ['yuv422p'],
    'huffyuv': ['yuv422p','yuv420p','yuv444p','rgb24','bgr24'],
    'ffv1': ['yuv420p','yuv422p','yuv444p','yuv420p10le','yuv422p10le','yuv444p10le','yuv420p12le','yuv422p12le','yuv444p12le','yuv420p16le','yuv422p16le','yuv444p16le','rgb24','bgr24','gbrp','gbrp10le','gbrp12le','gbrp16le']
};

// Pixel format display names and grouping
const pixelFormatInfo = {
    'yuv420p': { name: 'yuv420p (8-bit 4:2:0)', group: '420-8bit' },
    'yuvj420p': { name: 'yuvj420p (8-bit 4:2:0 Full Range)', group: '420-8bit' },
    'yuva420p': { name: 'yuva420p (8-bit 4:2:0 with Alpha)', group: '420-8bit' },
    'yuv422p': { name: 'yuv422p (8-bit 4:2:2)', group: '422-8bit' },
    'yuvj422p': { name: 'yuvj422p (8-bit 4:2:2 Full Range)', group: '422-8bit' },
    'yuv440p': { name: 'yuv440p (8-bit 4:4:0)', group: '440-8bit' },
    'yuv444p': { name: 'yuv444p (8-bit 4:4:4)', group: '444-8bit' },
    'yuvj444p': { name: 'yuvj444p (8-bit 4:4:4 Full Range)', group: '444-8bit' },
    'yuv420p10le': { name: 'yuv420p10le (10-bit 4:2:0)', group: '420-10bit' },
    'yuv422p10le': { name: 'yuv422p10le (10-bit 4:2:2)', group: '422-10bit' },
    'yuv440p10le': { name: 'yuv440p10le (10-bit 4:4:0)', group: '440-10bit' },
    'yuv444p10le': { name: 'yuv444p10le (10-bit 4:4:4)', group: '444-10bit' },
    'yuv420p12le': { name: 'yuv420p12le (12-bit 4:2:0)', group: '420-12bit' },
    'yuv422p12le': { name: 'yuv422p12le (12-bit 4:2:2)', group: '422-12bit' },
    'yuv440p12le': { name: 'yuv440p12le (12-bit 4:4:0)', group: '440-12bit' },
    'yuv444p12le': { name: 'yuv444p12le (12-bit 4:4:4)', group: '444-12bit' },
    'yuv420p16le': { name: 'yuv420p16le (16-bit 4:2:0)', group: '420-16bit' },
    'yuv422p16le': { name: 'yuv422p16le (16-bit 4:2:2)', group: '422-16bit' },
    'yuv444p16le': { name: 'yuv444p16le (16-bit 4:4:4)', group: '444-16bit' },
    'nv12': { name: 'NV12 (8-bit 4:2:0)', group: '420-8bit' },
    'nv16': { name: 'NV16 (8-bit 4:2:2)', group: '422-8bit' },
    'nv21': { name: 'NV21 (8-bit 4:2:0)', group: '420-8bit' },
    'p010le': { name: 'P010LE (10-bit 4:2:0)', group: '420-10bit' },
    'p016le': { name: 'P016LE (16-bit 4:2:0)', group: '420-16bit' },
    'gbrp': { name: 'GBRP (8-bit RGB Planar)', group: 'rgb-8bit' },
    'gbrp10le': { name: 'GBRP10LE (10-bit RGB Planar)', group: 'rgb-10bit' },
    'gbrp12le': { name: 'GBRP12LE (12-bit RGB Planar)', group: 'rgb-12bit' },
    'gbrp16le': { name: 'GBRP16LE (16-bit RGB Planar)', group: 'rgb-16bit' },
    'rgb24': { name: 'RGB24 (8-bit RGB)', group: 'rgb-8bit' },
    'bgr24': { name: 'BGR24 (8-bit BGR)', group: 'rgb-8bit' },
    'gray': { name: 'Gray (8-bit Grayscale)', group: 'gray-8bit' },
    'gray10le': { name: 'Gray10LE (10-bit Grayscale)', group: 'gray-10bit' },
    'gray12le': { name: 'Gray12LE (12-bit Grayscale)', group: 'gray-12bit' }
};

// Codec name mapping: FFmpeg decoder name -> FFmpeg encoder name
// This maps codec_name from metadata to the correct encoder to use
const codecEncoderMapping = {
    'h264':'libx264', 'avc':'libx264', 'avc1':'libx264', 'hevc':'libx265', 'h265':'libx265', 'hvc1':'libx265',
    'vp8':'libvpx', 'vp9':'libvpx-vp9', 'av1':'libsvtav1',
    'mpeg4':'mpeg4', 'msmpeg4':'mpeg4', 'msmpeg4v2':'mpeg4', 'msmpeg4v3':'mpeg4', 'mpeg2video':'mpeg2video', 'mpeg1video':'mpeg2video',
    'prores':'prores_ks', 'theora':'libtheora', 'dnxhd':'dnxhd', 'huffyuv':'huffyuv', 'ffv1':'ffv1',
    'mjpeg':'mjpeg', 'png':'png', 'qtrle':'qtrle', 'svq1':'svq1', 'wmv1':'wmv1', 'wmv2':'wmv2', 'wmv3':'wmv2'
};

// Get the correct encoder name for a given codec from metadata
function getEncoderForCodec(codecName) {
    if (!codecName) return 'libx264'; // Default fallback

    // Convert to lowercase for case-insensitive matching
    const codec = codecName.toLowerCase();

    // Check if we have a mapping for this codec
    if (codecEncoderMapping[codec]) {
        return codecEncoderMapping[codec];
    }

    // Fallback: return libx264 for unknown codecs
    console.warn(`Unknown codec "${codecName}", falling back to libx264`);
    return 'libx264';
}

// Cross-platform helpers

// file:// URL builder.
// Windows: C:\Users\... → file:///C:/Users/...
// Unix:    /home/...    → file:///home/...  (file:// + /path = three slashes)
function filePathToUrl(filePath) {
    const normalized = filePath.replace(/\\/g, '/');
    return normalized.startsWith('/')
        ? 'file://' + normalized
        : 'file:///' + normalized;
}

// Centralised cancellation-error detector.
// Linux/macOS surfaces SIGKILL; Windows says "terminated"; Electron IPC can
// time out with "reply was never sent" when the child is killed mid-flight.
function isCancellationError(error) {
    if (!error || !error.message) return false;
    const m = error.message.toLowerCase();
    return (
        m.includes('cancelled') ||
        m.includes('sigkill') ||
        m.includes('killed') ||
        m.includes('terminated') ||
        m.includes('reply was never sent')
    );
}

// Platform flags used for keyboard shortcuts and labels
const isMacOS = navigator.platform.startsWith('Mac') || navigator.userAgent.includes('Macintosh');
const modifierLabel = isMacOS ? '⌘' : 'Ctrl';

// On macOS rewrite static "Ctrl+…" menu-shortcut labels to "⌘+…"
if (isMacOS) {
    document.querySelectorAll('.menu-shortcut').forEach(span => {
        span.textContent = span.textContent.replace(/^Ctrl/, '⌘');
    });
}

// DOM Elements
const emptyState = document.getElementById('emptyState');
const dropZone = document.querySelector('.empty-state-content');
const editorState = document.getElementById('editorState');
const videoPreview = document.getElementById('videoPreview');
const previewAudio = document.getElementById('previewAudio');
const playPauseBtn = document.getElementById('playPauseBtn');
const playIcon = document.getElementById('playIcon');
const pauseIcon = document.getElementById('pauseIcon');
const frameBackBtn = document.getElementById('frameBackBtn');
const frameForwardBtn = document.getElementById('frameForwardBtn');
const lockTimelineBtn = document.getElementById('lockTimelineBtn');
const loopBtn = document.getElementById('loopBtn');
const currentTimeDisplay = document.getElementById('currentTime');
const totalTimeDisplay = document.getElementById('totalTime');
const scrubber = document.getElementById('scrubber');
const playhead = document.getElementById('playhead');
const volumeSlider = document.getElementById('volumeSlider');
const muteBtn = document.getElementById('muteBtn');
const audioTrackControl = document.getElementById('audioTrackControl');
const audioTrackSelect = document.getElementById('audioTrackSelect');
const audioExportTrackGroup = document.getElementById('audioExportTrackGroup');
const audioExportTrack = document.getElementById('audioExportTrack');
const audioTrackPickerGroup = document.getElementById('audioTrackPickerGroup');
const audioTrackPicker = document.getElementById('audioTrackPicker');
const audioTrackPickerLabel = document.getElementById('audioTrackPickerLabel');

const trimStartHandle = document.getElementById('trimStart');
const trimEndHandle = document.getElementById('trimEnd');
const trimStartInput = document.getElementById('trimStartInput');
const trimEndInput = document.getElementById('trimEndInput');
const resetTrimBtn = document.getElementById('resetTrimBtn');
const trimInfo = document.getElementById('trimInfo');

const transformToggle = document.getElementById('transformToggle');
const transformControls = document.getElementById('transformControls');
const cropToggle = document.getElementById('cropToggle');
const cropControls = document.getElementById('cropControls');
const quickCropBtn = document.getElementById('quickCropBtn');
const cropOverlay = document.getElementById('cropOverlay');
const cropX = document.getElementById('cropX');
const cropY = document.getElementById('cropY');
const cropWidth = document.getElementById('cropWidth');
const cropHeight = document.getElementById('cropHeight');

// Crop magnifier
const cropMagnifier = document.getElementById('cropMagnifier');
const cropMagnifierInfo = document.getElementById('cropMagnifierInfo');
const cropHandleCoords = document.getElementById('cropHandleCoords');
const magnifierCanvas = document.getElementById('magnifierCanvas');
const magnifierCtx = magnifierCanvas.getContext('2d');
let MAGNIFIER_SIZE = 150; // Changed from const to let - can be adjusted with Ctrl+Wheel
let magnifierZoom = 5; // Default 5x zoom, can be changed with mouse wheel
magnifierCanvas.width = MAGNIFIER_SIZE;
magnifierCanvas.height = MAGNIFIER_SIZE;

// Transform buttons
const rotateLeftBtn = document.getElementById('rotateLeftBtn');
const rotateRightBtn = document.getElementById('rotateRightBtn');
const flipHorizontalBtn = document.getElementById('flipHorizontalBtn');
const flipVerticalBtn = document.getElementById('flipVerticalBtn');

// Scale controls
const scaleWidth = document.getElementById('scaleWidth');
const scaleHeight = document.getElementById('scaleHeight');
const scalePresets = document.querySelectorAll('.preset-btn');

const videoCodec = document.getElementById('videoCodec');
const audioCodec = document.getElementById('audioCodec');
const qualityMode = document.getElementById('qualityMode');
const crfSettings = document.getElementById('crfSettings');
const bitrateSettings = document.getElementById('bitrateSettings');
const crfSlider = document.getElementById('crfSlider');
const crfValue = document.getElementById('crfValue');
const videoBitrate = document.getElementById('videoBitrate');
const audioBitrate = document.getElementById('audioBitrate');
const encoderPreset = document.getElementById('encoderPreset');
const pixelFormat = document.getElementById('pixelFormat');
const presetGroup = document.getElementById('presetGroup');
const pixelFormatGroup = document.getElementById('pixelFormatGroup');
const container = document.getElementById('container');
const ffmpegCommand = document.getElementById('ffmpegCommand');
const exportBtn = document.getElementById('exportBtn');

// Initialize volume from slider value
videoPreview.volume = volumeSlider.value / 100;
previewAudio.volume = volumeSlider.value / 100;

// Utility Functions
function formatTime(seconds) {
    if (!seconds || isNaN(seconds)) return '00:00';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
}

function formatFileSize(bytes) {
    if (!bytes || bytes === 0) return '0 B';

    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));

    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function clamp(value, min, max) {
    return Math.min(Math.max(value, min), max);
}

// Helper function to get effective video dimensions considering rotation
function getEffectiveVideoDimensions() {
    if (!state.videoMetadata || !state.videoMetadata.streams || !state.videoMetadata.streams[0]) {
        return { width: 0, height: 0 };
    }
    
    const originalWidth = state.videoMetadata.streams[0].width;
    const originalHeight = state.videoMetadata.streams[0].height;
    
    // Swap dimensions for 90 and 270 degree rotations
    if (state.rotation === 90 || state.rotation === 270) {
        return { width: originalHeight, height: originalWidth };
    }
    
    return { width: originalWidth, height: originalHeight };
}

// Convert coordinates from rotated video space to original video space
function rotatedToOriginalCoords(x, y, rotatedWidth, rotatedHeight, originalWidth, originalHeight, rotation) {
    switch (rotation) {
        case 0:
            return { x, y };
        case 90:
            // 90° CW: rotated (x,y) -> original (y, originalHeight - x)
            return { x: y, y: originalHeight - x };
        case 180:
            // 180°: rotated (x,y) -> original (originalWidth - x, originalHeight - y)
            return { x: originalWidth - x, y: originalHeight - y };
        case 270:
            // 270° CW (or 90° CCW): rotated (x,y) -> original (originalWidth - y, x)
            return { x: originalWidth - y, y: x };
        default:
            return { x, y };
    }
}

// Debounce function for performance optimization
function debounce(func, wait) {
    let timeout;
    return function executedFunction(...args) {
        const later = () => {
            clearTimeout(timeout);
            func(...args);
        };
        clearTimeout(timeout);
        timeout = setTimeout(later, wait);
    };
}

// Format file path to "path > to > file.mp4" format
function formatPathDisplay(filePath) {
    if (!filePath) return '-';

    // Normalize path separators to forward slashes
    const normalized = filePath.replace(/\\/g, '/');

    // Split the path into segments
    const segments = normalized.split('/').filter(seg => seg.length > 0);

    // Join with " > " separator
    return segments.join(' > ');
}

// Update window title and timeline filename
function updateWindowTitle() {
    const filenameElement = document.getElementById('timelineFilenameText');

    if (state.videoPath) {
        document.title = 'FFcut';
        if (filenameElement) {
            filenameElement.textContent = formatPathDisplay(state.videoPath);
        }
    } else {
        document.title = 'FFcut';
        if (filenameElement) {
            filenameElement.textContent = '-';
        }
    }
}

// Update File Info Display
function updateFileInfo() {
    if (!state.videoMetadata || !state.videoMetadata.streams) return;

    const videoStream = state.videoMetadata.streams.find(s => s.codec_type === 'video');
    state.audioStreams = state.videoMetadata.streams.filter(s => s.codec_type === 'audio');

    // Video codec
    if (videoStream) {
        const codec = videoStream.codec_name.toUpperCase();
        document.getElementById('videoCodecInfo').textContent = codec;
    }

    // Profile
    if (videoStream && videoStream.profile) {
        document.getElementById('profileInfo').textContent = videoStream.profile;
    } else {
        document.getElementById('profileInfo').textContent = '-';
    }

    // Pixel format
    if (state.videoMetadata.pix_fmt) {
        document.getElementById('pixelFormatInfo').textContent = state.videoMetadata.pix_fmt;
    } else if (videoStream && videoStream.pix_fmt) {
        document.getElementById('pixelFormatInfo').textContent = videoStream.pix_fmt;
    } else {
        document.getElementById('pixelFormatInfo').textContent = '-';
    }

    // Resolution
    if (videoStream && videoStream.width && videoStream.height) {
        document.getElementById('resolutionInfo').textContent = `${videoStream.width}×${videoStream.height}`;
    } else {
        document.getElementById('resolutionInfo').textContent = '-';
    }

    // Audio codec - show first audio stream info
    if (state.audioStreams.length > 0) {
        const audioStream = state.audioStreams[0];
        const codec = audioStream.codec_name.toUpperCase();
        const channels = audioStream.channels ? ` ${audioStream.channels}ch` : '';
        const trackInfo = state.audioStreams.length > 1 ? ` (${state.audioStreams.length} tracks)` : '';
        document.getElementById('audioCodecInfo').textContent = `${codec}${channels}${trackInfo}`;
    } else {
        document.getElementById('audioCodecInfo').textContent = 'None';
    }

    // FPS
    if (videoStream && videoStream.r_frame_rate) {
        const [num, den] = videoStream.r_frame_rate.split('/').map(Number);
        const fps = (num / den).toFixed(2);
        document.getElementById('fpsInfo').textContent = fps;
    }

    // Bitrate
    if (state.videoMetadata.format && state.videoMetadata.format.bit_rate) {
        const bitrate = parseInt(state.videoMetadata.format.bit_rate);
        const mbps = (bitrate / 1000000).toFixed(2);
        document.getElementById('bitrateInfo').textContent = `${mbps} Mbps`;
    }

    // File size
    if (state.videoMetadata.format && state.videoMetadata.format.size) {
        const size = parseInt(state.videoMetadata.format.size);
        const sizeFormatted = formatFileSize(size);
        document.getElementById('fileSizeInfo').textContent = sizeFormatted;
    } else {
        document.getElementById('fileSizeInfo').textContent = '-';
    }

    // Setup audio track selector
    setupAudioTrackSelector();

    // Update Auto codec option text
    updateAutoCodecOption();
}

// Update Auto codec option to show current video codec
function updateAutoCodecOption() {
    if (!state.videoMetadata || !state.videoMetadata.streams) return;

    const videoStream = state.videoMetadata.streams.find(s => s.codec_type === 'video');
    if (videoStream && videoStream.codec_name) {
        const codecName = videoStream.codec_name.toUpperCase();
        const autoOption = document.querySelector('#videoCodec option[value="auto"]');
        if (autoOption) {
            autoOption.textContent = `Auto | ${codecName}`;
        }
    }
}

// Setup Audio Track Selector
function setupAudioTrackSelector() {
    // Always clear the select elements first
    audioTrackSelect.innerHTML = '';
    audioExportTrack.innerHTML = '';
    audioTrackPicker.innerHTML = '';

    // Always show audio track controls
    audioTrackControl.style.display = 'flex';
    audioExportTrackGroup.style.display = 'block';

    if (state.audioStreams.length > 0) {
        // Show audio track selector for preview
        state.selectedAudioTrack = 0;

        // Populate preview track selector
        state.audioStreams.forEach((stream, index) => {
            const option = document.createElement('option');
            option.value = index;

            const codec = stream.codec_name.toUpperCase();
            const channels = stream.channels ? ` ${stream.channels}ch` : '';
            const language = stream.tags && stream.tags.language ? ` (${stream.tags.language})` : '';
            const title = stream.tags && stream.tags.title ? ` - ${stream.tags.title}` : '';

            option.textContent = `Track ${index + 1}: ${codec}${channels}${language}${title}`;
            audioTrackSelect.appendChild(option);
        });

        // Populate export track selector with new options
        const allOption = document.createElement('option');
        allOption.value = 'all';
        allOption.textContent = 'All tracks';
        audioExportTrack.appendChild(allOption);

        const pickExportOption = document.createElement('option');
        pickExportOption.value = 'pick-export';
        pickExportOption.textContent = 'Pick to export';
        audioExportTrack.appendChild(pickExportOption);

        const pickMergeOption = document.createElement('option');
        pickMergeOption.value = 'pick-merge';
        pickMergeOption.textContent = 'Pick to merge';
        audioExportTrack.appendChild(pickMergeOption);

        const noSoundOption = document.createElement('option');
        noSoundOption.value = 'none';
        noSoundOption.textContent = 'No sound';
        audioExportTrack.appendChild(noSoundOption);

        // Set default to 'all'
        audioExportTrack.value = 'all';
        state.exportSettings.audioTrack = 'all';

        // Initialize selected tracks array
        state.exportSettings.selectedAudioTracks = [];

        // Hide picker initially
        audioTrackPickerGroup.style.display = 'none';

        // Create track picker checkboxes
        createAudioTrackPicker();
    } else {
        // No audio streams - show "No Sound" option
        const noSoundOption = document.createElement('option');
        noSoundOption.value = 'none';
        noSoundOption.textContent = 'No Sound';
        audioTrackSelect.appendChild(noSoundOption);

        const noSoundExportOption = document.createElement('option');
        noSoundExportOption.value = 'none';
        noSoundExportOption.textContent = 'No Sound';
        audioExportTrack.appendChild(noSoundExportOption);

        audioTrackPickerGroup.style.display = 'none';
    }
}

// Create audio track picker with checkboxes
function createAudioTrackPicker() {
    audioTrackPicker.innerHTML = '';

    state.audioStreams.forEach((stream, index) => {
        const trackItem = document.createElement('div');
        trackItem.className = 'audio-track-item';

        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.id = `audio-track-${index}`;
        checkbox.value = index;
        checkbox.checked = false;

        const trackInfo = document.createElement('div');
        trackInfo.className = 'audio-track-info';

        const trackName = document.createElement('div');
        trackName.className = 'audio-track-name';
        const codec = stream.codec_name.toUpperCase();
        const language = stream.tags && stream.tags.language ? ` (${stream.tags.language})` : '';
        const title = stream.tags && stream.tags.title ? ` - ${stream.tags.title}` : '';
        trackName.textContent = `Track ${index + 1}${language}${title}`;

        const trackDetails = document.createElement('div');
        trackDetails.className = 'audio-track-details';
        const channels = stream.channels ? `${stream.channels}ch` : 'Unknown channels';
        const sampleRate = stream.sample_rate ? `${(stream.sample_rate / 1000).toFixed(1)}kHz` : '';
        const bitrate = stream.bit_rate ? `${(stream.bit_rate / 1000).toFixed(0)}kbps` : '';
        trackDetails.textContent = `${codec} • ${channels}${sampleRate ? ` • ${sampleRate}` : ''}${bitrate ? ` • ${bitrate}` : ''}`;

        trackInfo.appendChild(trackName);
        trackInfo.appendChild(trackDetails);

        trackItem.appendChild(checkbox);
        trackItem.appendChild(trackInfo);

        // Make the whole item clickable
        trackItem.addEventListener('click', (e) => {
            if (e.target !== checkbox) {
                checkbox.checked = !checkbox.checked;
                updateSelectedAudioTracks();
            }
        });

        checkbox.addEventListener('change', updateSelectedAudioTracks);

        audioTrackPicker.appendChild(trackItem);
    });
}

// Update selected audio tracks array
function updateSelectedAudioTracks() {
    const checkboxes = audioTrackPicker.querySelectorAll('input[type="checkbox"]:checked');
    state.exportSettings.selectedAudioTracks = Array.from(checkboxes).map(cb => parseInt(cb.value));
    updateFFmpegCommand();
}

// Build FFmpeg Command
function buildFFmpegCommand() {
    const videoCopy = state.exportSettings.videoCodec === 'copy';
    const videoAuto = state.exportSettings.videoCodec === 'auto';
    const audioCopy = state.exportSettings.audioCodec === 'copy';

    // Both copy - full lossless mode (not available with auto codec or transformations)
    if (videoCopy && audioCopy && !hasAnyTransformations()) {
        let cmd = '-c copy';

        if (state.trimStart > 0) {
            cmd = `-ss ${state.trimStart.toFixed(3)} ${cmd}`;
        }

        if (state.trimEnd < videoPreview.duration) {
            cmd += ` -t ${(state.trimEnd - state.trimStart).toFixed(3)}`;
        }

        // Handle audio track selection for lossless mode
        const audioTrackMode = state.exportSettings.audioTrack;

        if (audioTrackMode === 'none') {
            cmd += ` -map 0:v:0 -an`;
        } else if (audioTrackMode === 'all') {
            // All tracks - explicitly map video and all audio streams
            cmd += ` -map 0:v:0 -map 0:a`;
        } else if (audioTrackMode === 'pick-export') {
            if (state.exportSettings.selectedAudioTracks.length > 0) {
                cmd += ` -map 0:v:0`;
                state.exportSettings.selectedAudioTracks.forEach(trackIndex => {
                    cmd += ` -map 0:a:${trackIndex}`;
                });
            } else {
                cmd += ` -map 0:v:0 -an`;
            }
        } else if (audioTrackMode === 'pick-merge') {
            // Cannot merge with copy codec, need to re-encode
            // This will be handled in the encoding section below
            // For now, just map video
            if (state.exportSettings.selectedAudioTracks.length > 0) {
                // Merge requires encoding, so we can't use full lossless mode
                // Fall through to encoding section
                return buildFFmpegCommandWithEncoding();
            } else {
                cmd += ` -map 0:v:0 -an`;
            }
        } else if (state.audioStreams.length > 1 && audioTrackMode !== 'all') {
            // Legacy single track selection
            cmd += ` -map 0:v:0 -map 0:a:${state.exportSettings.audioTrack}`;
        }

        return cmd;
    }

    return buildFFmpegCommandWithEncoding();
}

function buildFFmpegCommandWithEncoding() {
    const videoCopy = state.exportSettings.videoCodec === 'copy';
    const videoAuto = state.exportSettings.videoCodec === 'auto';
    const audioCopy = state.exportSettings.audioCodec === 'copy';

    let cmd = '';

    // Input seeking (faster)
    if (state.trimStart > 0) {
        cmd += `-ss ${state.trimStart.toFixed(3)} `;
    }

    // Duration
    if (state.trimEnd < videoPreview.duration) {
        cmd += `-t ${(state.trimEnd - state.trimStart).toFixed(3)} `;
    }

    // Video filters
    let filters = [];

    // Rotation filter - apply BEFORE crop to match preview behavior
    if (state.rotation === 90) {
        filters.push('transpose=1'); // 90 degrees clockwise
    } else if (state.rotation === 180) {
        filters.push('transpose=1,transpose=1'); // 180 degrees
    } else if (state.rotation === 270) {
        filters.push('transpose=2'); // 90 degrees counter-clockwise
    }

    // Crop filter - apply AFTER rotation to match preview
    // Crop coordinates are relative to the rotated video
    if (state.cropEnabled) {
        const effectiveDims = getEffectiveVideoDimensions();
        const hasCrop = state.crop.width !== effectiveDims.width || 
                        state.crop.height !== effectiveDims.height ||
                        state.crop.x !== 0 || 
                        state.crop.y !== 0;
        
        if (hasCrop) {
            filters.push(`crop=${state.crop.width}:${state.crop.height}:${state.crop.x}:${state.crop.y}`);
        }
    }

    // Flip filters
    if (state.flipHorizontal) {
        filters.push('hflip');
    }
    if (state.flipVertical) {
        filters.push('vflip');
    }

    // Scale filter - only add if values are set (not -1)
    if (state.scale.width !== -1 || state.scale.height !== -1) {
        const scaleW = state.scale.width !== -1 ? state.scale.width : -1;
        const scaleH = state.scale.height !== -1 ? state.scale.height : -1;
        filters.push(`scale=${scaleW}:${scaleH}`);
        // Fix SAR (Sample Aspect Ratio) to prevent distortion in players
        filters.push('setsar=1');
    }

    if (filters.length > 0) {
        cmd += `-vf "${filters.join(',')}" `;
    }

    // Video codec
    if (videoCopy && !hasAnyTransformations()) {
        cmd += `-c:v copy `;
    } else if (videoAuto) {
        // Auto mode: use the encoder matching the original video codec
        const videoStream = state.videoMetadata?.streams?.find(s => s.codec_type === 'video');
        const originalCodecName = videoStream?.codec_name;
        const encoder = getEncoderForCodec(originalCodecName);

        cmd += `-c:v ${encoder} `;

        // For auto mode with CPU encoders, use reasonable quality settings
        if (encoder === 'libx264' || encoder === 'libx265') {
            cmd += `-crf 23 -preset medium `;
        } else if (encoder === 'libvpx' || encoder === 'libvpx-vp9') {
            cmd += `-crf 23 -b:v 0 `;
        } else if (encoder === 'libsvtav1') {
            cmd += `-crf 23 -preset 6 `;
        }

        // For auto mode, try to preserve source pixel format if compatible
        const sourcePixFmt = state.videoMetadata?.pix_fmt || videoStream?.pix_fmt;
        if (sourcePixFmt) {
            const supportedFormats = codecPixelFormatSupport[encoder] || ['yuv420p'];
            if (supportedFormats.includes(sourcePixFmt)) {
                cmd += `-pix_fmt ${sourcePixFmt} `;
            } else {
                // Fallback to yuv420p for compatibility
                cmd += `-pix_fmt yuv420p `;
            }
        } else {
            cmd += `-pix_fmt yuv420p `;
        }
    } else {
        cmd += `-c:v ${state.exportSettings.videoCodec} `;

        // Quality settings (only for encoding)
        const codec = state.exportSettings.videoCodec;
        if (codec !== 'copy') {
            if (state.exportSettings.qualityMode === 'crf') {
                // CRF mode
                if (codec.includes('nvenc')) {
                    cmd += `-cq ${state.exportSettings.crf} `;
                } else if (codec.includes('qsv')) {
                    cmd += `-global_quality ${state.exportSettings.crf} `;
                } else if (codec.includes('vaapi') || codec.includes('amf')) {
                    cmd += `-qp ${state.exportSettings.crf} `;
                } else if (codec.includes('libvpx')) {
                    cmd += `-crf ${state.exportSettings.crf} -b:v 0 `;
                } else {
                    cmd += `-crf ${state.exportSettings.crf} `;
                }
            } else {
                // Bitrate mode
                cmd += `-b:v ${state.exportSettings.videoBitrate}k `;
            }

            // Preset (for codecs that support it)
            if (codec.includes('264') || codec.includes('265') || codec.includes('libvpx') || codec.includes('libaom')) {
                cmd += `-preset ${state.exportSettings.preset} `;
            }

            // Pixel format handling
            if (state.exportSettings.pixelFormat !== 'auto') {
                // User explicitly selected a format
                cmd += `-pix_fmt ${state.exportSettings.pixelFormat} `;
            } else {
                // Auto mode: use source format if supported, otherwise fallback to yuv420p
                const supportedFormats = codecPixelFormatSupport[codec] || ['yuv420p'];
                const sourcePixFmt = state.videoMetadata?.pix_fmt;

                if (sourcePixFmt && supportedFormats.includes(sourcePixFmt)) {
                    // Source format is supported, use it
                    cmd += `-pix_fmt ${sourcePixFmt} `;
                } else {
                    // Fallback to yuv420p (most compatible)
                    cmd += `-pix_fmt yuv420p `;
                }
            }
        }
    }

    // Audio track selection
    const selectedContainer = state.exportSettings.container.toLowerCase();
    const audioTrackMode = state.exportSettings.audioTrack;

    if (audioTrackMode === 'none') {
        // No audio export - video only
        cmd += `-map 0:v:0 -an `;
    } else if (audioTrackMode === 'all') {
        // Export all audio tracks
        cmd += `-map 0:v:0 -map 0:a `;

        if (audioCopy) {
            cmd += `-c:a copy `;
        } else {
            cmd += `-c:a ${state.exportSettings.audioCodec} `;

            if (!state.exportSettings.audioCodec.startsWith('pcm')) {
                cmd += `-b:a ${state.exportSettings.audioBitrate}k `;
            }
        }
    } else if (audioTrackMode === 'pick-export') {
        // Export selected audio tracks
        if (state.exportSettings.selectedAudioTracks.length > 0) {
            cmd += `-map 0:v:0 `;

            state.exportSettings.selectedAudioTracks.forEach(trackIndex => {
                cmd += `-map 0:a:${trackIndex} `;
            });

            if (audioCopy) {
                cmd += `-c:a copy `;
            } else {
                cmd += `-c:a ${state.exportSettings.audioCodec} `;

                if (!state.exportSettings.audioCodec.startsWith('pcm')) {
                    cmd += `-b:a ${state.exportSettings.audioBitrate}k `;
                }
            }
        } else {
            // No tracks selected, default to video only
            cmd += `-map 0:v:0 -an `;
        }
    } else if (audioTrackMode === 'pick-merge') {
        // Merge selected audio tracks into one
        if (state.exportSettings.selectedAudioTracks.length > 0) {
            if (state.exportSettings.selectedAudioTracks.length === 1) {
                // Single track - just map it normally
                cmd += `-map 0:v:0 -map 0:a:${state.exportSettings.selectedAudioTracks[0]} `;

                if (audioCopy) {
                    cmd += `-c:a copy `;
                } else {
                    cmd += `-c:a ${state.exportSettings.audioCodec} `;
                    if (!state.exportSettings.audioCodec.startsWith('pcm')) {
                        cmd += `-b:a ${state.exportSettings.audioBitrate}k `;
                    }
                }
            } else {
                // Multiple tracks - merge them using amix filter
                // amix is better than amerge for mixing multiple audio streams
                const trackInputs = state.exportSettings.selectedAudioTracks
                    .map(trackIndex => `[0:a:${trackIndex}]`)
                    .join('');

                cmd += `-filter_complex "${trackInputs}amix=inputs=${state.exportSettings.selectedAudioTracks.length}:duration=longest:dropout_transition=0[aout]" `;
                cmd += `-map 0:v:0 -map [aout] `;

                // Always need to encode when merging
                cmd += `-c:a ${state.exportSettings.audioCodec} `;
                if (!state.exportSettings.audioCodec.startsWith('pcm')) {
                    cmd += `-b:a ${state.exportSettings.audioBitrate}k `;
                }
            }
        } else {
            // No tracks selected, default to video only
            cmd += `-map 0:v:0 -an `;
        }
    } else {
        // Legacy support: single track by index (should not be used anymore)
        if (state.audioStreams.length > 0) {
            const trackIndex = parseInt(audioTrackMode);
            cmd += `-map 0:v:0 -map 0:a:${trackIndex} `;
        }

        if (audioCopy) {
            cmd += `-c:a copy `;
        } else {
            cmd += `-c:a ${state.exportSettings.audioCodec} `;

            if (!state.exportSettings.audioCodec.startsWith('pcm')) {
                cmd += `-b:a ${state.exportSettings.audioBitrate}k `;
            }
        }
    }

    return cmd.trim();
}

function updateFFmpegCommand() {
    const cmd = buildFFmpegCommand();
    ffmpegCommand.value = cmd;
    state.exportSettings.customCommand = cmd;
}

// Initialize codec UI visibility based on current codec selections
function initializeCodecUI() {
    // Video codec settings
    const videoCopy = state.exportSettings.videoCodec === 'copy';
    const videoAuto = state.exportSettings.videoCodec === 'auto';
    const qualityGroup = qualityMode.closest('.control-group');

    if (qualityGroup) qualityGroup.style.display = (videoCopy || videoAuto) ? 'none' : 'block';
    if (crfSettings) crfSettings.style.display = ((videoCopy || videoAuto) || state.exportSettings.qualityMode !== 'crf') ? 'none' : 'block';
    if (bitrateSettings) bitrateSettings.style.display = ((videoCopy || videoAuto) || state.exportSettings.qualityMode !== 'bitrate') ? 'none' : 'block';

    const codec = state.exportSettings.videoCodec;
    const supportsPreset = codec.includes('264') || codec.includes('265') ||
    codec.includes('libvpx') || codec.includes('libaom');
    presetGroup.style.display = (supportsPreset && !videoCopy && !videoAuto) ? 'block' : 'none';

    // Always show pixel format selector when encoding video
    pixelFormatGroup.style.display = (!videoCopy && !videoAuto) ? 'block' : 'none';

    // Audio codec settings
    const audioCopy = state.exportSettings.audioCodec === 'copy';
    const audioBitrateGroup = audioBitrate.closest('.control-group');
    if (audioBitrateGroup) {
        audioBitrateGroup.style.display = audioCopy ? 'none' : 'block';
    }
}

// Update pixel format options based on selected codec
function updatePixelFormatOptions(codec) {
    const supportedFormats = codecPixelFormatSupport[codec] || ['yuv420p']; // Default to yuv420p if codec not found
    const currentValue = pixelFormat.value;

    // Save current selection
    const wasAuto = currentValue === 'auto';

    // Clear existing options except "Auto"
    pixelFormat.innerHTML = '<option value="auto">Auto | from source</option>';

    // Group formats by category
    const groups = {
        '420-8bit': { label: '8-bit 4:2:0 (Most Compatible)', formats: [] },
        '422-8bit': { label: '8-bit 4:2:2 (Better Quality)', formats: [] },
        '444-8bit': { label: '8-bit 4:4:4 (Highest Quality)', formats: [] },
        '420-10bit': { label: '10-bit 4:2:0', formats: [] },
        '422-10bit': { label: '10-bit 4:2:2', formats: [] },
        '444-10bit': { label: '10-bit 4:4:4', formats: [] },
        '420-12bit': { label: '12-bit 4:2:0', formats: [] },
        '422-12bit': { label: '12-bit 4:2:2', formats: [] },
        '444-12bit': { label: '12-bit 4:4:4', formats: [] }
    };

    // Sort supported formats into groups
    supportedFormats.forEach(format => {
        const info = pixelFormatInfo[format];
        if (info && groups[info.group]) {
            groups[info.group].formats.push({ value: format, name: info.name });
        }
    });

    // Add optgroups with formats
    Object.keys(groups).forEach(groupKey => {
        const group = groups[groupKey];
        if (group.formats.length > 0) {
            const optgroup = document.createElement('optgroup');
            optgroup.label = group.label;

            group.formats.forEach(format => {
                const option = document.createElement('option');
                option.value = format.value;
                option.textContent = format.name;
                optgroup.appendChild(option);
            });

            pixelFormat.appendChild(optgroup);
        }
    });

    // Restore selection if still available, otherwise reset to auto
    if (!wasAuto && supportedFormats.includes(currentValue)) {
        pixelFormat.value = currentValue;
    } else {
        pixelFormat.value = 'auto';
        state.exportSettings.pixelFormat = 'auto';
    }
}

// Import Video
// If filePath is provided (CLI arg / second-instance / macOS open-file) the
// dialog is skipped; otherwise it behaves exactly as before.
async function importVideo(filePath) {
    if (!filePath) {
        filePath = await window.electronAPI.selectVideo();
    }
    if (!filePath) return;

    let wasCancelled = false;

    try {
        state.videoPath = filePath;
        state.videoFilename = window.electronAPI.path.basename(filePath);

        // Show loading message
        const loadingMsg = document.createElement('div');
        loadingMsg.id = 'loadingMessage';
        loadingMsg.style.cssText = 'position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); background: rgba(0,0,0,0.9); padding: 30px; border-radius: 12px; z-index: 10000; text-align: center; color: white; min-width: 300px;';
        loadingMsg.innerHTML = '<div style="font-size: 18px; margin-bottom: 10px;">Loading video...</div><div id="transcodeStatus" style="font-size: 14px; color: #888; margin-bottom: 15px;"></div><button id="cancelTranscodeBtn" style="background: #ff4444; color: white; border: none; padding: 8px 16px; border-radius: 6px; cursor: pointer; font-size: 14px;">Cancel</button>';
        document.body.appendChild(loadingMsg);

        // Add cancel button handler
        const cancelBtn = document.getElementById('cancelTranscodeBtn');
        cancelBtn.addEventListener('click', async () => {
            try {
                await window.electronAPI.cancelTranscode();
                wasCancelled = true;
                cancelBtn.disabled = true;
                cancelBtn.textContent = 'Cancelling...';
                // Update status
                const statusDiv = document.getElementById('transcodeStatus');
                if (statusDiv) {
                    statusDiv.textContent = 'Cancelling...';
                }
            } catch (error) {
                console.error('Failed to cancel transcode:', error);
            }
        });

        // Check if operation was cancelled before starting
        if (wasCancelled) {
            const msg = document.getElementById('loadingMessage');
            if (msg) msg.remove();
            return;
        }

        // First, get video info to determine if we need transcoding
        let videoInfo;
        try {
            videoInfo = await window.electronAPI.getVideoInfo(filePath);
        } catch (error) {
            // Handle cancellation errors gracefully
            if (isCancellationError(error)) {
                // Remove loading message silently
                const msg = document.getElementById('loadingMessage');
                if (msg) msg.remove();
                return;
            }
            // Re-throw other errors
            throw error;
        }

        state.videoMetadata = videoInfo.metadata;
        state.needsPreview = !!videoInfo.needsPreview;
        state.previewVideoPath = null;
        state.previewAudioPaths = null;
        state.previewPath = null;

        // Determine if we need transcoding (multi-audio or unsupported codec)
        const needsTranscoding = videoInfo.multiAudio || videoInfo.needsPreview;

        // Save current playback state in case we need to restore it
        const wasPlayingBefore = !videoPreview.paused;
        const currentTimeBefore = videoPreview.currentTime;
        const wasAudioPlayingBefore = state.previewAudioPaths && !previewAudio.paused;

        // Only stop playback and show loading dialog if transcoding is needed
        if (needsTranscoding) {
            // Stop current playback before loading new video
            if (!videoPreview.paused) {
                videoPreview.pause();
            }
            if (state.previewAudioPaths && !previewAudio.paused) {
                previewAudio.pause();
            }

            // Reset playback state when loading new video
            state.isPlaying = false;
            playIcon.style.display = 'block';
            pauseIcon.style.display = 'none';

            // Clear any existing sources to prevent conflicts
            videoPreview.src = '';
            previewAudio.src = '';
        }

        if (videoInfo.multiAudio) {
            const transcodeStatus = document.getElementById('transcodeStatus');
            if (transcodeStatus) transcodeStatus.textContent = 'Splitting video and audio tracks...';
            try {
                const split = await window.electronAPI.prepareMultiAudioPreview(filePath, videoInfo.metadata);
                state.previewVideoPath = split.videoPath;
                state.previewAudioPaths = split.audioPaths;
                videoPreview.src = filePathToUrl(split.videoPath);
                videoPreview.muted = true;
                previewAudio.src = filePathToUrl(split.audioPaths[0]);
                previewAudio.volume = volumeSlider.value / 100;
            } catch (error) {
                if (isCancellationError(error)) {
                    // Restore previous video state if operation was cancelled
                    if (wasPlayingBefore) {
                        // Try to restore playback state
                        videoPreview.currentTime = currentTimeBefore;
                        setTimeout(() => {
                            videoPreview.play().catch(err => {
                                console.warn('Failed to restore playback after cancellation:', err);
                            });
                            if (wasAudioPlayingBefore) {
                                previewAudio.currentTime = currentTimeBefore;
                                previewAudio.play().catch(err => {
                                    console.warn('Failed to restore audio playback after cancellation:', err);
                                });
                            }
                            state.isPlaying = true;
                            playIcon.style.display = 'none';
                            pauseIcon.style.display = 'block';
                        }, 100);
                    }
                    // Remove loading message and show cancelled status
                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();
                    alert('Video loading cancelled by user');
                    return;
                }
                throw error;
            }
        } else if (videoInfo.needsPreview) {
            state.previewPath = videoInfo.previewPath;
            videoPreview.src = filePathToUrl(videoInfo.previewPath);
            videoPreview.muted = false;
            previewAudio.src = '';
        } else {
            // Video doesn't need preview, load directly
            // Don't stop playback for this case - try to preserve it
            videoPreview.src = filePathToUrl(filePath);
            videoPreview.muted = false;
            previewAudio.src = '';

            // Remove loading message immediately since no transcoding needed
            const msg = document.getElementById('loadingMessage');
            if (msg) msg.remove();
        }

        updateWindowTitle();
        state.videoElement = videoPreview;

        videoPreview.addEventListener('loadedmetadata', () => {
            // Ensure playback state is correctly reset after loading
            state.isPlaying = false;
            playIcon.style.display = 'block';
            pauseIcon.style.display = 'none';

            // Remove loading message
            const msg = document.getElementById('loadingMessage');
            if (msg) msg.remove();

            const duration = videoPreview.duration;
            state.trimStart = 0;
            state.trimEnd = duration;

            totalTimeDisplay.textContent = formatTime(duration);
            scrubber.max = duration;
            trimStartInput.max = duration;
            trimEndInput.max = duration;
            trimStartInput.value = '0.0';
            trimEndInput.value = duration.toFixed(1);

            // Initialize trim handles
            updateTrimHandles();
            updateTrimInfo();

            // Set crop defaults
            const videoWidth = state.videoMetadata.streams[0].width;
            const videoHeight = state.videoMetadata.streams[0].height;
            cropWidth.max = videoWidth;
            cropHeight.max = videoHeight;
            cropX.max = videoWidth;
            cropY.max = videoHeight;
            cropWidth.value = videoWidth;
            cropHeight.value = videoHeight;
            cropX.value = 0;
            cropY.value = 0;
            state.crop.width = videoWidth;
            state.crop.height = videoHeight;
            state.crop.originalWidth = videoWidth;
            state.crop.originalHeight = videoHeight;
            state.crop.x = 0;
            state.crop.y = 0;
            
            // Reset transformations
            state.rotation = 0;
            state.flipHorizontal = false;
            state.flipVertical = false;
            state.scale.width = -1;
            state.scale.height = -1;
            scaleWidth.value = '';
            scaleHeight.value = '';
            updateTransformUI();
            updatePreviewTransform();

            // Update file info
            updateFileInfo();

            // Initialize UI elements visibility based on default codec settings
            initializeCodecUI();

            // Update pixel format options based on current codec
            if (state.exportSettings.videoCodec !== 'copy') {
                updatePixelFormatOptions(state.exportSettings.videoCodec);
            }

            // Update FFmpeg command
            updateFFmpegCommand();
        });

        // Show editor
        emptyState.style.display = 'none';
        editorState.style.display = 'grid';
    } catch (error) {
        console.error('Error importing video:', error);

        // Handle cancellation errors gracefully
        if (isCancellationError(error)) {
            // Loading message should already be removed by inner handlers
            return;
        }

        const msg = document.getElementById('loadingMessage');
        if (msg) msg.remove();
        alert('Error importing video: ' + error.message);
    }
}

// Drag and Drop - Global handlers to prevent default behavior
document.addEventListener('dragover', (e) => {
    e.preventDefault();
    e.stopPropagation();
});

document.addEventListener('drop', (e) => {
    e.preventDefault();
    e.stopPropagation();
});

// Drag and Drop - Empty state
dropZone.addEventListener('dragover', (e) => {
    e.preventDefault();
    e.stopPropagation();
    dropZone.classList.add('drag-over');
});

dropZone.addEventListener('dragleave', (e) => {
    e.preventDefault();
    e.stopPropagation();
    // Only remove class if we're actually leaving the drop zone
    if (!dropZone.contains(e.relatedTarget)) {
        dropZone.classList.remove('drag-over');
    }
});

dropZone.addEventListener('drop', async (e) => {
    e.preventDefault();
    e.stopPropagation();
    dropZone.classList.remove('drag-over');

    console.log('Drop event on dropZone:', e);
    const files = e.dataTransfer.files;
    console.log('Files count:', files.length);

    if (files.length > 0) {
        const file = files[0];
        console.log('File object:', file);
        console.log('File.name:', file.name);

        // Use webUtils.getPathForFile through electronAPI
        let filePath;
        try {
            filePath = window.electronAPI.getFilePathFromFile(file);
            console.log('File path from webUtils:', filePath);
        } catch (error) {
            console.error('Error getting file path:', error);
            filePath = null;
        }

        if (!filePath) {
            console.error('Could not get file path');
            alert('Error: Could not access file path. Please use the Import button instead.');
            return;
        }

        // Check if it's a video file
        const videoExtensions = ['.mp4', '.mov', '.avi', '.mkv', '.webm', '.flv', '.wmv', '.m4v'];
        const ext = window.electronAPI.path.extname(filePath).toLowerCase();
        console.log('File extension:', ext);

        if (videoExtensions.includes(ext)) {
            let wasCancelled = false;

            try {
                state.videoPath = filePath;
                state.videoFilename = window.electronAPI.path.basename(filePath);

                // Show loading message
                const loadingMsg = document.createElement('div');
                loadingMsg.id = 'loadingMessage';
                loadingMsg.style.cssText = 'position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); background: rgba(0,0,0,0.9); padding: 30px; border-radius: 12px; z-index: 10000; text-align: center; color: white; min-width: 300px;';
                loadingMsg.innerHTML = '<div style="font-size: 18px; margin-bottom: 10px;">Loading video...</div><div id="transcodeStatus" style="font-size: 14px; color: #888; margin-bottom: 15px;"></div><button id="cancelTranscodeBtn" style="background: #ff4444; color: white; border: none; padding: 8px 16px; border-radius: 6px; cursor: pointer; font-size: 14px;">Cancel</button>';
                document.body.appendChild(loadingMsg);

                // Add cancel button handler
                const cancelBtn = document.getElementById('cancelTranscodeBtn');
                cancelBtn.addEventListener('click', async () => {
                    try {
                        await window.electronAPI.cancelTranscode();
                        wasCancelled = true;
                        cancelBtn.disabled = true;
                        cancelBtn.textContent = 'Cancelling...';
                        // Update status
                        const statusDiv = document.getElementById('transcodeStatus');
                        if (statusDiv) {
                            statusDiv.textContent = 'Cancelling...';
                        }
                    } catch (error) {
                        console.error('Failed to cancel transcode:', error);
                    }
                });

                // Check if operation was cancelled before starting
                if (wasCancelled) {
                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();
                    return;
                }

                let videoInfo;
                try {
                    videoInfo = await window.electronAPI.getVideoInfo(filePath);
                } catch (error) {
                // Handle cancellation errors gracefully
                if (isCancellationError(error)) {
                    // Remove loading message silently
                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();
                    return;
                }
                    // Re-throw other errors
                    throw error;
                }

                state.videoMetadata = videoInfo.metadata;
                state.needsPreview = !!videoInfo.needsPreview;
                state.previewVideoPath = null;
                state.previewAudioPaths = null;
                state.previewPath = null;

                if (videoInfo.multiAudio) {
                    const transcodeStatus = document.getElementById('transcodeStatus');
                    if (transcodeStatus) transcodeStatus.textContent = 'Splitting video and audio tracks...';
                    try {
                        const split = await window.electronAPI.prepareMultiAudioPreview(filePath, videoInfo.metadata);
                        state.previewVideoPath = split.videoPath;
                        state.previewAudioPaths = split.audioPaths;
                        videoPreview.src = filePathToUrl(split.videoPath);
                        videoPreview.muted = true;
                        previewAudio.src = filePathToUrl(split.audioPaths[0]);
                        previewAudio.volume = volumeSlider.value / 100;
                    } catch (error) {
                        if (isCancellationError(error)) {
                            // Restore previous video state if operation was cancelled
                            if (wasPlayingBefore) {
                                // Try to restore playback state
                                videoPreview.currentTime = currentTimeBefore;
                                setTimeout(() => {
                                    videoPreview.play().catch(err => {
                                        console.warn('Failed to restore playback after cancellation:', err);
                                    });
                                    if (wasAudioPlayingBefore) {
                                        previewAudio.currentTime = currentTimeBefore;
                                        previewAudio.play().catch(err => {
                                            console.warn('Failed to restore audio playback after cancellation:', err);
                                        });
                                    }
                                    state.isPlaying = true;
                                    playIcon.style.display = 'none';
                                    pauseIcon.style.display = 'block';
                                }, 100);
                            }
                            // Remove loading message and show cancelled status
                            const msg = document.getElementById('loadingMessage');
                            if (msg) msg.remove();
                            alert('Video loading cancelled by user');
                            return;
                        }
                        throw error;
                    }
                } else if (videoInfo.needsPreview) {
                    state.previewPath = videoInfo.previewPath;
                    videoPreview.src = filePathToUrl(videoInfo.previewPath);
                    videoPreview.muted = false;
                    previewAudio.src = '';
                } else {
                    // Video doesn't need preview, load directly
                    // Don't stop playback for this case - try to preserve it
                    videoPreview.src = filePathToUrl(filePath);
                    videoPreview.muted = false;
                    previewAudio.src = '';

                    // Remove loading message immediately since no transcoding needed
                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();
                }

                updateWindowTitle();
                state.videoElement = videoPreview;

                videoPreview.addEventListener('loadedmetadata', () => {
                    // Ensure playback state is correctly reset after loading
                    state.isPlaying = false;
                    playIcon.style.display = 'block';
                    pauseIcon.style.display = 'none';

                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();

                    const duration = videoPreview.duration;
                    state.trimStart = 0;
                    state.trimEnd = duration;

                    totalTimeDisplay.textContent = formatTime(duration);
                    scrubber.max = duration;
                    trimStartInput.max = duration;
                    trimEndInput.max = duration;
                    trimStartInput.value = '0.0';
                    trimEndInput.value = duration.toFixed(1);

                    updateTrimHandles();
                    updateTrimInfo();

                    const videoWidth = state.videoMetadata.streams[0].width;
                    const videoHeight = state.videoMetadata.streams[0].height;
                    cropWidth.max = videoWidth;
                    cropHeight.max = videoHeight;
                    cropX.max = videoWidth;
                    cropY.max = videoHeight;
                    cropWidth.value = videoWidth;
                    cropHeight.value = videoHeight;
                    state.crop.width = videoWidth;
                    state.crop.height = videoHeight;

                    updateFileInfo();
                    initializeCodecUI();
                    updateFFmpegCommand();
                });

                emptyState.style.display = 'none';
                editorState.style.display = 'grid';
            } catch (error) {
                console.error('Error loading video:', error);

                // Handle cancellation errors gracefully
                if (isCancellationError(error)) {
                    // Loading message should already be removed by inner handlers
                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();
                    // Don't show alert for cancellation
                    return;
                }

                const msg = document.getElementById('loadingMessage');
                if (msg) msg.remove();
                alert('Error loading video: ' + error.message);
            }
        } else {
            alert('Please drop a video file');
        }
    }
});

// Drag and Drop - Editor state (when video is already loaded)
editorState.addEventListener('dragover', (e) => {
    e.preventDefault();
    e.stopPropagation();
    editorState.classList.add('drag-over');
});

editorState.addEventListener('dragleave', (e) => {
    e.preventDefault();
    e.stopPropagation();
    // Only remove class if we're actually leaving the editor state
    if (!editorState.contains(e.relatedTarget)) {
        editorState.classList.remove('drag-over');
    }
});

editorState.addEventListener('drop', async (e) => {
    e.preventDefault();
    e.stopPropagation();
    editorState.classList.remove('drag-over');

    console.log('Drop event on editorState:', e);
    const files = e.dataTransfer.files;
    console.log('Files count:', files.length);

    if (files.length > 0) {
        const file = files[0];
        console.log('File object:', file);
        console.log('File.name:', file.name);

        // Use webUtils.getPathForFile through electronAPI
        let filePath;
        try {
            filePath = window.electronAPI.getFilePathFromFile(file);
            console.log('File path from webUtils:', filePath);
        } catch (error) {
            console.error('Error getting file path:', error);
            filePath = null;
        }

        if (!filePath) {
            console.error('Could not get file path');
            alert('Error: Could not access file path. Please use the Import button instead.');
            return;
        }

        // Check if it's a video file
        const videoExtensions = ['.mp4', '.mov', '.avi', '.mkv', '.webm', '.flv', '.wmv', '.m4v'];
        const ext = window.electronAPI.path.extname(filePath).toLowerCase();
        console.log('File extension:', ext);

        if (videoExtensions.includes(ext)) {
            let wasCancelled = false;

            // Load new video (reuse import logic)
            try {
                state.videoPath = filePath;
                state.videoFilename = window.electronAPI.path.basename(filePath);

                // Show loading message
                const loadingMsg = document.createElement('div');
                loadingMsg.id = 'loadingMessage';
                loadingMsg.style.cssText = 'position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); background: rgba(0,0,0,0.9); padding: 30px; border-radius: 12px; z-index: 10000; text-align: center; color: white; min-width: 300px;';
                loadingMsg.innerHTML = '<div style="font-size: 18px; margin-bottom: 10px;">Loading video...</div><div id="transcodeStatus" style="font-size: 14px; color: #888; margin-bottom: 15px;"></div><button id="cancelTranscodeBtn" style="background: #ff4444; color: white; border: none; padding: 8px 16px; border-radius: 6px; cursor: pointer; font-size: 14px;">Cancel</button>';
                document.body.appendChild(loadingMsg);

                // Add cancel button handler
                const cancelBtn = document.getElementById('cancelTranscodeBtn');
                cancelBtn.addEventListener('click', async () => {
                    try {
                        await window.electronAPI.cancelTranscode();
                        wasCancelled = true;
                        cancelBtn.disabled = true;
                        cancelBtn.textContent = 'Cancelling...';
                        // Update status
                        const statusDiv = document.getElementById('transcodeStatus');
                        if (statusDiv) {
                            statusDiv.textContent = 'Cancelling...';
                        }
                    } catch (error) {
                        console.error('Failed to cancel transcode:', error);
                    }
                });

                // Check if operation was cancelled before starting
                if (wasCancelled) {
                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();
                    return;
                }

                let videoInfo;
                try {
                    videoInfo = await window.electronAPI.getVideoInfo(filePath);
                } catch (error) {
                // Handle cancellation errors gracefully
                if (isCancellationError(error)) {
                    // Remove loading message silently
                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();
                    return;
                }
                    // Re-throw other errors
                    throw error;
                }

                state.videoMetadata = videoInfo.metadata;
                state.needsPreview = !!videoInfo.needsPreview;
                state.previewVideoPath = null;
                state.previewAudioPaths = null;
                state.previewPath = null;

                if (videoInfo.multiAudio) {
                    const transcodeStatus = document.getElementById('transcodeStatus');
                    if (transcodeStatus) transcodeStatus.textContent = 'Splitting video and audio tracks...';
                    try {
                        const split = await window.electronAPI.prepareMultiAudioPreview(filePath, videoInfo.metadata);
                        state.previewVideoPath = split.videoPath;
                        state.previewAudioPaths = split.audioPaths;
                        videoPreview.src = filePathToUrl(split.videoPath);
                        videoPreview.muted = true;
                        previewAudio.src = filePathToUrl(split.audioPaths[0]);
                        previewAudio.volume = volumeSlider.value / 100;
                    } catch (error) {
                        if (isCancellationError(error)) {
                            // Restore previous video state if operation was cancelled
                            if (wasPlayingBefore) {
                                // Try to restore playback state
                                videoPreview.currentTime = currentTimeBefore;
                                setTimeout(() => {
                                    videoPreview.play().catch(err => {
                                        console.warn('Failed to restore playback after cancellation:', err);
                                    });
                                    if (wasAudioPlayingBefore) {
                                        previewAudio.currentTime = currentTimeBefore;
                                        previewAudio.play().catch(err => {
                                            console.warn('Failed to restore audio playback after cancellation:', err);
                                        });
                                    }
                                    state.isPlaying = true;
                                    playIcon.style.display = 'none';
                                    pauseIcon.style.display = 'block';
                                }, 100);
                            }
                            // Remove loading message and show cancelled status
                            const msg = document.getElementById('loadingMessage');
                            if (msg) msg.remove();
                            alert('Video loading cancelled by user');
                            return;
                        }
                        throw error;
                    }
                } else if (videoInfo.needsPreview) {
                    state.previewPath = videoInfo.previewPath;
                    videoPreview.src = filePathToUrl(videoInfo.previewPath);
                    videoPreview.muted = false;
                    previewAudio.src = '';
                } else {
                    // Video doesn't need preview, load directly
                    // Don't stop playback for this case - try to preserve it
                    videoPreview.src = filePathToUrl(filePath);
                    videoPreview.muted = false;
                    previewAudio.src = '';

                    // Remove loading message immediately since no transcoding needed
                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();
                }

                updateWindowTitle();
                state.videoElement = videoPreview;

                videoPreview.addEventListener('loadedmetadata', () => {
                    // Ensure playback state is correctly reset after loading
                    state.isPlaying = false;
                    playIcon.style.display = 'block';
                    pauseIcon.style.display = 'none';

                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();

                    const duration = videoPreview.duration;
                    state.trimStart = 0;
                    state.trimEnd = duration;

                    totalTimeDisplay.textContent = formatTime(duration);
                    scrubber.max = duration;
                    trimStartInput.max = duration;
                    trimEndInput.max = duration;
                    trimStartInput.value = '0.0';
                    trimEndInput.value = duration.toFixed(1);

                    updateTrimHandles();
                    updateTrimInfo();

                    const videoWidth = state.videoMetadata.streams[0].width;
                    const videoHeight = state.videoMetadata.streams[0].height;
                    cropWidth.max = videoWidth;
                    cropHeight.max = videoHeight;
                    cropX.max = videoWidth;
                    cropY.max = videoHeight;
                    cropWidth.value = videoWidth;
                    cropHeight.value = videoHeight;
                    state.crop.width = videoWidth;
                    state.crop.height = videoHeight;

                    updateFileInfo();
                    initializeCodecUI();
                    updateFFmpegCommand();
                });
            } catch (error) {
                console.error('Error loading video:', error);

                // Handle cancellation errors gracefully
                if (isCancellationError(error)) {
                    // Loading message should already be removed by inner handlers
                    const msg = document.getElementById('loadingMessage');
                    if (msg) msg.remove();
                    // Don't show alert for cancellation
                    return;
                }

                const msg = document.getElementById('loadingMessage');
                if (msg) msg.remove();
                alert('Error loading video: ' + error.message);
            }
        } else {
            alert('Please drop a video file');
        }
    }
});

// Event Listeners - Import (фиксим двойной вызов)
let isImporting = false;

document.getElementById('menuImport').addEventListener('click', async () => {
    if (isImporting) return;
    isImporting = true;
    closeAllMenus();
    await importVideo();
    // Сбрасываем флаг после небольшой задержки
    setTimeout(() => {
        isImporting = false;
    }, 300);
});

document.getElementById('importBtnEmpty').addEventListener('click', async () => {
    if (isImporting) return;
    isImporting = true;
    await importVideo();
    setTimeout(() => {
        isImporting = false;
    }, 300);
});

// Video Playback Controls
playPauseBtn.addEventListener('click', async () => {
    if (state.isPlaying) {
        videoPreview.pause();
        if (state.previewAudioPaths) previewAudio.pause();
    } else {
        // Sync audio time before playing to minimize drift
        if (state.previewAudioPaths && previewAudio.src) {
            previewAudio.currentTime = videoPreview.currentTime;

            // Start both simultaneously
            try {
                await Promise.all([
                    videoPreview.play(),
                                  previewAudio.play()
                ]);
            } catch (err) {
                console.warn('Playback start failed:', err);
                // Fallback: try starting them sequentially
                videoPreview.play();
                if (state.previewAudioPaths) previewAudio.play();
            }
        } else {
            videoPreview.play();
        }
    }
});

// Frame navigation controls
frameBackBtn.addEventListener('click', () => {
    const step = getFrameStep();
    const newTime = Math.max(0, videoPreview.currentTime - step);
    videoPreview.currentTime = newTime;
    if (state.previewAudioPaths) previewAudio.currentTime = newTime;
    currentTimeDisplay.textContent = formatTime(newTime);
    updatePlayhead();
});

frameForwardBtn.addEventListener('click', () => {
    const step = getFrameStep();
    const newTime = Math.min(videoPreview.duration, videoPreview.currentTime + step);
    videoPreview.currentTime = newTime;
    if (state.previewAudioPaths) previewAudio.currentTime = newTime;
    currentTimeDisplay.textContent = formatTime(newTime);
    updatePlayhead();
});

function getFrameStep() {
    // Try to get frame rate from video metadata
    if (state.videoMetadata && state.videoMetadata.streams) {
        const videoStream = state.videoMetadata.streams.find(s => s.codec_type === 'video');
        if (videoStream && videoStream.r_frame_rate) {
            const [num, den] = videoStream.r_frame_rate.split('/').map(Number);
            const fps = num / den;
            if (fps > 0) {
                return 1 / fps; // One frame duration in seconds
            }
        }
    }
    // Fallback to 5 seconds if frame rate is not available
    return 5;
}

videoPreview.addEventListener('play', () => {
    state.isPlaying = true;
    if (state.previewAudioPaths) previewAudio.play();
    playIcon.style.display = 'none';
    pauseIcon.style.display = 'block';

    // Reset trim region tracking when starting playback
    const hasTrimRegion = state.trimEnd > 0 && state.trimEnd < videoPreview.duration;
    if (hasTrimRegion) {
        const currentTime = videoPreview.currentTime;
        // Set flag based on current position
        state.lastTimeInTrimRegion = currentTime >= state.trimStart && currentTime < state.trimEnd;
    }
});

videoPreview.addEventListener('pause', () => {
    state.isPlaying = false;
    if (state.previewAudioPaths) {
        previewAudio.pause();
        // Reset playback rate when paused
        previewAudio.playbackRate = 1.0;
    }
    playIcon.style.display = 'block';
    pauseIcon.style.display = 'none';
});

// Handle video end
videoPreview.addEventListener('ended', () => {
    // If loop is enabled, jump back to trim start (or beginning if no trim)
    if (state.isLooping) {
        const hasTrimRegion = state.trimEnd > 0 && state.trimEnd < videoPreview.duration;
        videoPreview.currentTime = hasTrimRegion ? state.trimStart : 0;
        if (state.previewAudioPaths) previewAudio.currentTime = hasTrimRegion ? state.trimStart : 0;
        videoPreview.play();
        if (state.previewAudioPaths) previewAudio.play();
    }
});

// Handle seeking events to prevent audio glitches
videoPreview.addEventListener('seeking', () => {
    // Pause audio during seek to prevent clicks/pops
    if (state.previewAudioPaths && previewAudio.src && !previewAudio.paused) {
        state.wasAudioPlayingBeforeSeek = true;
        previewAudio.pause();
    }
});

videoPreview.addEventListener('seeked', () => {
    // Sync audio position after seek completes
    if (state.previewAudioPaths && previewAudio.src) {
        previewAudio.currentTime = videoPreview.currentTime;

        // Resume audio if it was playing before seek
        if (state.wasAudioPlayingBeforeSeek && state.isPlaying) {
            // Small delay to ensure smooth resume
            setTimeout(() => {
                previewAudio.play().catch(err => {
                    console.warn('Audio resume after seek failed:', err);
                });
            }, 20);
            state.wasAudioPlayingBeforeSeek = false;
        }
    }
});

videoPreview.addEventListener('timeupdate', () => {
    if (!state.isScrubbing) {
        const currentTime = videoPreview.currentTime;

        // Smart audio sync with automatic drift compensation
        if (state.previewAudioPaths && previewAudio.src && !previewAudio.paused) {
            const audioTime = previewAudio.currentTime;
            const timeDiff = currentTime - audioTime;
            const absDiff = Math.abs(timeDiff);

            // Threshold for hard sync (significant drift)
            const hardSyncThreshold = 0.3; // 300ms

            // Threshold for soft sync via playback rate adjustment
            const softSyncThreshold = 0.05; // 50ms

            if (absDiff > hardSyncThreshold) {
                // Large drift: hard sync by jumping to position
                previewAudio.currentTime = currentTime;
            } else if (absDiff > softSyncThreshold) {
                // Small drift: soft sync by adjusting playback rate
                // This provides smoother audio without clicks
                const maxRateAdjustment = 0.05; // Max 5% speed adjustment
                let rateAdjustment = timeDiff * 0.1; // Proportional adjustment
                rateAdjustment = Math.max(-maxRateAdjustment, Math.min(maxRateAdjustment, rateAdjustment));
                previewAudio.playbackRate = 1.0 + rateAdjustment;
            } else {
                // In sync: reset playback rate to normal
                previewAudio.playbackRate = 1.0;
            }
        }

        currentTimeDisplay.textContent = formatTime(currentTime);
        scrubber.value = currentTime;
        updatePlayhead();

        // Check if we have a trim region set
        const hasTrimRegion = state.trimEnd > 0 && state.trimEnd < videoPreview.duration;

        if (hasTrimRegion) {
            // Tolerance for comparison (to handle timeupdate imprecision)
            const tolerance = 0.1;

            // Check if playhead started inside trim region (was between In and Out at some point)
            const wasInTrimRegion = state.lastTimeInTrimRegion ||
            (currentTime >= state.trimStart - tolerance && currentTime < state.trimEnd);

            // Track if we're currently in trim region for next iteration
            if (currentTime >= state.trimStart - tolerance && currentTime < state.trimEnd) {
                state.lastTimeInTrimRegion = true;
            }

            // Lock timeline: restrict playback to trim region
            if (state.isTimelineLocked && currentTime >= state.trimEnd - tolerance) {
                if (state.isLooping) {
                    // Loop within trim region
                    videoPreview.currentTime = state.trimStart;
                    if (state.previewAudioPaths) previewAudio.currentTime = state.trimStart;
                    state.lastTimeInTrimRegion = true;
                } else {
                    // Stop at trim end
                    videoPreview.pause();
                    if (state.previewAudioPaths) previewAudio.pause();
                    videoPreview.currentTime = state.trimStart;
                    if (state.previewAudioPaths) previewAudio.currentTime = state.trimStart;
                    state.lastTimeInTrimRegion = true;
                }
            }
            // Loop playback without timeline lock
            else if (state.isLooping && !state.isTimelineLocked) {
                // If we were playing inside trim region and reached/passed trim end, loop to trim start
                if (wasInTrimRegion && currentTime >= state.trimEnd - tolerance) {
                    videoPreview.currentTime = state.trimStart;
                    if (state.previewAudioPaths) previewAudio.currentTime = state.trimStart;
                    state.lastTimeInTrimRegion = true;
                }
                // If we've gone past trim end, we're no longer in trim region
                else if (currentTime >= state.trimEnd) {
                    state.lastTimeInTrimRegion = false;
                }
            }
        }
    }
});

// Loop button
loopBtn.addEventListener('click', () => {
    state.isLooping = !state.isLooping;
    loopBtn.classList.toggle('active', state.isLooping);
});

// Lock Timeline button
lockTimelineBtn.addEventListener('click', () => {
    state.isTimelineLocked = !state.isTimelineLocked;
    lockTimelineBtn.classList.toggle('active', state.isTimelineLocked);
});

// Scrubber
scrubber.addEventListener('input', (e) => {
    state.isScrubbing = true;
    let time = parseFloat(e.target.value);

    // Limit scrubber to trim range when timeline is locked
    if (state.isTimelineLocked) {
        time = clamp(time, state.trimStart, state.trimEnd);
    }

    videoPreview.currentTime = time;
    if (state.previewAudioPaths) previewAudio.currentTime = time;
    currentTimeDisplay.textContent = formatTime(time);
    updatePlayhead();
});

scrubber.addEventListener('change', () => {
    state.isScrubbing = false;

    // Update trim region tracking when user manually changes position
    const hasTrimRegion = state.trimEnd > 0 && state.trimEnd < videoPreview.duration;
    if (hasTrimRegion) {
        const currentTime = videoPreview.currentTime;
        state.lastTimeInTrimRegion = currentTime >= state.trimStart && currentTime < state.trimEnd;
    }
});

function updatePlayhead() {
    if (!videoPreview.duration) return;
    const percent = (videoPreview.currentTime / videoPreview.duration) * 100;
    playhead.style.left = `${percent}%`;
}

// Volume Control (video or separate audio)
function getVolumeTarget() {
    return state.previewAudioPaths ? previewAudio : videoPreview;
}
volumeSlider.addEventListener('input', (e) => {
    const target = getVolumeTarget();
    target.volume = e.target.value / 100;
    if (state.previewAudioPaths) {
        if (e.target.value > 0) target.muted = false;
        updateMuteButton();
    } else if (videoPreview.muted && e.target.value > 0) {
        videoPreview.muted = false;
        updateMuteButton();
    }
});

muteBtn.addEventListener('click', () => {
    const target = getVolumeTarget();
    target.muted = !target.muted;
    updateMuteButton();
});

function updateMuteButton() {
    const volumeOnIcon = document.getElementById('volumeOnIcon');
    const volumeMutedIcon = document.getElementById('volumeMutedIcon');
    const muted = getVolumeTarget().muted;
    if (muted) {
        volumeOnIcon.style.display = 'none';
        volumeMutedIcon.style.display = 'block';
        muteBtn.classList.add('muted');
    } else {
        volumeOnIcon.style.display = 'block';
        volumeMutedIcon.style.display = 'none';
        muteBtn.classList.remove('muted');
    }
}

// Trim Controls
function updateTrimInfo() {
    const duration = state.trimEnd - state.trimStart;

    if (duration === videoPreview.duration && state.trimStart === 0) {
        trimInfo.textContent = 'Trim not set';
    } else {
        trimInfo.textContent = `${formatTime(state.trimStart)} - ${formatTime(state.trimEnd)} (${formatTime(duration)})`;
    }
}

function updateTrimHandles() {
    if (!videoPreview.duration) return;

    const timeline = document.querySelector('.timeline-track');
    const timelineWidth = timeline.offsetWidth;

    // Start handle - positioned by LEFT edge
    const startPercent = (state.trimStart / videoPreview.duration) * 100;
    const startPixels = (startPercent / 100) * timelineWidth;
    trimStartHandle.style.left = `${startPixels}px`;

    // End handle - positioned by RIGHT edge
    const endPercent = (state.trimEnd / videoPreview.duration) * 100;
    const endPixels = (endPercent / 100) * timelineWidth;
    const handleWidth = 12;
    trimEndHandle.style.left = `${endPixels - handleWidth}px`;
}

trimStartInput.addEventListener('change', (e) => {
    const value = parseFloat(e.target.value);
    if (!isNaN(value)) {
        state.trimStart = clamp(value, 0, state.trimEnd - 0.1);
        trimStartInput.value = state.trimStart.toFixed(1);
        updateTrimHandles();
        updateTrimInfo();
        updateFFmpegCommand();
    }
});

trimEndInput.addEventListener('change', (e) => {
    const value = parseFloat(e.target.value);
    if (!isNaN(value)) {
        state.trimEnd = clamp(value, state.trimStart + 0.1, videoPreview.duration);
        trimEndInput.value = state.trimEnd.toFixed(1);
        updateTrimHandles();
        updateTrimInfo();
        updateFFmpegCommand();
    }
});

resetTrimBtn.addEventListener('click', () => {
    state.trimStart = 0;
    state.trimEnd = videoPreview.duration;
    trimStartInput.value = '0.0';
    trimEndInput.value = videoPreview.duration.toFixed(1);
    updateTrimHandles();
    updateTrimInfo();
    updateFFmpegCommand();
});

// Trim Handle Dragging (оптимизированный с RAF и debounce)
let trimDragStartX = 0;
let trimDragStartValue = 0;
let isDraggingTrimRAF = false;
let lastTrimDragEvent = null;

// Debounced FFmpeg command update for performance
const debouncedUpdateFFmpegCommand = debounce(updateFFmpegCommand, 100);

trimStartHandle.addEventListener('mousedown', (e) => {
    state.isDraggingTrim = 'start';
    trimDragStartX = e.clientX;
    trimDragStartValue = state.trimStart;
    e.preventDefault();
});

trimEndHandle.addEventListener('mousedown', (e) => {
    state.isDraggingTrim = 'end';
    trimDragStartX = e.clientX;
    trimDragStartValue = state.trimEnd;
    e.preventDefault();
});

function processTrimDrag() {
    if (!isDraggingTrimRAF || !lastTrimDragEvent) return;

    const e = lastTrimDragEvent;
    const timeline = document.querySelector('.timeline-track');
    const rect = timeline.getBoundingClientRect();
    const deltaX = e.clientX - trimDragStartX;
    const deltaPercent = (deltaX / rect.width) * 100;
    const deltaTime = (deltaPercent / 100) * videoPreview.duration;

    if (state.isDraggingTrim === 'start') {
        state.trimStart = clamp(trimDragStartValue + deltaTime, 0, state.trimEnd - 0.1);
        trimStartInput.value = state.trimStart.toFixed(1);
    } else if (state.isDraggingTrim === 'end') {
        state.trimEnd = clamp(trimDragStartValue + deltaTime, state.trimStart + 0.1, videoPreview.duration);
        trimEndInput.value = state.trimEnd.toFixed(1);
    }

    updateTrimHandles();
    updateTrimInfo();
    debouncedUpdateFFmpegCommand();

    isDraggingTrimRAF = false;
}

document.addEventListener('mousemove', (e) => {
    if (!state.isDraggingTrim || !videoPreview.duration) return;

    lastTrimDragEvent = e;

    if (!isDraggingTrimRAF) {
        isDraggingTrimRAF = true;
        requestAnimationFrame(processTrimDrag);
    }
});

document.addEventListener('mouseup', () => {
    state.isDraggingTrim = null;
    state.isDraggingCrop = null;
    // Hide magnifier and info
    cropMagnifier.classList.remove('active');
    cropMagnifierInfo.classList.remove('active');
    cropHandleCoords.classList.remove('active');
    // Ensure final update happens
    updateFFmpegCommand();
});

// Hide magnifier when cursor leaves the preview area
document.querySelector('.preview-container').addEventListener('mouseleave', () => {
    cropMagnifier.classList.remove('active');
    cropMagnifierInfo.classList.remove('active');
    cropHandleCoords.classList.remove('active');
});

// Window resize handler
window.addEventListener('resize', () => {
    updateTrimHandles();
    if (state.transformEnabled) {
        // Re-calculate preview transform to fit in resized container
        updatePreviewTransform();
        if (state.cropEnabled) {
            updateCropOverlay();
        }
    }
});

// Quick Crop Toggle
quickCropBtn.addEventListener('click', () => {
    // If transform is disabled, enable both transform and crop
    if (!state.transformEnabled) {
        transformToggle.checked = true;
        transformToggle.dispatchEvent(new Event('change'));
        cropToggle.checked = true;
        cropToggle.dispatchEvent(new Event('change'));
    } else {
        // If transform is enabled, just toggle crop
        cropToggle.checked = !cropToggle.checked;
        cropToggle.dispatchEvent(new Event('change'));
    }
});

// Transform Controls
transformToggle.addEventListener('change', (e) => {
    state.transformEnabled = e.target.checked;
    transformControls.style.display = state.transformEnabled ? 'block' : 'none';

    if (state.transformEnabled) {
        updatePreviewTransform();
        
        // Restore crop overlay if crop is enabled
        if (state.cropEnabled) {
            cropOverlay.classList.add('active');
        }
        
        // Automatically switch to Auto codec if any transformations are applied
        if (hasAnyTransformations() && videoCodec.value === 'copy') {
            videoCodec.value = 'auto';
            videoCodec.dispatchEvent(new Event('change'));
        }
    } else {
        // Remove preview transform and crop overlay when disabled
        videoPreview.style.transform = '';
        cropOverlay.classList.remove('active');
        // Also turn off the quickCropBtn when transform is disabled
        quickCropBtn.classList.remove('active');
    }

    updateFFmpegCommand();
});

// Crop Toggle (inside Transform section)
cropToggle.addEventListener('change', (e) => {
    state.cropEnabled = e.target.checked;
    cropControls.style.display = state.cropEnabled ? 'block' : 'none';
    cropOverlay.classList.toggle('active', state.cropEnabled && state.transformEnabled);
    
    // Update quickCropBtn visual state to reflect crop state
    quickCropBtn.classList.toggle('active', state.cropEnabled);

    if (state.cropEnabled) {
        // Update crop input limits based on current rotation
        updateCropInputLimits();
        
        // If crop dimensions are not set or invalid, initialize to full frame
        const effectiveDims = getEffectiveVideoDimensions();
        if (state.crop.width === 0 || state.crop.height === 0 || 
            state.crop.width > effectiveDims.width || state.crop.height > effectiveDims.height) {
            state.crop.x = 0;
            state.crop.y = 0;
            state.crop.width = effectiveDims.width;
            state.crop.height = effectiveDims.height;
            cropX.value = 0;
            cropY.value = 0;
            cropWidth.value = effectiveDims.width;
            cropHeight.value = effectiveDims.height;
        }
        
        updateCropOverlay();
        
        // Switch to encoding if using copy codec
        if (videoCodec.value === 'copy') {
            videoCodec.value = 'auto';
            videoCodec.dispatchEvent(new Event('change'));
        }
    }

    updateFFmpegCommand();
});

function updateCropValues() {
    state.crop.x = parseInt(cropX.value) || 0;
    state.crop.y = parseInt(cropY.value) || 0;
    state.crop.width = parseInt(cropWidth.value) || 0;
    state.crop.height = parseInt(cropHeight.value) || 0;
    updateCropOverlay();
    updateFFmpegCommand();
}

// Reset crop to full frame
function resetCrop() {
    if (!state.cropEnabled || !state.videoMetadata) return;
    
    const effectiveDims = getEffectiveVideoDimensions();
    
    state.crop.x = 0;
    state.crop.y = 0;
    state.crop.width = effectiveDims.width;
    state.crop.height = effectiveDims.height;
    
    cropX.value = 0;
    cropY.value = 0;
    cropWidth.value = effectiveDims.width;
    cropHeight.value = effectiveDims.height;
    
    updateCropOverlay();
    updateFFmpegCommand();
}

// Update magnifier view during crop dragging
function updateMagnifier(clientX, clientY) {
    if (!state.isDraggingCrop || !videoPreview.videoWidth) {
        cropMagnifier.classList.remove('active');
        cropMagnifierInfo.classList.remove('active');
        cropHandleCoords.classList.remove('active');
        return;
    }
    
    // Show magnifier and info
    cropMagnifier.classList.add('active');
    cropMagnifierInfo.classList.add('active');
    
    // Position magnifier near cursor (offset to not block the view)
    const offsetX = 30;
    const offsetY = 30;
    cropMagnifier.style.left = `${clientX + offsetX}px`;
    cropMagnifier.style.top = `${clientY + offsetY}px`;
    
    // Update magnifier size dynamically
    cropMagnifier.style.width = `${MAGNIFIER_SIZE}px`;
    cropMagnifier.style.height = `${MAGNIFIER_SIZE}px`;
    
    // Position info above magnifier - only W and H horizontally
    const infoHeight = 28; // Single line now
    cropMagnifierInfo.style.left = `${clientX + offsetX}px`;
    cropMagnifierInfo.style.top = `${clientY + offsetY - infoHeight - 8}px`;
    
    // Update info text - only width and height, horizontally
    cropMagnifierInfo.textContent = `W: ${state.crop.width}  H: ${state.crop.height}`;
    
    // Get video position and scale
    const videoRect = videoPreview.getBoundingClientRect();
    
    // Video dimensions on screen
    const displayWidth = videoRect.width;
    const displayHeight = videoRect.height;
    
    // Effective (rotated) video dimensions
    const effectiveDims = getEffectiveVideoDimensions();
    const rotatedWidth = effectiveDims.width;
    const rotatedHeight = effectiveDims.height;
    
    // Original video dimensions
    const originalWidth = videoPreview.videoWidth;
    const originalHeight = videoPreview.videoHeight;
    
    // Scale factors (for rotated video)
    const scaleX = rotatedWidth / displayWidth;
    const scaleY = rotatedHeight / displayHeight;
    
    // Calculate the position of the handle/point being dragged in ROTATED video coordinates
    const handle = state.isDraggingCrop;
    let handleRotatedX, handleRotatedY;
    
    // Determine handle position based on what's being dragged (in rotated coordinates)
    if (handle === 'move') {
        // Center of crop area
        handleRotatedX = state.crop.x + state.crop.width / 2;
        handleRotatedY = state.crop.y + state.crop.height / 2;
    } else if (handle === 'nw') {
        handleRotatedX = state.crop.x;
        handleRotatedY = state.crop.y;
    } else if (handle === 'ne') {
        handleRotatedX = state.crop.x + state.crop.width;
        handleRotatedY = state.crop.y;
    } else if (handle === 'sw') {
        handleRotatedX = state.crop.x;
        handleRotatedY = state.crop.y + state.crop.height;
    } else if (handle === 'se') {
        handleRotatedX = state.crop.x + state.crop.width;
        handleRotatedY = state.crop.y + state.crop.height;
    } else if (handle === 'n') {
        handleRotatedX = state.crop.x + state.crop.width / 2;
        handleRotatedY = state.crop.y;
    } else if (handle === 's') {
        handleRotatedX = state.crop.x + state.crop.width / 2;
        handleRotatedY = state.crop.y + state.crop.height;
    } else if (handle === 'e') {
        handleRotatedX = state.crop.x + state.crop.width;
        handleRotatedY = state.crop.y + state.crop.height / 2;
    } else if (handle === 'w') {
        handleRotatedX = state.crop.x;
        handleRotatedY = state.crop.y + state.crop.height / 2;
    }
    
    // Display handle coordinates above the handle (not for 'move')
    if (handle !== 'move') {
        cropHandleCoords.classList.add('active');
        
        // Convert handle position to screen coordinates
        const handleScreenX = videoRect.left + (handleRotatedX / scaleX);
        const handleScreenY = videoRect.top + (handleRotatedY / scaleY);
        
        // Position coords above the handle
        cropHandleCoords.style.left = `${handleScreenX - 30}px`;
        cropHandleCoords.style.top = `${handleScreenY - 30}px`;
        cropHandleCoords.textContent = `X: ${Math.round(handleRotatedX)}  Y: ${Math.round(handleRotatedY)}`;
    } else {
        cropHandleCoords.classList.remove('active');
    }
    
    // Convert handle position from rotated to original video coordinates
    const handleOriginal = rotatedToOriginalCoords(
        handleRotatedX, 
        handleRotatedY, 
        rotatedWidth, 
        rotatedHeight,
        originalWidth,
        originalHeight,
        state.rotation
    );
    
    // Size of the area to capture (in original video pixels)
    const captureSize = MAGNIFIER_SIZE / magnifierZoom;
    
    // Calculate source rectangle (centered on handle position in original coordinates)
    const sx = Math.max(0, Math.min(originalWidth - captureSize, handleOriginal.x - captureSize / 2));
    const sy = Math.max(0, Math.min(originalHeight - captureSize, handleOriginal.y - captureSize / 2));
    
    // Clear canvas
    magnifierCtx.clearRect(0, 0, MAGNIFIER_SIZE, MAGNIFIER_SIZE);
    
    // Save context state
    magnifierCtx.save();
    
    // Apply rotation to canvas around its center
    const centerX = MAGNIFIER_SIZE / 2;
    const centerY = MAGNIFIER_SIZE / 2;
    
    magnifierCtx.translate(centerX, centerY);
    magnifierCtx.rotate((state.rotation * Math.PI) / 180);
    
    // Draw magnified video centered at the rotated origin
    // By drawing at (-centerX, -centerY), the image center will be at (0, 0)
    // which is now the center of the canvas after translation
    try {
        magnifierCtx.drawImage(
            videoPreview,
            sx, sy, captureSize, captureSize,
            -centerX, -centerY, MAGNIFIER_SIZE, MAGNIFIER_SIZE
        );
    } catch (e) {
        // Video not ready yet
        magnifierCtx.restore();
        return;
    }
    
    // Restore context state (removes rotation)
    magnifierCtx.restore();
    
    // Draw center crosshair (always in the center, not rotated)
    const crosshairCenterX = MAGNIFIER_SIZE / 2;
    const crosshairCenterY = MAGNIFIER_SIZE / 2;
    const crosshairSize = 10;
    
    magnifierCtx.strokeStyle = '#ff0066';
    magnifierCtx.lineWidth = 2;
    
    // Horizontal line
    magnifierCtx.beginPath();
    magnifierCtx.moveTo(crosshairCenterX - crosshairSize, crosshairCenterY);
    magnifierCtx.lineTo(crosshairCenterX + crosshairSize, crosshairCenterY);
    magnifierCtx.stroke();
    
    // Vertical line
    magnifierCtx.beginPath();
    magnifierCtx.moveTo(crosshairCenterX, crosshairCenterY - crosshairSize);
    magnifierCtx.lineTo(crosshairCenterX, crosshairCenterY + crosshairSize);
    magnifierCtx.stroke();
    
    // Draw center dot
    magnifierCtx.fillStyle = '#ff0066';
    magnifierCtx.beginPath();
    magnifierCtx.arc(crosshairCenterX, crosshairCenterY, 2, 0, Math.PI * 2);
    magnifierCtx.fill();
}

// Update crop input max values based on current rotation
function updateCropInputLimits() {
    if (!state.videoMetadata || !state.videoMetadata.streams || !state.videoMetadata.streams[0]) {
        return;
    }
    
    const effectiveDims = getEffectiveVideoDimensions();
    cropWidth.max = effectiveDims.width;
    cropHeight.max = effectiveDims.height;
    cropX.max = effectiveDims.width;
    cropY.max = effectiveDims.height;
    
    // Adjust current crop values if they exceed new limits
    if (state.crop.x > effectiveDims.width) {
        state.crop.x = 0;
        cropX.value = 0;
    }
    if (state.crop.y > effectiveDims.height) {
        state.crop.y = 0;
        cropY.value = 0;
    }
    if (state.crop.width > effectiveDims.width) {
        state.crop.width = effectiveDims.width;
        cropWidth.value = effectiveDims.width;
    }
    if (state.crop.height > effectiveDims.height) {
        state.crop.height = effectiveDims.height;
        cropHeight.value = effectiveDims.height;
    }
    
    // Ensure crop area fits within boundaries
    if (state.crop.x + state.crop.width > effectiveDims.width) {
        state.crop.x = effectiveDims.width - state.crop.width;
        cropX.value = state.crop.x;
    }
    if (state.crop.y + state.crop.height > effectiveDims.height) {
        state.crop.y = effectiveDims.height - state.crop.height;
        cropY.value = state.crop.y;
    }
}

cropX.addEventListener('change', updateCropValues);
cropY.addEventListener('change', updateCropValues);
cropWidth.addEventListener('change', updateCropValues);
cropHeight.addEventListener('change', updateCropValues);

// Helper function to check if any transformations are applied
function hasAnyTransformations() {
    const hasCrop = state.cropEnabled && (
        state.crop.width !== state.crop.originalWidth || 
        state.crop.height !== state.crop.originalHeight ||
        state.crop.x !== 0 || 
        state.crop.y !== 0
    );
    const hasScale = state.scale.width !== -1 || state.scale.height !== -1;
    return hasCrop || state.rotation !== 0 || state.flipHorizontal || state.flipVertical || hasScale;
}

// Transform Controls
rotateLeftBtn.addEventListener('click', () => {
    const previousRotation = state.rotation;
    state.rotation = (state.rotation - 90 + 360) % 360;
    updateTransformUI();
    updatePreviewTransform();
    
    // Reset crop to full frame when rotation changes to avoid confusion
    // (crop coordinates are relative to rotated video, so changing rotation
    // would make the crop area appear in wrong place)
    if (state.cropEnabled && previousRotation !== state.rotation) {
        const effectiveDims = getEffectiveVideoDimensions();
        state.crop.x = 0;
        state.crop.y = 0;
        state.crop.width = effectiveDims.width;
        state.crop.height = effectiveDims.height;
        cropX.value = 0;
        cropY.value = 0;
        cropWidth.value = effectiveDims.width;
        cropHeight.value = effectiveDims.height;
        updateCropInputLimits();
        updateCropOverlay();
    }
    
    // Enable transform mode if not already enabled
    if (!state.transformEnabled) {
        transformToggle.checked = true;
        transformToggle.dispatchEvent(new Event('change'));
        return;
    }
    
    // Switch to encoding if using copy codec
    if (videoCodec.value === 'copy') {
        videoCodec.value = 'auto';
        videoCodec.dispatchEvent(new Event('change'));
    }
    
    updateFFmpegCommand();
});

rotateRightBtn.addEventListener('click', () => {
    const previousRotation = state.rotation;
    state.rotation = (state.rotation + 90) % 360;
    updateTransformUI();
    updatePreviewTransform();
    
    // Reset crop to full frame when rotation changes to avoid confusion
    // (crop coordinates are relative to rotated video, so changing rotation
    // would make the crop area appear in wrong place)
    if (state.cropEnabled && previousRotation !== state.rotation) {
        const effectiveDims = getEffectiveVideoDimensions();
        state.crop.x = 0;
        state.crop.y = 0;
        state.crop.width = effectiveDims.width;
        state.crop.height = effectiveDims.height;
        cropX.value = 0;
        cropY.value = 0;
        cropWidth.value = effectiveDims.width;
        cropHeight.value = effectiveDims.height;
        updateCropInputLimits();
        updateCropOverlay();
    }
    
    // Enable transform mode if not already enabled
    if (!state.transformEnabled) {
        transformToggle.checked = true;
        transformToggle.dispatchEvent(new Event('change'));
        return;
    }
    
    // Switch to encoding if using copy codec
    if (videoCodec.value === 'copy') {
        videoCodec.value = 'auto';
        videoCodec.dispatchEvent(new Event('change'));
    }
    
    updateFFmpegCommand();
});

flipHorizontalBtn.addEventListener('click', () => {
    state.flipHorizontal = !state.flipHorizontal;
    updateTransformUI();
    updatePreviewTransform();
    
    // Enable transform mode if not already enabled
    if (!state.transformEnabled) {
        transformToggle.checked = true;
        transformToggle.dispatchEvent(new Event('change'));
        return;
    }
    
    // Switch to encoding if using copy codec
    if (videoCodec.value === 'copy') {
        videoCodec.value = 'auto';
        videoCodec.dispatchEvent(new Event('change'));
    }
    
    updateFFmpegCommand();
});

flipVerticalBtn.addEventListener('click', () => {
    state.flipVertical = !state.flipVertical;
    updateTransformUI();
    updatePreviewTransform();
    
    // Enable transform mode if not already enabled
    if (!state.transformEnabled) {
        transformToggle.checked = true;
        transformToggle.dispatchEvent(new Event('change'));
        return;
    }
    
    // Switch to encoding if using copy codec
    if (videoCodec.value === 'copy') {
        videoCodec.value = 'auto';
        videoCodec.dispatchEvent(new Event('change'));
    }
    
    updateFFmpegCommand();
});

function updateTransformUI() {
    // Update button states - only show active when that specific transformation is applied
    const hasRotation = state.rotation !== 0;
    rotateLeftBtn.classList.toggle('active', hasRotation);
    rotateRightBtn.classList.toggle('active', hasRotation);
    
    flipHorizontalBtn.classList.toggle('active', state.flipHorizontal);
    flipVerticalBtn.classList.toggle('active', state.flipVertical);
}

function updatePreviewTransform() {
    if (!state.transformEnabled) {
        videoPreview.style.transform = '';
        videoPreview.style.width = '';
        videoPreview.style.height = '';
        return;
    }

    let transforms = [];
    
    // Get effective dimensions considering rotation
    const effectiveDims = getEffectiveVideoDimensions();
    const isRotated90or270 = state.rotation === 90 || state.rotation === 270;
    
    // For 90/270 degree rotations, we need to adjust the preview size
    // to properly fit the swapped dimensions
    if (isRotated90or270 && state.videoMetadata) {
        const originalWidth = state.videoMetadata.streams[0].width;
        const originalHeight = state.videoMetadata.streams[0].height;
        
        // Calculate the scale factor to fit the rotated video in the container
        const container = document.querySelector('.preview-container');
        if (container) {
            const containerWidth = container.clientWidth;
            const containerHeight = container.clientHeight;
            
            // After rotation, height becomes width and width becomes height
            const rotatedWidth = originalHeight;
            const rotatedHeight = originalWidth;
            
            // Calculate scale to fit in container
            const scaleToFit = Math.min(
                containerWidth / rotatedWidth,
                containerHeight / rotatedHeight
            );
            
            // Set the video preview size before rotation so it fits correctly after rotation
            videoPreview.style.width = `${originalWidth * scaleToFit}px`;
            videoPreview.style.height = `${originalHeight * scaleToFit}px`;
        }
    } else {
        // For 0/180 degree rotations, use default sizing
        videoPreview.style.width = '';
        videoPreview.style.height = '';
    }
    
    // Rotation
    if (state.rotation !== 0) {
        transforms.push(`rotate(${state.rotation}deg)`);
    }
    
    // Flip
    let scaleX = state.flipHorizontal ? -1 : 1;
    let scaleY = state.flipVertical ? -1 : 1;
    
    if (scaleX !== 1 || scaleY !== 1) {
        transforms.push(`scale(${scaleX}, ${scaleY})`);
    }
    
    videoPreview.style.transform = transforms.join(' ');
    
    // Update crop overlay if crop is enabled
    if (state.cropEnabled) {
        updateCropOverlay();
    }
}

// Scale Controls
function updateScaleValues() {
    state.scale.width = parseInt(scaleWidth.value) || -1;
    state.scale.height = parseInt(scaleHeight.value) || -1;
    updateFFmpegCommand();
}

scaleWidth.addEventListener('change', updateScaleValues);
scaleHeight.addEventListener('change', updateScaleValues);

// Scale presets
scalePresets.forEach(btn => {
    btn.addEventListener('click', () => {
        const width = parseInt(btn.dataset.width);
        const height = parseInt(btn.dataset.height);
        
        if (width === -1 && height === -1) {
            // Reset
            scaleWidth.value = '';
            scaleHeight.value = '';
            state.scale.width = -1;
            state.scale.height = -1;
        } else {
            scaleWidth.value = width;
            scaleHeight.value = height;
            state.scale.width = width;
            state.scale.height = height;
        }
        
        // Enable transform mode if not already enabled
        if (!state.transformEnabled && (state.scale.width !== -1 || state.scale.height !== -1)) {
            transformToggle.checked = true;
            transformToggle.dispatchEvent(new Event('change'));
            return;
        }
        
        // Switch to encoding if using copy codec
        if ((state.scale.width !== -1 || state.scale.height !== -1) && videoCodec.value === 'copy') {
            videoCodec.value = 'auto';
            videoCodec.dispatchEvent(new Event('change'));
        }
        
        updateFFmpegCommand();
    });
});

function updateCropOverlay() {
    if (!state.cropEnabled) return;

    const videoRect = videoPreview.getBoundingClientRect();
    const containerRect = document.querySelector('.preview-container').getBoundingClientRect();

    // Use effective dimensions considering rotation
    const effectiveDims = getEffectiveVideoDimensions();
    const videoWidth = effectiveDims.width;
    const videoHeight = effectiveDims.height;

    const scaleX = videoRect.width / videoWidth;
    const scaleY = videoRect.height / videoHeight;

    const offsetX = (containerRect.width - videoRect.width) / 2;
    const offsetY = (containerRect.height - videoRect.height) / 2;

    const cropHandles = document.querySelector('.crop-handles');
    cropHandles.style.left = `${offsetX + state.crop.x * scaleX}px`;
    cropHandles.style.top = `${offsetY + state.crop.y * scaleY}px`;
    cropHandles.style.width = `${state.crop.width * scaleX}px`;
    cropHandles.style.height = `${state.crop.height * scaleY}px`;
}

// Crop Handle Dragging (оптимизированный)
let cropDragStartX = 0;
let cropDragStartY = 0;
let cropDragStartState = null;
let isDraggingCropRAF = false;
let lastCropDragEvent = null;

// Handle dragging crop handles (corners and edges)
document.querySelectorAll('.crop-handle').forEach(handle => {
    handle.addEventListener('mousedown', (e) => {
        e.preventDefault();
        e.stopPropagation();

        const handleClass = Array.from(handle.classList).find(c => c.startsWith('crop-handle-'));
        state.isDraggingCrop = handleClass.replace('crop-handle-', '');

        cropDragStartX = e.clientX;
        cropDragStartY = e.clientY;
        cropDragStartState = {
            x: state.crop.x,
            y: state.crop.y,
            width: state.crop.width,
            height: state.crop.height
        };
    });
});

// Handle dragging the entire crop area (move)
document.querySelector('.crop-handles').addEventListener('mousedown', (e) => {
    // Only handle if clicking on the crop-handles div itself, not on the handle buttons
    if (e.target.classList.contains('crop-handle')) {
        return; // Let the handle's own event handler deal with it
    }

    e.preventDefault();
    e.stopPropagation();

    state.isDraggingCrop = 'move';

    cropDragStartX = e.clientX;
    cropDragStartY = e.clientY;
    cropDragStartState = {
        x: state.crop.x,
        y: state.crop.y,
        width: state.crop.width,
        height: state.crop.height
    };
});

// Double click to reset crop to full frame
document.querySelector('.crop-handles').addEventListener('dblclick', (e) => {
    e.preventDefault();
    e.stopPropagation();
    resetCrop();
});

// Right click context menu for crop reset
document.querySelector('.crop-handles').addEventListener('contextmenu', (e) => {
    e.preventDefault();
    e.stopPropagation();
    
    // Remove any existing menu
    const existingMenu = document.querySelector('.crop-context-menu');
    if (existingMenu) {
        existingMenu.remove();
    }
    
    const menu = document.createElement('div');
    menu.className = 'crop-context-menu';
    menu.style.cssText = `
        position: fixed;
        left: ${e.clientX}px;
        top: ${e.clientY}px;
        background: #2a2a2a;
        border: 1px solid #444;
        border-radius: 4px;
        padding: 4px 0;
        z-index: 10000;
        box-shadow: 0 2px 8px rgba(0,0,0,0.3);
        min-width: 120px;
    `;
    
    const resetItem = document.createElement('div');
    resetItem.textContent = 'Reset';
    resetItem.style.cssText = `
        padding: 8px 16px;
        cursor: pointer;
        color: #fff;
        font-size: 13px;
    `;
    resetItem.addEventListener('mouseenter', () => {
        resetItem.style.background = '#3a3a3a';
    });
    resetItem.addEventListener('mouseleave', () => {
        resetItem.style.background = 'transparent';
    });
    resetItem.addEventListener('click', (evt) => {
        evt.stopPropagation();
        resetCrop();
        menu.remove();
        document.removeEventListener('click', closeMenu);
        document.removeEventListener('contextmenu', closeMenu);
        document.removeEventListener('keydown', handleEscape);
    });
    
    menu.appendChild(resetItem);
    document.body.appendChild(menu);
    
    // Close menu on any click outside
    const closeMenu = (evt) => {
        if (!menu.contains(evt.target)) {
            menu.remove();
            document.removeEventListener('click', closeMenu);
            document.removeEventListener('contextmenu', closeMenu);
            document.removeEventListener('keydown', handleEscape);
        }
    };
    
    // Close menu on Escape key
    const handleEscape = (evt) => {
        if (evt.key === 'Escape') {
            menu.remove();
            document.removeEventListener('click', closeMenu);
            document.removeEventListener('contextmenu', closeMenu);
            document.removeEventListener('keydown', handleEscape);
        }
    };
    
    // Add listeners with a small delay to prevent immediate closing
    setTimeout(() => {
        document.addEventListener('click', closeMenu);
        document.addEventListener('contextmenu', closeMenu);
        document.addEventListener('keydown', handleEscape);
    }, 10);
});

function processCropDrag() {
    if (!isDraggingCropRAF || !lastCropDragEvent) return;

    const e = lastCropDragEvent;
    const videoRect = videoPreview.getBoundingClientRect();
    
    // Use effective dimensions considering rotation
    const effectiveDims = getEffectiveVideoDimensions();
    const videoWidth = effectiveDims.width;
    const videoHeight = effectiveDims.height;

    const scaleX = videoWidth / videoRect.width;
    const scaleY = videoHeight / videoRect.height;

    // Apply sensitivity reduction when Shift is held
    const sensitivity = e.shiftKey ? 0.2 : 1.0;

    const deltaX = Math.round((e.clientX - cropDragStartX) * scaleX * sensitivity);
    const deltaY = Math.round((e.clientY - cropDragStartY) * scaleY * sensitivity);

    const handle = state.isDraggingCrop;

    // Move entire crop area
    if (handle === 'move') {
        const newX = cropDragStartState.x + deltaX;
        const newY = cropDragStartState.y + deltaY;

        // Clamp to video boundaries
        state.crop.x = clamp(newX, 0, videoWidth - cropDragStartState.width);
        state.crop.y = clamp(newY, 0, videoHeight - cropDragStartState.height);
        // Width and height stay the same
        state.crop.width = cropDragStartState.width;
        state.crop.height = cropDragStartState.height;
    }
    // Corner handles
    else if (handle === 'nw') {
        state.crop.x = clamp(cropDragStartState.x + deltaX, 0, cropDragStartState.x + cropDragStartState.width - 10);
        state.crop.y = clamp(cropDragStartState.y + deltaY, 0, cropDragStartState.y + cropDragStartState.height - 10);
        state.crop.width = cropDragStartState.width - (state.crop.x - cropDragStartState.x);
        state.crop.height = cropDragStartState.height - (state.crop.y - cropDragStartState.y);
    } else if (handle === 'ne') {
        state.crop.y = clamp(cropDragStartState.y + deltaY, 0, cropDragStartState.y + cropDragStartState.height - 10);
        state.crop.width = clamp(cropDragStartState.width + deltaX, 10, videoWidth - cropDragStartState.x);
        state.crop.height = cropDragStartState.height - (state.crop.y - cropDragStartState.y);
    } else if (handle === 'sw') {
        state.crop.x = clamp(cropDragStartState.x + deltaX, 0, cropDragStartState.x + cropDragStartState.width - 10);
        state.crop.width = cropDragStartState.width - (state.crop.x - cropDragStartState.x);
        state.crop.height = clamp(cropDragStartState.height + deltaY, 10, videoHeight - cropDragStartState.y);
    } else if (handle === 'se') {
        state.crop.width = clamp(cropDragStartState.width + deltaX, 10, videoWidth - cropDragStartState.x);
        state.crop.height = clamp(cropDragStartState.height + deltaY, 10, videoHeight - cropDragStartState.y);
    }
    // Edge handles
    else if (handle === 'n') {
        state.crop.y = clamp(cropDragStartState.y + deltaY, 0, cropDragStartState.y + cropDragStartState.height - 10);
        state.crop.height = cropDragStartState.height - (state.crop.y - cropDragStartState.y);
    } else if (handle === 's') {
        state.crop.height = clamp(cropDragStartState.height + deltaY, 10, videoHeight - cropDragStartState.y);
    } else if (handle === 'w') {
        state.crop.x = clamp(cropDragStartState.x + deltaX, 0, cropDragStartState.x + cropDragStartState.width - 10);
        state.crop.width = cropDragStartState.width - (state.crop.x - cropDragStartState.x);
    } else if (handle === 'e') {
        state.crop.width = clamp(cropDragStartState.width + deltaX, 10, videoWidth - cropDragStartState.x);
    }

    // Update inputs
    cropX.value = state.crop.x;
    cropY.value = state.crop.y;
    cropWidth.value = state.crop.width;
    cropHeight.value = state.crop.height;

    updateCropOverlay();
    updateFFmpegCommand();

    isDraggingCropRAF = false;
}

document.addEventListener('mousemove', (e) => {
    if (!state.isDraggingCrop || !state.videoMetadata) {
        return;
    }

    lastCropDragEvent = e;
    
    // Update magnifier
    updateMagnifier(e.clientX, e.clientY);

    if (!isDraggingCropRAF) {
        isDraggingCropRAF = true;
        requestAnimationFrame(processCropDrag);
    }
});

// Mouse wheel event for magnifier zoom control and size adjustment
document.addEventListener('wheel', (e) => {
    // Only handle wheel events when magnifier is active
    if (!state.isDraggingCrop || !cropMagnifier.classList.contains('active')) {
        return;
    }
    
    e.preventDefault();
    
    // Ctrl+Wheel: Change magnifier window size
    if (e.ctrlKey) {
        const sizeDelta = e.deltaY > 0 ? -10 : 10;
        MAGNIFIER_SIZE = Math.max(100, Math.min(400, MAGNIFIER_SIZE + sizeDelta));
        
        // Update canvas size
        magnifierCanvas.width = MAGNIFIER_SIZE;
        magnifierCanvas.height = MAGNIFIER_SIZE;
    } 
    // Normal Wheel: Change zoom level
    else {
        const delta = e.deltaY > 0 ? -0.5 : 0.5;
        magnifierZoom = Math.max(2, Math.min(15, magnifierZoom + delta));
    }
    
    // Update magnifier with new zoom level or size
    if (lastCropDragEvent) {
        updateMagnifier(lastCropDragEvent.clientX, lastCropDragEvent.clientY);
    }
}, { passive: false });

// Export Settings
videoCodec.addEventListener('change', (e) => {
    state.exportSettings.videoCodec = e.target.value;

    // Update visibility of quality/preset/profile based on codec
    const codec = e.target.value;
    const isCopy = codec === 'copy';
    const isAuto = codec === 'auto';

    // Hide all encoding-specific settings when copy is selected
    const qualityGroup = qualityMode.closest('.control-group');
    const crfGroup = crfSettings.closest('.control-group') || crfSettings;
    const bitrateGroup = bitrateSettings.closest('.control-group') || bitrateSettings;

    if (qualityGroup) qualityGroup.style.display = (isCopy || isAuto) ? 'none' : 'block';
    if (crfSettings) crfSettings.style.display = ((isCopy || isAuto) || state.exportSettings.qualityMode !== 'crf') ? 'none' : 'block';
    if (bitrateSettings) bitrateSettings.style.display = ((isCopy || isAuto) || state.exportSettings.qualityMode !== 'bitrate') ? 'none' : 'block';

    // Update preset visibility
    const supportsPreset = codec.includes('264') || codec.includes('265') ||
    codec.includes('libvpx') || codec.includes('libaom');
    presetGroup.style.display = (supportsPreset && !isCopy && !isAuto) ? 'block' : 'none';

    // Update pixel format visibility - show for all encoding codecs
    pixelFormatGroup.style.display = (!isCopy && !isAuto) ? 'block' : 'none';

    // Update available pixel formats based on codec
    if (!isCopy && !isAuto) {
        updatePixelFormatOptions(codec);
    }

    updateFFmpegCommand();
});

audioCodec.addEventListener('change', (e) => {
    state.exportSettings.audioCodec = e.target.value;

    // Hide audio bitrate when copy is selected
    const isCopy = e.target.value === 'copy';
    const audioBitrateGroup = audioBitrate.closest('.control-group');
    if (audioBitrateGroup) {
        audioBitrateGroup.style.display = isCopy ? 'none' : 'block';
    }

    updateFFmpegCommand();
});

qualityMode.addEventListener('change', (e) => {
    state.exportSettings.qualityMode = e.target.value;
    const isCRF = e.target.value === 'crf';
    crfSettings.style.display = isCRF ? 'block' : 'none';
    bitrateSettings.style.display = isCRF ? 'none' : 'block';
    updateFFmpegCommand();
});

crfSlider.addEventListener('input', (e) => {
    const value = parseInt(e.target.value);
    crfValue.value = value;
    state.exportSettings.crf = value;
    updateFFmpegCommand();
});

crfValue.addEventListener('change', (e) => {
    const value = clamp(parseInt(e.target.value), 0, 51);
    crfSlider.value = value;
    crfValue.value = value;
    state.exportSettings.crf = value;
    updateFFmpegCommand();
});

videoBitrate.addEventListener('change', (e) => {
    state.exportSettings.videoBitrate = parseInt(e.target.value);
    updateFFmpegCommand();
});

audioBitrate.addEventListener('change', (e) => {
    state.exportSettings.audioBitrate = parseInt(e.target.value);
    updateFFmpegCommand();
});

encoderPreset.addEventListener('change', (e) => {
    state.exportSettings.preset = e.target.value;
    updateFFmpegCommand();
});

pixelFormat.addEventListener('change', (e) => {
    state.exportSettings.pixelFormat = e.target.value;
    updateFFmpegCommand();
});

container.addEventListener('change', (e) => {
    state.exportSettings.container = e.target.value;
    updateFFmpegCommand();
});

// Switch preview to selected audio track (split preview: video + separate audio files)
function applyAudioTrackToPreview() {
    if (!state.previewAudioPaths) return;

    const idx = state.selectedAudioTrack;
    const wasPlaying = state.isPlaying;
    const currentTime = videoPreview.currentTime;

    // Pause audio during switch to prevent glitches
    if (wasPlaying) {
        previewAudio.pause();
    }

    // Switch audio source
    previewAudio.src = filePathToUrl(state.previewAudioPaths[idx]);
    previewAudio.volume = volumeSlider.value / 100;

    // Wait for audio to load before syncing and playing
    previewAudio.addEventListener('loadedmetadata', function onLoaded() {
        previewAudio.removeEventListener('loadedmetadata', onLoaded);

        // Sync to current video time
        previewAudio.currentTime = currentTime;

        // Resume playback if was playing
        if (wasPlaying) {
            // Small delay to ensure audio is ready
            setTimeout(() => {
                previewAudio.play().catch(err => {
                    console.warn('Audio play failed after track switch:', err);
                });
            }, 50);
        }
    }, { once: true });
}

// Audio Track Selection for Preview
audioTrackSelect.addEventListener('change', (e) => {
    const value = e.target.value;
    if (value === 'none') {
        // No sound selected - mute or stop audio
        if (state.previewAudioPaths) {
            previewAudio.pause();
            previewAudio.src = '';
        }
    } else {
        state.selectedAudioTrack = parseInt(value);
        applyAudioTrackToPreview();
    }
});

// Audio Track Selection for Export
audioExportTrack.addEventListener('change', (e) => {
    const value = e.target.value;
    state.exportSettings.audioTrack = value;

    // Show/hide track picker based on selection
    if (value === 'pick-export') {
        audioTrackPickerGroup.style.display = 'block';
        audioTrackPickerLabel.textContent = 'Select tracks to export';

        // Clear all checkboxes
        audioTrackPicker.querySelectorAll('input[type="checkbox"]').forEach(cb => {
            cb.checked = false;
        });
        state.exportSettings.selectedAudioTracks = [];
    } else if (value === 'pick-merge') {
        audioTrackPickerGroup.style.display = 'block';
        audioTrackPickerLabel.textContent = 'Select tracks to merge';

        // Clear all checkboxes
        audioTrackPicker.querySelectorAll('input[type="checkbox"]').forEach(cb => {
            cb.checked = false;
        });
        state.exportSettings.selectedAudioTracks = [];
    } else {
        audioTrackPickerGroup.style.display = 'none';
        state.exportSettings.selectedAudioTracks = [];
    }

    updateFFmpegCommand();
});

ffmpegCommand.addEventListener('change', (e) => {
    state.exportSettings.customCommand = e.target.value;
});

// Export Video
exportBtn.addEventListener('click', async () => {
    if (!state.videoPath) return;

    const ext = state.exportSettings.container;
    const defaultName = `edited_video.${ext}`;

    const outputPath = await window.electronAPI.selectOutput(defaultName);
    if (!outputPath) return;

    const options = {
        inputPath: state.videoPath,
        outputPath: outputPath,
        customCommand: state.exportSettings.customCommand,
        trimStart: state.trimStart,
        trimEnd: state.trimEnd,
        cropEnabled: state.cropEnabled,
        crop: state.cropEnabled ? state.crop : null
    };

    let isCancelled = false;

    try {
        exportBtn.disabled = true;
        exportBtn.textContent = 'Exporting...';

        // Create modal progress dialog
        const exportModal = document.createElement('div');
        exportModal.id = 'exportModal';
        exportModal.style.cssText = 'position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); background: rgba(0,0,0,0.95); padding: 30px; border-radius: 16px; z-index: 10000; text-align: center; color: white; min-width: 350px; box-shadow: 0 20px 40px rgba(0,0,0,0.5);';
        exportModal.innerHTML = `
            <div style="font-size: 20px; margin-bottom: 15px; font-weight: 600;">Exporting Video</div>
            <div class="progress-bar" style="height: 8px; background: rgba(255,255,255,0.2); border-radius: 4px; overflow: hidden; margin-bottom: 15px;">
                <div class="progress-fill" id="modalProgressFill" style="height: 100%; background: linear-gradient(90deg, var(--color-primary), var(--color-secondary)); border-radius: 4px; transition: width 0.3s ease; box-shadow: 0 0 10px rgba(0, 217, 255, 0.5); width: 0%;"></div>
            </div>
            <div class="progress-text" id="modalProgressText" style="font-size: 16px; font-weight: 500; margin-bottom: 20px;">Preparing export...</div>
            <button id="cancelExportBtn" style="background: #ff4444; color: white; border: none; padding: 10px 20px; border-radius: 8px; cursor: pointer; font-size: 14px; font-weight: 500;">Cancel Export</button>
        `;
        document.body.appendChild(exportModal);

        // Add cancel button handler
        const cancelBtn = document.getElementById('cancelExportBtn');
        cancelBtn.addEventListener('click', async () => {
            try {
                isCancelled = true;
                await window.electronAPI.cancelExport();
                cancelBtn.disabled = true;
                cancelBtn.textContent = 'Cancelling...';
                document.getElementById('modalProgressText').textContent = 'Cancelling export...';
            } catch (error) {
                console.error('Failed to cancel export:', error);
            }
        });

        const result = await window.electronAPI.exportVideo(options);

        // Handle cancellation
        if (result && result.cancelled) {
            isCancelled = true;
            return; // Exit early, modal will be closed by progress handler
        }

        // Success
        if (!isCancelled) {
            document.getElementById('modalProgressFill').style.width = '100%';
            document.getElementById('modalProgressText').textContent = 'Export completed successfully!';

            // Replace cancel button with OK button
            const cancelBtn = document.getElementById('cancelExportBtn');
            cancelBtn.textContent = 'OK';
            cancelBtn.style.background = '#00d4aa';
            cancelBtn.removeEventListener('click', cancelBtn.onclick); // Remove cancel handler
            cancelBtn.addEventListener('click', () => {
                exportModal.remove();
            });
        }
    } catch (error) {
        console.error('Export error:', error);
        const exportModal = document.getElementById('exportModal');
        if (exportModal) {
            exportModal.remove();
        }

        // Don't show error if export was cancelled or if it's an IPC timeout error
        if (!isCancelled && !isCancellationError(error)) {
            alert('Export error: ' + error.message);
        }
    } finally {
        exportBtn.disabled = false;
        exportBtn.innerHTML = `
        <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
        <path d="M10 14V6M10 6L7 9M10 6L13 9" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
        <path d="M4 13V15C4 15.5304 4.21071 16.0391 4.58579 16.4142C4.96086 16.7893 5.46957 17 6 17H14C14.5304 17 15.0391 16.7893 15.4142 16.4142C15.7893 16.0391 16 15.5304 16 15V13" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        </svg>
        Export video
        `;
    }
});

// Export Progress
window.electronAPI.onExportProgress((progress) => {
    if (progress.status === 'cancelled') {
        // Handle cancellation
        const exportModal = document.getElementById('exportModal');
        if (exportModal) {
            const cancelBtn = document.getElementById('cancelExportBtn');
            const progressText = document.getElementById('modalProgressText');
            if (cancelBtn) cancelBtn.style.display = 'none';
            if (progressText) progressText.textContent = 'Export cancelled';
            setTimeout(() => {
                exportModal.remove();
                // Restore export button state
                exportBtn.disabled = false;
                exportBtn.innerHTML = `
                <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
                <path d="M10 14V6M10 6L7 9M10 6L13 9" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                <path d="M4 13V15C4 15.5304 4.21071 16.0391 4.58579 16.4142C4.96086 16.7893 5.46957 17 6 17H14C14.5304 17 15.0391 16.7893 15.4142 16.4142C15.7893 16.0391 16 15.5304 16 15V13" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                </svg>
                Export video
                `;
            }, 1500);
        }
    } else if (progress.percent) {
        const percent = Math.round(progress.percent);
        const progressFill = document.getElementById('modalProgressFill');
        const progressText = document.getElementById('modalProgressText');
        if (progressFill) progressFill.style.width = `${percent}%`;
        if (progressText) progressText.textContent = `Exporting... ${percent}%`;
    }
});

// Transcode Progress (for unsupported codec preview creation)
window.electronAPI.onTranscodeProgress((data) => {
    const statusDiv = document.getElementById('transcodeStatus');
    const cancelBtn = document.getElementById('cancelTranscodeBtn');
    if (!statusDiv) return;

    if (data.status === 'started') {
        statusDiv.textContent = `Creating preview with ${data.encoder}...`;
        if (cancelBtn) {
            cancelBtn.style.display = 'inline-block';
            cancelBtn.disabled = false;
            cancelBtn.textContent = 'Cancel';
        }
    } else if (data.status === 'progress') {
        statusDiv.textContent = `Creating preview: ${data.percent}%`;
        if (cancelBtn) {
            cancelBtn.style.display = 'inline-block';
        }
    } else if (data.status === 'completed') {
        statusDiv.textContent = 'Preview ready!';
        if (cancelBtn) {
            cancelBtn.style.display = 'none';
        }
    } else if (data.status === 'cancelled') {
        statusDiv.textContent = 'Preview creation cancelled';
        if (cancelBtn) {
            cancelBtn.style.display = 'none';
        }
        // Remove loading message after a short delay
        setTimeout(() => {
            const msg = document.getElementById('loadingMessage');
            if (msg) msg.remove();
        }, 1500);
    } else if (data.status === 'error') {
        statusDiv.textContent = `Error: ${data.error}`;
        if (cancelBtn) {
            cancelBtn.style.display = 'none';
        }
        // Remove loading message after a short delay
        setTimeout(() => {
            const msg = document.getElementById('loadingMessage');
            if (msg) msg.remove();
        }, 2000);
    }
});

// Menu handling
const fileMenu = document.getElementById('fileMenu');
const editMenu = document.getElementById('editMenu');
const viewMenu = document.getElementById('viewMenu');
const helpMenu = document.getElementById('helpMenu');
const presetsMenuElement = document.getElementById('presetsMenu');

// File menu
fileMenu.addEventListener('click', (e) => {
    e.stopPropagation();
    const isActive = fileMenu.classList.contains('active');
    closeAllMenus();
    if (!isActive) {
        fileMenu.classList.add('active');
    }
});

// Edit menu
editMenu.addEventListener('click', (e) => {
    e.stopPropagation();
    const isActive = editMenu.classList.contains('active');
    closeAllMenus();
    if (!isActive) {
        editMenu.classList.add('active');
    }
});

// View menu
viewMenu.addEventListener('click', (e) => {
    e.stopPropagation();
    const isActive = viewMenu.classList.contains('active');
    closeAllMenus();
    if (!isActive) {
        viewMenu.classList.add('active');
    }
});

// Help menu
helpMenu.addEventListener('click', (e) => {
    e.stopPropagation();
    const isActive = helpMenu.classList.contains('active');
    closeAllMenus();
    if (!isActive) {
        helpMenu.classList.add('active');
    }
});

// Presets menu
presetsMenuElement.addEventListener('click', (e) => {
    e.stopPropagation();
    const isActive = presetsMenuElement.classList.contains('active');
    closeAllMenus();
    if (!isActive) {
        presetsMenuElement.classList.add('active');
    }
});

// Menu options
document.getElementById('menuExport').addEventListener('click', () => {
    closeAllMenus();
    if (state.videoPath) {
        exportBtn.click();
    }
});

document.getElementById('menuQuit').addEventListener('click', () => {
    closeAllMenus();
    if (confirm('Are you sure you want to quit?')) {
        window.close();
    }
});

document.getElementById('menuResetTrim').addEventListener('click', () => {
    closeAllMenus();
    if (state.videoPath) {
        resetTrimBtn.click();
    }
});

document.getElementById('menuFullscreen').addEventListener('click', () => {
    closeAllMenus();
    toggleFullscreen();
});

document.getElementById('menuPreviewFullscreen').addEventListener('click', () => {
    closeAllMenus();
    toggleVideoFullscreen();
});

document.getElementById('menuShortcuts').addEventListener('click', () => {
    closeAllMenus();
    const m = modifierLabel;
    alert(`Keyboard Shortcuts:

    ${m}+N - Import Video
    ${m}+S - Export Video
    ${m}+Q - Quit
    ${m}+F - Video Fullscreen
    F11 - Application Fullscreen

    Space - Play/Pause
    ← → - Previous/Next frame
    , . - Previous/Next frame

    Q - Set trim start
    E - Set trim end
    R - Reset Trim
    C - Toggle crop
    A - Toggle loop
    Shift+A - Toggle timeline lock`);
});

// Settings dialog
let contextMenuEnabled = false;

async function loadSettingsDialog() {
    try {
        // Get platform info
        const infoResult = await window.electronAPI.contextMenu.getInfo();
        if (infoResult.success) {
            const description = document.getElementById('contextMenuDescription');
            description.textContent = infoResult.description;
        }

        // Get current status
        const statusResult = await window.electronAPI.contextMenu.isEnabled();
        if (statusResult.success) {
            contextMenuEnabled = statusResult.enabled;
            document.getElementById('contextMenuToggle').checked = contextMenuEnabled;
        }
    } catch (err) {
        console.error('Error loading settings:', err);
    }
}

function showSettingsDialog() {
    document.getElementById('settingsDialog').style.display = 'flex';
    loadSettingsDialog();
}

function hideSettingsDialog() {
    document.getElementById('settingsDialog').style.display = 'none';
    document.getElementById('contextMenuStatus').style.display = 'none';
}

function showSettingsStatus(message, isError = false) {
    const status = document.getElementById('contextMenuStatus');
    status.textContent = message;
    status.className = 'settings-status ' + (isError ? 'error' : 'success');
    status.style.display = 'block';

    // Hide after 5 seconds
    setTimeout(() => {
        status.style.display = 'none';
    }, 5000);
}

document.getElementById('menuSettings').addEventListener('click', () => {
    closeAllMenus();
    showSettingsDialog();
});

document.getElementById('closeSettingsDialog').addEventListener('click', hideSettingsDialog);

document.getElementById('settingsDialog').addEventListener('click', (e) => {
    if (e.target === document.getElementById('settingsDialog')) {
        hideSettingsDialog();
    }
});

document.getElementById('contextMenuToggle').addEventListener('change', async (e) => {
    const toggle = e.target;
    const shouldEnable = toggle.checked;

    // Disable toggle during operation
    toggle.disabled = true;

    try {
        if (shouldEnable) {
            const result = await window.electronAPI.contextMenu.enable();
            if (result.success) {
                contextMenuEnabled = true;
                let message = 'Context menu integration enabled successfully!';
                if (result.note) {
                    message += ' ' + result.note;
                }
                showSettingsStatus(message, false);
            } else {
                toggle.checked = false;
                showSettingsStatus('Failed to enable context menu: ' + (result.error || 'Unknown error'), true);
            }
        } else {
            const result = await window.electronAPI.contextMenu.disable();
            if (result.success) {
                contextMenuEnabled = false;
                showSettingsStatus('Context menu integration disabled successfully!', false);
            } else {
                toggle.checked = true;
                showSettingsStatus('Failed to disable context menu: ' + (result.error || 'Unknown error'), true);
            }
        }
    } catch (err) {
        console.error('Error toggling context menu:', err);
        toggle.checked = contextMenuEnabled;
        showSettingsStatus('Error: ' + err.message, true);
    } finally {
        toggle.disabled = false;
    }
});

document.getElementById('menuAbout').addEventListener('click', async () => {
    closeAllMenus();
    let platformStr;
    if (isMacOS) {
        platformStr = 'macOS';
    } else if (navigator.userAgent.includes('Windows')) {
        platformStr = 'Windows';
    } else {
        platformStr = 'Linux';
    }

    try {
        const version = await window.electronAPI.getVersion();
        alert(`FFcut v${version}

    Fast and reliable video editor
    Built with Electron 40.1.0 + FFmpeg

    Platform: ${platformStr}
    License: MIT

    GitHub: https://github.com/Woysful/FFcut`);
    } catch (error) {
        console.error('Error getting app version:', error);
        // Fallback to hardcoded version if API fails
        alert(`FFcut v1.4.0

    Fast and reliable video editor
    Built with Electron 40.1.0 + FFmpeg

    Platform: ${platformStr}
    License: MIT

    GitHub: https://github.com/Woysful/FFcut`);
    }
});

function closeAllMenus() {
    document.querySelectorAll('.menu-item').forEach(item => {
        item.classList.remove('active');
    });
}

// Close menus when clicking outside
document.addEventListener('click', (e) => {
    if (!e.target.closest('.menu-item')) {
        closeAllMenus();
    }
});

// Prevent dropdowns from closing when clicking inside
document.querySelectorAll('.menu-dropdown').forEach(dropdown => {
    dropdown.addEventListener('click', (e) => {
        e.stopPropagation();
    });
});

// Keyboard Shortcuts (исправленные - с проверкой focus)
document.addEventListener('keydown', (e) => {
    // Check if user is typing in an input field or textarea
    // Exclude range inputs (scrubber, volume) to allow hotkeys while scrubbing
    const activeElement = document.activeElement;
    const isTyping = activeElement && (
        (activeElement.tagName === 'INPUT' && activeElement.type !== 'range') ||
        activeElement.tagName === 'TEXTAREA' ||
        activeElement.isContentEditable
    );

    // On macOS shortcuts use Cmd (metaKey); on Windows/Linux they use Ctrl.
    const modPressed = isMacOS ? e.metaKey : e.ctrlKey;

    // Global shortcuts (всегда активны)
    if (modPressed && e.code === 'KeyN') {
        e.preventDefault();
        if (!isImporting) {
            importVideo();
        }
        return;
    }

    if (modPressed && e.code === 'KeyS') {
        e.preventDefault();
        if (state.videoPath) {
            exportBtn.click();
        }
        return;
    }

    if (modPressed && e.code === 'KeyQ') {
        e.preventDefault();
        if (confirm('Are you sure you want to quit?')) {
            window.close();
        }
        return;
    }

    if (e.code === 'F11') {
        e.preventDefault();
        toggleFullscreen();
        return;
    }

    if (modPressed && e.code === 'KeyF') {
        e.preventDefault();
        toggleVideoFullscreen();
        return;
    }

    // Video controls (только когда не набираем текст)
    if (!state.videoElement || isTyping) return;

    if (e.code === 'KeyR') {
        e.preventDefault();
        if (state.videoPath) {
            resetTrimBtn.click();
        }
        return;
    }

    if (e.code === 'Space') {
        e.preventDefault();
        if (state.isPlaying) {
            videoPreview.pause();
        } else {
            videoPreview.play();
        }
    }

    if (e.code === 'ArrowLeft') {
        // e.preventDefault();
        // const step = getFrameStep();
        // const t = Math.max(0, videoPreview.currentTime - step);
        // videoPreview.currentTime = t;
        // if (state.previewAudioPaths) previewAudio.currentTime = t;
        // currentTimeDisplay.textContent = formatTime(t);
        // updatePlayhead();
        e.preventDefault();
        frameBackBtn.click();
    }

    if (e.code === 'ArrowRight') {
        // e.preventDefault();
        // const step = getFrameStep();
        // const t = Math.min(videoPreview.duration, videoPreview.currentTime + step);
        // videoPreview.currentTime = t;
        // if (state.previewAudioPaths) previewAudio.currentTime = t;
        // currentTimeDisplay.textContent = formatTime(t);
        // updatePlayhead();
        e.preventDefault();
        frameForwardBtn.click();
    }

    if (e.code === 'Comma') {
        e.preventDefault();
        frameBackBtn.click();
    }

    if (e.code === 'Period') {
        e.preventDefault();
        frameForwardBtn.click();
    }

    if (e.code === 'KeyQ') {
        e.preventDefault();
        state.trimStart = videoPreview.currentTime;
        trimStartInput.value = state.trimStart.toFixed(1);
        updateTrimHandles();
        updateTrimInfo();
        updateFFmpegCommand();
    }

    if (e.code === 'KeyE') {
        e.preventDefault();
        state.trimEnd = videoPreview.currentTime;
        trimEndInput.value = state.trimEnd.toFixed(1);
        updateTrimHandles();
        updateTrimInfo();
        updateFFmpegCommand();
    }

    if (e.code === 'KeyA') {
        if (e.shiftKey) {
            // Shift+A - Toggle timeline lock
            e.preventDefault();
            state.isTimelineLocked = !state.isTimelineLocked;
            lockTimelineBtn.classList.toggle('active', state.isTimelineLocked);
        } else {
            // A - Toggle loop
            e.preventDefault();
            state.isLooping = !state.isLooping;
            loopBtn.classList.toggle('active', state.isLooping);
        }
    }

    if (e.code === 'KeyC') {
        e.preventDefault();
        // Same logic as quickCropBtn
        // If transform is disabled, enable both transform and crop
        if (!state.transformEnabled) {
            transformToggle.checked = true;
            transformToggle.dispatchEvent(new Event('change'));
            cropToggle.checked = true;
            cropToggle.dispatchEvent(new Event('change'));
        } else {
            // If transform is enabled, just toggle crop
            cropToggle.checked = !cropToggle.checked;
            cropToggle.dispatchEvent(new Event('change'));
        }
    }
});

// Check fullscreen states
function isAppFullscreen() {
    return document.fullscreenElement === document.documentElement;
}

function isVideoFullscreen() {
    const videoContainer = document.querySelector('.preview-container');
    return document.fullscreenElement === videoContainer;
}

// Fullscreen toggle for entire application
function toggleFullscreen() {
    const videoContainer = document.querySelector('.preview-container');
    if (!videoContainer) return;

    if (!document.fullscreenElement) {
        // Not in fullscreen → go to app fullscreen
        document.documentElement.requestFullscreen().catch(err => {
            console.error('Fullscreen error:', err);
        });
    } else if (isVideoFullscreen()) {
        // In video fullscreen → exit all fullscreen
        if (document.exitFullscreen) {
            document.exitFullscreen();
        }
    } else {
        // In app fullscreen → exit fullscreen
        if (document.exitFullscreen) {
            document.exitFullscreen();
        }
    }
}

// Fullscreen toggle for video preview
function toggleVideoFullscreen() {
    const videoContainer = document.querySelector('.preview-container');
    if (!videoContainer) return;

    if (!document.fullscreenElement) {
        // Not in fullscreen → go to video fullscreen
        videoContainer.requestFullscreen().catch(err => {
            console.error('Video fullscreen error:', err);
        });
    } else if (isAppFullscreen()) {
        // In app fullscreen → go to video fullscreen (will be app + video fullscreen)
        videoContainer.requestFullscreen().catch(err => {
            console.error('Video fullscreen error:', err);
        });
    } else if (isVideoFullscreen()) {
        // In video fullscreen → exit video fullscreen (stay in app fullscreen if we were in it)
        if (document.exitFullscreen) {
            document.exitFullscreen();
        }
    }
}

let presets = {};
let currentEditingPreset = null;

// DOM Elements for presets
const presetsMenu = document.getElementById('presetsMenu');
const presetsDropdown = document.getElementById('presetsDropdown');
const presetsListContainer = document.getElementById('presetsListContainer');
const menuManagePresets = document.getElementById('menuManagePresets');

const presetModal = document.getElementById('presetModal');
const closePresetModal = document.getElementById('closePresetModal');
const presetList = document.getElementById('presetList');
const saveNewPresetBtn = document.getElementById('saveNewPresetBtn');
const exportPresetsBtn = document.getElementById('exportPresetsBtn');
const importPresetsBtn = document.getElementById('importPresetsBtn');
const importPresetsFileInput = document.getElementById('importPresetsFileInput');

const savePresetDialog = document.getElementById('savePresetDialog');
const closeSavePresetDialog = document.getElementById('closeSavePresetDialog');
const cancelSavePreset = document.getElementById('cancelSavePreset');
const confirmSavePreset = document.getElementById('confirmSavePreset');
const presetNameInput = document.getElementById('presetNameInput');
const savePresetTitle = document.getElementById('savePresetTitle');

// Load presets on startup
async function loadPresetsFromStorage() {
    try {
        presets = await window.electronAPI.loadPresets();
        updatePresetsDropdown();
    } catch (err) {
        console.error('Failed to load presets:', err);
    }
}

// Update presets dropdown in menu bar
function updatePresetsDropdown() {
    const presetNames = Object.keys(presets);

    if (presetNames.length === 0) {
        presetsListContainer.innerHTML = '<div class="menu-option disabled" style="opacity: 0.5; cursor: default;">No presets saved</div>';
    } else {
        presetsListContainer.innerHTML = presetNames.map(name => `
            <div class="menu-option preset-load-option" data-preset="${escapeHtml(name)}">
                <svg width="16" height="16" viewBox="0 0 20 20" fill="none">
                    <path d="M10 3V6M10 14V17M17 10H14M6 10H3" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                    <circle cx="10" cy="10" r="2" stroke="currentColor" stroke-width="2"/>
                </svg>
                ${escapeHtml(name)}
            </div>
        `).join('');

        // Add event listeners to load presets
        document.querySelectorAll('.preset-load-option').forEach(option => {
            option.addEventListener('click', () => {
                const presetName = option.dataset.preset;
                loadPreset(presetName);
                closeAllMenus();
            });
        });
    }
}

// Load preset into export settings
function loadPreset(name) {
    const preset = presets[name];
    if (!preset) return;

    // Update UI elements
    videoCodec.value = preset.videoCodec || 'copy';
    audioCodec.value = preset.audioCodec || 'copy';
    audioExportTrack.value = preset.audioTrack || 'all';
    qualityMode.value = preset.qualityMode || 'crf';
    crfSlider.value = preset.crf || 23;
    crfValue.value = preset.crf || 23;
    videoBitrate.value = preset.videoBitrate || 5000;
    audioBitrate.value = preset.audioBitrate || 192;
    encoderPreset.value = preset.preset || 'medium';
    pixelFormat.value = preset.pixelFormat || 'auto';
    container.value = preset.container || 'mp4';

    // Update state
    state.exportSettings = { ...preset };

    // Handle audio track picker visibility and restore selected tracks
    if (preset.audioTrack === 'pick-export' || preset.audioTrack === 'pick-merge') {
        audioTrackPickerGroup.style.display = 'block';
        audioTrackPickerLabel.textContent = preset.audioTrack === 'pick-export'
            ? 'Select tracks to export'
            : 'Select tracks to merge';

        // Restore selected tracks
        if (preset.selectedAudioTracks && preset.selectedAudioTracks.length > 0) {
            audioTrackPicker.querySelectorAll('input[type="checkbox"]').forEach(cb => {
                const trackIndex = parseInt(cb.value);
                cb.checked = preset.selectedAudioTracks.includes(trackIndex);
            });
        }
    } else {
        audioTrackPickerGroup.style.display = 'none';
    }

    // Dispatch change events to trigger UI updates
    videoCodec.dispatchEvent(new Event('change'));
    audioCodec.dispatchEvent(new Event('change'));
    qualityMode.dispatchEvent(new Event('change'));
    audioExportTrack.dispatchEvent(new Event('change'));

    updateFFmpegCommand();

    console.log(`Loaded preset: ${name}`);
}

// Get current export settings
function getCurrentExportSettings() {
    return {
        videoCodec: videoCodec.value,
        audioCodec: audioCodec.value,
        audioTrack: audioExportTrack.value,
        selectedAudioTracks: [...state.exportSettings.selectedAudioTracks], // Save selected tracks
        qualityMode: qualityMode.value,
        crf: parseInt(crfSlider.value),
        videoBitrate: parseInt(videoBitrate.value),
        audioBitrate: parseInt(audioBitrate.value),
        preset: encoderPreset.value,
        pixelFormat: pixelFormat.value,
        container: container.value,
        customCommand: ffmpegCommand.value
    };
}

// Show preset manager modal
function showPresetManager() {
    presetModal.style.display = 'flex';
    renderPresetList();
}

// Hide preset manager modal
function hidePresetManager() {
    presetModal.style.display = 'none';
}

// Render preset list in manager
function renderPresetList() {
    const presetNames = Object.keys(presets);

    if (presetNames.length === 0) {
        presetList.innerHTML = '<div class="empty-preset-message">No presets saved yet</div>';
        return;
    }

    presetList.innerHTML = presetNames.map(name => {
        const preset = presets[name];
        const details = [];

        // Build details string
        if (preset.videoCodec) details.push(`Video: ${preset.videoCodec}`);
        if (preset.audioCodec) details.push(`Audio: ${preset.audioCodec}`);
        if (preset.qualityMode === 'crf') details.push(`CRF: ${preset.crf}`);
        if (preset.qualityMode === 'bitrate') details.push(`Bitrate: ${preset.videoBitrate}k`);
        if (preset.container) details.push(`${preset.container.toUpperCase()}`);

        return `
            <div class="preset-item">
                <div class="preset-info">
                    <div class="preset-name">${escapeHtml(name)}</div>
                    <div class="preset-details">
                        ${details.map(d => `<span class="preset-detail">${escapeHtml(d)}</span>`).join('<span style="opacity: 0.3;">•</span>')}
                    </div>
                </div>
                <div class="preset-actions-group">
                    <button class="btn-icon btn-load" data-action="load" data-preset="${escapeHtml(name)}" title="Load preset">
                        <svg width="16" height="16" viewBox="0 0 20 20" fill="none">
                            <path d="M10 4V12M10 12L7 9M10 12L13 9" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                            <path d="M4 13V15C4 15.5304 4.21071 16.0391 4.58579 16.4142C4.96086 16.7893 5.46957 17 6 17H14C14.5304 17 15.0391 16.7893 15.4142 16.4142C15.7893 16.0391 16 15.5304 16 15V13" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                        </svg>
                    </button>
                    <button class="btn-icon" data-action="rename" data-preset="${escapeHtml(name)}" title="Rename preset">
                        <svg width="16" height="16" viewBox="0 0 20 20" fill="none">
                            <path d="M12 4L16 8M3 17L4.5 11.5L14 2L18 6L8.5 15.5L3 17Z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                        </svg>
                    </button>
                    <button class="btn-icon" data-action="overwrite" data-preset="${escapeHtml(name)}" title="Overwrite with current settings">
                        <svg width="16" height="16" viewBox="0 0 20 20" fill="none">
                            <path d="M4 10C4 6.68629 6.68629 4 10 4C13.3137 4 16 6.68629 16 10C16 13.3137 13.3137 16 10 16C6.68629 16 4 13.3137 4 10Z" stroke="currentColor" stroke-width="2"/>
                            <path d="M10 7V10L12 12" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                        </svg>
                    </button>
                    <button class="btn-icon btn-danger" data-action="delete" data-preset="${escapeHtml(name)}" title="Delete preset">
                        <svg width="16" height="16" viewBox="0 0 20 20" fill="none">
                            <path d="M6 6L14 14M6 14L14 6" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                        </svg>
                    </button>
                </div>
            </div>
        `;
    }).join('');

    // Add event listeners
    presetList.querySelectorAll('[data-action]').forEach(btn => {
        btn.addEventListener('click', (e) => {
            const action = e.currentTarget.dataset.action;
            const presetName = e.currentTarget.dataset.preset;
            handlePresetAction(action, presetName);
        });
    });
}

// Handle preset actions
async function handlePresetAction(action, presetName) {
    switch (action) {
        case 'load':
            loadPreset(presetName);
            hidePresetManager();
            break;

        case 'rename':
            currentEditingPreset = presetName;
            presetNameInput.value = presetName;
            savePresetTitle.textContent = 'Rename Preset';
            savePresetDialog.style.display = 'flex';
            setTimeout(() => presetNameInput.select(), 100);
            break;

        case 'overwrite':
            if (confirm(`Overwrite preset "${presetName}" with current settings?`)) {
                await savePresetWithName(presetName);
            }
            break;

        case 'delete':
            if (confirm(`Delete preset "${presetName}"?`)) {
                await deletePreset(presetName);
            }
            break;
    }
}

// Show save preset dialog
function showSavePresetDialog() {
    currentEditingPreset = null;
    presetNameInput.value = '';
    savePresetTitle.textContent = 'Save Preset';
    savePresetDialog.style.display = 'flex';
    setTimeout(() => presetNameInput.focus(), 100);
}

// Hide save preset dialog
function hideSavePresetDialog() {
    savePresetDialog.style.display = 'none';
    presetNameInput.value = '';
    currentEditingPreset = null;
}

// Save preset with name
async function savePresetWithName(name) {
    if (!name || name.trim() === '') return;

    const settings = getCurrentExportSettings();
    const result = await window.electronAPI.savePreset(name.trim(), settings);

    if (result.success) {
        presets[name.trim()] = settings;
        updatePresetsDropdown();
        renderPresetList();
        console.log(`Preset saved: ${name}`);
    } else {
        alert(`Failed to save preset: ${result.error}`);
    }
}

// Delete preset
async function deletePreset(name) {
    const result = await window.electronAPI.deletePreset(name);

    if (result.success) {
        delete presets[name];
        updatePresetsDropdown();
        renderPresetList();
        console.log(`Preset deleted: ${name}`);
    } else {
        alert(`Failed to delete preset: ${result.error}`);
    }
}

// Rename preset
async function renamePreset(oldName, newName) {
    if (!newName || newName.trim() === '') return;

    const result = await window.electronAPI.renamePreset(oldName, newName.trim());

    if (result.success) {
        presets[newName.trim()] = presets[oldName];
        if (oldName !== newName.trim()) {
            delete presets[oldName];
        }
        updatePresetsDropdown();
        renderPresetList();
        console.log(`Preset renamed: ${oldName} → ${newName}`);
    } else {
        alert(`Failed to rename preset: ${result.error}`);
    }
}

// Escape HTML for security
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Export all presets to JSON file
async function exportPresets() {
    if (Object.keys(presets).length === 0) {
        alert('No presets to export');
        return;
    }

    try {
        // Create export data with metadata
        const exportData = {
            version: '1.0',
            exportDate: new Date().toISOString(),
            presets: presets
        };

        const jsonString = JSON.stringify(exportData, null, 2);
        const blob = new Blob([jsonString], { type: 'application/json' });

        // Create download link
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `ffcut-presets-${new Date().toISOString().split('T')[0]}.json`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);

        console.log('Presets exported successfully');
    } catch (err) {
        console.error('Failed to export presets:', err);
        alert(`Failed to export presets: ${err.message}`);
    }
}

// Import presets from JSON file
async function importPresets(file) {
    try {
        const text = await file.text();
        const importData = JSON.parse(text);

        // Validate import data structure
        if (!importData.presets || typeof importData.presets !== 'object') {
            throw new Error('Invalid preset file format');
        }

        const importedPresets = importData.presets;
        const presetNames = Object.keys(importedPresets);

        if (presetNames.length === 0) {
            alert('No presets found in the file');
            return;
        }

        // Check for conflicts
        const conflicts = presetNames.filter(name => presets[name]);
        let shouldProceed = true;

        if (conflicts.length > 0) {
            const message = `The following presets already exist and will be overwritten:\n${conflicts.join(', ')}\n\nContinue?`;
            shouldProceed = confirm(message);
        }

        if (!shouldProceed) return;

        // Import presets one by one
        let successCount = 0;
        let failCount = 0;

        for (const [name, settings] of Object.entries(importedPresets)) {
            try {
                const result = await window.electronAPI.savePreset(name, settings);
                if (result.success) {
                    presets[name] = settings;
                    successCount++;
                } else {
                    failCount++;
                    console.error(`Failed to import preset "${name}":`, result.error);
                }
            } catch (err) {
                failCount++;
                console.error(`Failed to import preset "${name}":`, err);
            }
        }

        // Update UI
        updatePresetsDropdown();
        renderPresetList();

        // Show result
        let message = `Successfully imported ${successCount} preset(s)`;
        if (failCount > 0) {
            message += `\n${failCount} preset(s) failed to import`;
        }
        alert(message);

        console.log(`Presets imported: ${successCount} succeeded, ${failCount} failed`);
    } catch (err) {
        console.error('Failed to import presets:', err);
        alert(`Failed to import presets: ${err.message}`);
    }
}

// Event listeners for preset system
menuManagePresets.addEventListener('click', (e) => {
    e.stopPropagation();
    closeAllMenus();
    showPresetManager();
});

closePresetModal.addEventListener('click', hidePresetManager);
presetModal.addEventListener('click', (e) => {
    if (e.target === presetModal) hidePresetManager();
});

saveNewPresetBtn.addEventListener('click', showSavePresetDialog);

// Export presets button
exportPresetsBtn.addEventListener('click', exportPresets);

// Import presets button
importPresetsBtn.addEventListener('click', () => {
    importPresetsFileInput.click();
});

// Handle file selection for import
importPresetsFileInput.addEventListener('change', async (e) => {
    const file = e.target.files[0];
    if (file) {
        await importPresets(file);
        // Reset input so the same file can be selected again
        e.target.value = '';
    }
});

closeSavePresetDialog.addEventListener('click', hideSavePresetDialog);
cancelSavePreset.addEventListener('click', hideSavePresetDialog);
savePresetDialog.addEventListener('click', (e) => {
    if (e.target === savePresetDialog) hideSavePresetDialog();
});

confirmSavePreset.addEventListener('click', async () => {
    const name = presetNameInput.value.trim();
    if (!name) {
        alert('Please enter a preset name');
        return;
    }

    if (currentEditingPreset) {
        // Renaming
        await renamePreset(currentEditingPreset, name);
    } else {
        // New preset
        if (presets[name]) {
            if (!confirm(`Preset "${name}" already exists. Overwrite?`)) {
                return;
            }
        }
        await savePresetWithName(name);
    }

    hideSavePresetDialog();
});

// Handle Enter key in preset name input
presetNameInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
        confirmSavePreset.click();
    } else if (e.key === 'Escape') {
        hideSavePresetDialog();
    }
});

// Load presets on startup
loadPresetsFromStorage();

// Initialize custom spinners for number inputs
initializeCustomSpinners();

// Listen for file-open requests from the main process (CLI arg, second
// instance, or macOS open-file event) and feed the path directly into
// importVideo, which will skip the file-dialog when a path is provided.
window.electronAPI.onOpenFile((filePath) => {
    console.log('Received open-file event:', filePath);
    importVideo(filePath);
});

// Update System

let updateDialog = null;
let updateData = null;

// Create update dialog
function createUpdateDialog() {
    const dialog = document.createElement('div');
    dialog.className = 'update-dialog';
    dialog.style.display = 'none';
    dialog.innerHTML = `
        <div class="update-dialog-content">
            <div class="update-dialog-header">
                <div class="update-dialog-icon">
                    <svg width="24" height="24" viewBox="0 0 20 20" fill="none">
                        <path d="M10 3V6M10 14V17M17 10H14M6 10H3" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                        <circle cx="10" cy="10" r="2" stroke="currentColor" stroke-width="2"/>
                        <path d="M15 5L13 7M7 13L5 15M5 5L7 7M13 13L15 15" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                    </svg>
                </div>
                <h3 class="update-dialog-title">Update Available</h3>
            </div>
            <div class="update-dialog-body">
                <div class="update-dialog-version">
                    A new version of FFcut is available: <strong id="updateVersion"></strong>
                </div>
                <div class="update-dialog-notes" id="updateNotes"></div>
                <div class="update-progress" id="updateProgress" style="display: none;">
                    <div class="update-progress-bar">
                        <div class="update-progress-fill" id="updateProgressFill" style="width: 0%;"></div>
                    </div>
                    <div class="update-progress-text" id="updateProgressText">Preparing...</div>
                </div>
            </div>
            <div class="update-dialog-actions">
                <button class="update-dialog-button secondary" id="updateLaterBtn">Later</button>
                <button class="update-dialog-button primary" id="updateInstallBtn">Download & Install</button>
            </div>
        </div>
    `;
    document.body.appendChild(dialog);
    return dialog;
}

// Show update dialog
function showUpdateDialog(data) {
    if (!updateDialog) {
        updateDialog = createUpdateDialog();

        // Setup event listeners
        const laterBtn = updateDialog.querySelector('#updateLaterBtn');
        const installBtn = updateDialog.querySelector('#updateInstallBtn');

        laterBtn.addEventListener('click', hideUpdateDialog);
        installBtn.addEventListener('click', handleInstallUpdate);
    }

    updateData = data;

    // Update dialog content
    const versionEl = updateDialog.querySelector('#updateVersion');
    const notesEl = updateDialog.querySelector('#updateNotes');
    const progressEl = updateDialog.querySelector('#updateProgress');
    const installBtn = updateDialog.querySelector('#updateInstallBtn');
    const laterBtn = updateDialog.querySelector('#updateLaterBtn');

    versionEl.textContent = data.version;
    notesEl.textContent = data.releaseNotes || 'No release notes available.';
    progressEl.style.display = 'none';
    installBtn.disabled = false;
    laterBtn.disabled = false;

    updateDialog.style.display = 'flex';
}

// Hide update dialog
function hideUpdateDialog() {
    if (updateDialog) {
        updateDialog.style.display = 'none';
    }
}

// Handle install update
async function handleInstallUpdate() {
    const installBtn = updateDialog.querySelector('#updateInstallBtn');
    const laterBtn = updateDialog.querySelector('#updateLaterBtn');
    const progressEl = updateDialog.querySelector('#updateProgress');

    installBtn.disabled = true;
    laterBtn.disabled = true;
    progressEl.style.display = 'block';

    try {
        await window.electronAPI.updater.downloadAndInstall();
    } catch (error) {
        console.error('Update installation failed:', error);
        installBtn.disabled = false;
        laterBtn.disabled = false;
        progressEl.style.display = 'none';
    }
}

// Update progress handlers
window.electronAPI.onUpdateDownloadProgress((data) => {
    if (!updateDialog) return;

    const progressFill = updateDialog.querySelector('#updateProgressFill');
    const progressText = updateDialog.querySelector('#updateProgressText');

    progressFill.style.width = `${data.progress}%`;
    progressText.textContent = `Downloading... ${data.downloaded} / ${data.total} (${data.progress}%)`;
});

window.electronAPI.onUpdateDownloadStatus((data) => {
    if (!updateDialog) return;

    const progressText = updateDialog.querySelector('#updateProgressText');

    if (data.status === 'downloading') {
        progressText.textContent = data.message;
    } else if (data.status === 'completed') {
        progressText.textContent = 'Download completed! Installing...';
    } else if (data.status === 'error') {
        progressText.textContent = `Error: ${data.message}`;
        const installBtn = updateDialog.querySelector('#updateInstallBtn');
        const laterBtn = updateDialog.querySelector('#updateLaterBtn');
        installBtn.disabled = false;
        laterBtn.disabled = false;
    }
});

window.electronAPI.onUpdateInstallStatus((data) => {
    if (!updateDialog) return;

    const progressText = updateDialog.querySelector('#updateProgressText');
    const installBtn = updateDialog.querySelector('#updateInstallBtn');
    const laterBtn = updateDialog.querySelector('#updateLaterBtn');

    if (data.status === 'installing') {
        progressText.textContent = data.message;
    } else if (data.status === 'completed') {
        progressText.textContent = data.message;
        installBtn.textContent = 'Restart Now';
        installBtn.disabled = false;
        laterBtn.textContent = 'Restart Later';
        laterBtn.disabled = false;

        // Change install button to restart app
        installBtn.onclick = () => {
            window.location.reload();
        };
    } else if (data.status === 'manual') {
        progressText.textContent = data.message;
        installBtn.textContent = 'OK';
        installBtn.disabled = false;
        laterBtn.style.display = 'none';
        installBtn.onclick = hideUpdateDialog;
    } else if (data.status === 'error') {
        progressText.textContent = `Error: ${data.message}`;
        installBtn.disabled = false;
        laterBtn.disabled = false;
    }
});

window.electronAPI.onUpdateCheckStatus((data) => {
    if (data.status === 'checking') {
        console.log('Checking for updates...');
    } else if (data.status === 'up-to-date') {
        showNotification('You are using the latest version!', 'success');
    } else if (data.status === 'error') {
        console.error('Update check error:', data.message);
        showNotification(`Failed to check for updates: ${data.message}`, 'error');
    }
});

window.electronAPI.onUpdateAvailable((data) => {
    const updateIndicator = document.getElementById('updateIndicator');

    if (data.silent) {
        // Silent check on startup - just show indicator
        if (updateIndicator) {
            updateIndicator.style.display = 'inline-block';
        }
    } else {
        // Manual check - show dialog
        showUpdateDialog(data);
    }
});

// Menu handlers
const menuCheckUpdates = document.getElementById('menuCheckUpdates');
if (menuCheckUpdates) {
    menuCheckUpdates.addEventListener('click', async () => {
        try {
            await window.electronAPI.updater.checkForUpdates();
        } catch (error) {
            console.error('Failed to check for updates:', error);
            showNotification('Failed to check for updates', 'error');
        }
    });
}

// Initialize custom number input spinners
function initializeCustomSpinners() {
    document.querySelectorAll('.control-group input[type="number"]:not(.number-input-container *)').forEach(input => {
        // Create wrapper and spinner structure
        const container = Object.assign(document.createElement('div'), { className: 'number-input-container' });
        const spinner = Object.assign(document.createElement('div'), { className: 'number-input-spinner' });

        // Create buttons with event handlers
        const updateValue = (delta) => {
            const step = parseFloat(input.step) || 1;
            const currentValue = parseFloat(input.value) || 0;
            const limit = delta > 0 ? (input.min ? parseFloat(input.min) : -Infinity) : (input.max ? parseFloat(input.max) : Infinity);
            const newValue = delta > 0 ? Math.max(limit, currentValue + step) : Math.min(limit, currentValue - step);
            input.value = newValue;
            input.dispatchEvent(new Event('input', { bubbles: true }));
            input.dispatchEvent(new Event('change', { bubbles: true }));
        };

        [['▲', 'Increase value', 1], ['▼', 'Decrease value', -1]].forEach(([symbol, title, delta]) => {
            const button = Object.assign(document.createElement('button'), {
                type: 'button',
                innerHTML: symbol,
                title,
                tabIndex: -1  // Prevent tab focus
            });
            button.addEventListener('click', () => updateValue(delta));
            spinner.appendChild(button);
        });

        // Replace input with wrapped version
        input.parentNode.insertBefore(container, input);
        container.appendChild(input);
        container.appendChild(spinner);
    });
}

// Helper function to show notifications
function showNotification(message, type = 'info') {
    // Create a simple toast notification
    const toast = document.createElement('div');
    toast.className = `notification-toast notification-${type}`;
    toast.textContent = message;
    toast.style.cssText = `
        position: fixed;
        top: 20px;
        right: 20px;
        padding: 12px 20px;
        background-color: ${type === 'success' ? '#22c55e' : type === 'error' ? '#ef4444' : '#3b82f6'};
        color: white;
        border-radius: 8px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
        z-index: 10001;
        font-size: 14px;
        font-weight: 500;
        opacity: 0;
        transform: translateY(-20px);
        transition: all 0.3s ease;
    `;

    document.body.appendChild(toast);

    // Animate in
    setTimeout(() => {
        toast.style.opacity = '1';
        toast.style.transform = 'translateY(0)';
    }, 10);

    // Remove after 3 seconds
    setTimeout(() => {
        toast.style.opacity = '0';
        toast.style.transform = 'translateY(-20px)';
        setTimeout(() => {
            document.body.removeChild(toast);
        }, 300);
    }, 3000);
}
