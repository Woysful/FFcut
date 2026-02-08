const ffmpeg = require('fluent-ffmpeg');
const fs = require('fs').promises;
const fsSync = require('fs');
const path = require('path');
const os = require('os');
const crypto = require('crypto');

/**
 * FFmpeg service for handling video transcoding operations
 * Centralizes all FFmpeg-related logic to reduce code duplication
 */
class FFmpegService {
    constructor() {
        this.currentProcess = null;
        this.isCancelled = false;
        this.currentFilePath = null;
        this.supportedHTML5Codecs = ['h264', 'avc', 'avc1', 'vp8', 'vp9', 'av1', 'theora'];
        this.tempDir = path.join(os.tmpdir(), 'ffcut-previews');
    }

    /**
     * Check if codec is natively supported by HTML5 video
     */
    isCodecSupported(codecName) {
        if (!codecName) return false;
        const codec = codecName.toLowerCase();
        return this.supportedHTML5Codecs.some(supported => codec.includes(supported));
    }

    /**
     * Detect available hardware encoder
     * Priority: NVENC > QSV > VAAPI > AMF > CPU
     */
    async detectHardwareEncoder() {
        return new Promise((resolve) => {
            ffmpeg.getAvailableEncoders((err, encoders) => {
                if (err) {
                    console.log('Could not detect encoders, using CPU fallback');
                    resolve('libx264');
                    return;
                }

                const hwEncoders = ['h264_nvenc', 'h264_qsv', 'h264_vaapi', 'h264_amf'];
                const available = hwEncoders.find(enc => encoders[enc]);
                resolve(available || 'libx264');
            });
        });
    }

    /**
     * Configure encoder settings based on encoder type
     */
    configureEncoder(command, encoder, includeAudio = false) {
        const configs = {
            'h264_nvenc': { codec: 'h264_nvenc', options: ['-preset', 'fast', '-cq', '28'] },
            'h264_qsv': { codec: 'h264_qsv', options: ['-preset', 'fast', '-global_quality', '28'] },
            'h264_vaapi': { codec: 'h264_vaapi', options: ['-qp', '28'] },
            'h264_amf': { codec: 'h264_amf', options: ['-quality', 'speed', '-qp_i', '28'] },
            'libx264': { codec: 'libx264', options: ['-preset', 'veryfast', '-crf', '28'] }
        };

        const config = configs[encoder] || configs.libx264;
        command.videoCodec(config.codec);
        
        for (let i = 0; i < config.options.length; i += 2) {
            command.addOption(config.options[i], config.options[i + 1]);
        }

        if (includeAudio) {
            command.audioCodec('aac').audioBitrate('128k');
        }

        return command;
    }

    /**
     * Set up progress handlers for FFmpeg command
     */
    setupProgressHandlers(command, event, outputPath, encoder, onComplete, onError) {
        command
            .on('start', (cmd) => {
                console.log('FFmpeg command:', cmd);
                this.currentProcess = command.ffmpegProc;
                event.sender.send('transcode-progress', { status: 'started', encoder });
            })
            .on('progress', (progress) => {
                if (this.handleCancellation(outputPath, event, onError)) return;

                if (progress.percent) {
                    event.sender.send('transcode-progress', {
                        status: 'progress',
                        percent: Math.round(progress.percent),
                        encoder
                    });
                }
            })
            .on('end', () => {
                this.cleanup();
                event.sender.send('transcode-progress', { status: 'completed', encoder });
                onComplete(outputPath);
            })
            .on('error', (err) => {
                this.cleanup();
                console.error('Transcode error:', err);

                // Clean up partial file on error (unless cancelled)
                if (!this.isCancelled && fsSync.existsSync(outputPath)) {
                    try {
                        fsSync.unlinkSync(outputPath);
                    } catch (cleanupErr) {
                        console.warn('Failed to clean up failed transcode file:', cleanupErr);
                    }
                }

                const status = this.isCancelled ? 'cancelled' : 'error';
                event.sender.send('transcode-progress', { 
                    status, 
                    error: this.isCancelled ? undefined : err.message 
                });
                onError(err);
            });
    }

    /**
     * Handle cancellation during progress
     */
    handleCancellation(outputPath, event, onError) {
        if (!this.isCancelled) return false;

        if (this.currentProcess) {
            this.currentProcess.kill('SIGKILL');
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
        onError(new Error('Transcoding cancelled by user'));
        return true;
    }

    /**
     * Create video-only preview for unsupported codecs
     */
    async createVideoOnlyPreview(inputPath, event, outputPath) {
        const encoder = await this.detectHardwareEncoder();
        
        return new Promise((resolve, reject) => {
            this.reset(inputPath);

            const command = ffmpeg(inputPath).outputOptions(['-map', '0:v:0', '-an']);
            this.configureEncoder(command, encoder, false);
            this.setupProgressHandlers(command, event, outputPath, encoder, resolve, reject);
            command.save(outputPath);
        });
    }

    /**
     * Create preview with video and first audio track
     */
    async createVideoPreview(inputPath, event) {
        await fs.mkdir(this.tempDir, { recursive: true }).catch(() => {});

        const fileName = path.basename(inputPath, path.extname(inputPath));
        const outputPath = path.join(this.tempDir, `${fileName}_preview.mp4`);

        // Check if preview exists and is valid
        if (await this.isValidPreview(outputPath)) {
            return outputPath;
        }

        const encoder = await this.detectHardwareEncoder();
        
        return new Promise((resolve, reject) => {
            this.reset(inputPath);

            const command = ffmpeg(inputPath).outputOptions(['-map', '0:v:0', '-map', '0:a:0']);
            this.configureEncoder(command, encoder, true);
            this.setupProgressHandlers(command, event, outputPath, encoder, resolve, reject);
            command.save(outputPath);
        });
    }

    /**
     * Check if preview file is valid
     */
    async isValidPreview(outputPath, minSize = 100 * 1024) {
        try {
            const stats = await fs.stat(outputPath);
            return stats.size > minSize;
        } catch {
            return false;
        }
    }

    /**
     * Prepare multi-audio preview (video + separate audio tracks)
     */
    async prepareMultiAudioPreview(filePath, event, metadata) {
        this.reset(filePath);

        const audioStreams = metadata.streams.filter(s => s.codec_type === 'audio');
        if (audioStreams.length <= 1) return null;

        const videoStream = metadata.streams.find(s => s.codec_type === 'video');
        const needsPreview = videoStream && !this.isCodecSupported(videoStream.codec_name);
        
        const dirHash = crypto.createHash('md5').update(filePath).digest('hex').slice(0, 12);
        const tempDir = path.join(this.tempDir, dirHash);
        const videoPath = path.join(tempDir, 'video.mp4');
        const audioPaths = audioStreams.map((_, i) => path.join(tempDir, `audio_${i}.m4a`));

        // Check if all files exist and are valid
        if (await this.areMultiAudioFilesValid(videoPath, audioPaths)) {
            return { videoPath, audioPaths };
        }

        // Create directory if needed
        if (!fsSync.existsSync(tempDir)) {
            fsSync.mkdirSync(tempDir, { recursive: true });
        }

        // Create video file if needed
        if (!fsSync.existsSync(videoPath)) {
            if (needsPreview) {
                await this.createVideoOnlyPreview(filePath, event, videoPath);
            } else {
                await this.copyVideoStream(filePath, videoPath);
            }
        }

        // Check for cancellation
        if (this.isCancelled) {
            throw new Error('Transcoding cancelled by user');
        }

        // Extract audio tracks
        await this.extractAudioTracks(filePath, audioPaths);

        this.currentFilePath = null;
        return { videoPath, audioPaths };
    }

    /**
     * Check if multi-audio files are valid
     */
    async areMultiAudioFilesValid(videoPath, audioPaths) {
        if (!fsSync.existsSync(videoPath)) return false;
        if (!audioPaths.every(p => fsSync.existsSync(p))) return false;

        try {
            const videoStats = fsSync.statSync(videoPath);
            const audioStats = audioPaths.map(p => fsSync.statSync(p));
            
            return videoStats.size > 100 * 1024 && 
                   audioStats.every(stats => stats.size > 10 * 1024);
        } catch {
            return false;
        }
    }

    /**
     * Copy video stream without re-encoding
     */
    async copyVideoStream(inputPath, outputPath) {
        return new Promise((resolve, reject) => {
            ffmpeg(inputPath)
                .outputOptions(['-map', '0:v:0', '-c', 'copy', '-an'])
                .on('error', reject)
                .on('end', resolve)
                .save(outputPath);
        });
    }

    /**
     * Extract multiple audio tracks
     */
    async extractAudioTracks(filePath, audioPaths) {
        const promises = audioPaths.map((audioPath, i) => {
            if (fsSync.existsSync(audioPath)) return Promise.resolve();
            if (this.isCancelled) return Promise.reject(new Error('Transcoding cancelled by user'));

            return new Promise((resolve, reject) => {
                ffmpeg(filePath)
                    .outputOptions(['-map', `0:a:${i}`])
                    .audioCodec('aac')
                    .audioBitrate('128k')
                    .on('error', (err) => {
                        if (fsSync.existsSync(audioPath)) {
                            try { fsSync.unlinkSync(audioPath); } 
                            catch (e) { console.warn('Failed to clean up partial audio:', e); }
                        }
                        reject(err);
                    })
                    .on('end', resolve)
                    .save(audioPath);
            });
        });

        await Promise.all(promises);
    }

    /**
     * Get video metadata using ffprobe
     */
    async getVideoMetadata(filePath) {
        return new Promise((resolve, reject) => {
            ffmpeg.ffprobe(filePath, (err, data) => {
                if (err) reject(err);
                else {
                    // Extract pixel format from video stream
                    const videoStream = data.streams.find(s => s.codec_type === 'video');
                    if (videoStream && videoStream.pix_fmt) {
                        data.pix_fmt = videoStream.pix_fmt;
                    }
                    resolve(data);
                }
            });
        });
    }

    /**
     * Clean up temp files for a specific video
     */
    async cleanupTempFiles(videoPath) {
        if (!videoPath) return;

        try {
            // Clean up single preview
            const fileName = path.basename(videoPath, path.extname(videoPath));
            const previewPath = path.join(this.tempDir, `${fileName}_preview.mp4`);
            await this.deleteFileIfExists(previewPath);

            // Clean up multi-audio directory
            const dirHash = crypto.createHash('md5').update(videoPath).digest('hex').slice(0, 12);
            const multiAudioDir = path.join(this.tempDir, dirHash);
            await this.deleteDirIfExists(multiAudioDir);
        } catch (err) {
            console.warn('Failed to clean up temp files:', videoPath, err);
        }
    }

    /**
     * Clean up all temp files
     */
    async cleanupAllTempFiles() {
        try {
            if (fsSync.existsSync(this.tempDir)) {
                await fs.rm(this.tempDir, { recursive: true, force: true });
                console.log('Cleaned up all temp preview files');
            }
        } catch (err) {
            console.error('Error cleaning temp files:', err);
        }
    }

    /**
     * Helper: delete file if exists
     */
    async deleteFileIfExists(filePath) {
        try {
            await fs.access(filePath);
            await fs.unlink(filePath);
            console.log('Cleaned up:', filePath);
        } catch {}
    }

    /**
     * Helper: delete directory if exists
     */
    async deleteDirIfExists(dirPath) {
        try {
            await fs.access(dirPath);
            await fs.rm(dirPath, { recursive: true, force: true });
            console.log('Cleaned up directory:', dirPath);
        } catch {}
    }

    /**
     * Cancel current operation
     */
    cancel() {
        this.isCancelled = true;
        if (this.currentProcess) {
            this.currentProcess.kill('SIGKILL');
            this.currentProcess = null;
        }
        return this.currentFilePath;
    }

    /**
     * Reset state for new operation
     */
    reset(filePath = null) {
        this.currentFilePath = filePath;
        this.isCancelled = false;
    }

    /**
     * Clean up process references
     */
    cleanup() {
        this.currentProcess = null;
        this.currentFilePath = null;
    }
}

module.exports = FFmpegService;
