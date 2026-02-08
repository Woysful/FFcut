const ffmpeg = require('fluent-ffmpeg');
const fsSync = require('fs');

/**
 * Video export service
 * Handles video export with custom FFmpeg commands
 */
class ExportService {
    constructor() {
        this.currentProcess = null;
        this.currentOutputPath = null;
        this.currentResolve = null;
        this.isCancelled = false;
    }

    /**
     * Export video with custom command
     */
    async exportVideo(inputPath, outputPath, customCommand, event) {
        return new Promise((resolve, reject) => {
            this.reset(outputPath, resolve);

            if (this.isCancelled) {
                this.cleanup();
                resolve({ cancelled: true });
                return;
            }

            let command = ffmpeg(inputPath);
            
            // Parse and apply custom command
            if (customCommand && customCommand.trim()) {
                command = this.applyCustomCommand(command, customCommand);
            }

            this.setupHandlers(command, outputPath, event, resolve, reject);
            command.save(outputPath);
        });
    }

    /**
     * Parse and apply custom FFmpeg command arguments
     */
    applyCustomCommand(command, customCommand) {
        const args = customCommand.trim().split(/\s+/);
        
        for (let i = 0; i < args.length; i++) {
            const arg = args[i];
            const nextArg = args[i + 1];

            switch (arg) {
                case '-ss':
                    if (nextArg) command = command.setStartTime(parseFloat(args[++i]));
                    break;
                case '-t':
                    if (nextArg) command = command.setDuration(parseFloat(args[++i]));
                    break;
                case '-c:v':
                    if (nextArg) command = command.videoCodec(args[++i]);
                    break;
                case '-c:a':
                    if (nextArg) command = command.audioCodec(args[++i]);
                    break;
                case '-b:v':
                    if (nextArg) command = command.videoBitrate(args[++i]);
                    break;
                case '-b:a':
                    if (nextArg) command = command.audioBitrate(args[++i]);
                    break;
                case '-vf':
                    if (nextArg) command = command.videoFilters(args[++i].replace(/"/g, ''));
                    break;
                case '-filter_complex':
                    if (nextArg) command = command.complexFilter(args[++i].replace(/"/g, ''));
                    break;
                case '-preset':
                case '-crf':
                case '-cq':
                case '-qp':
                case '-global_quality':
                case '-pix_fmt':
                case '-map':
                    if (nextArg) command = command.addOption(arg, args[++i]);
                    break;
                case '-c':
                    if (nextArg === 'copy') {
                        command = command.videoCodec('copy').audioCodec('copy');
                        i++;
                    }
                    break;
            }
        }

        return command;
    }

    /**
     * Set up event handlers for export process
     */
    setupHandlers(command, outputPath, event, resolve, reject) {
        command
            .on('start', (commandLine) => {
                console.log('FFmpeg export command:', commandLine);
                this.currentProcess = command.ffmpegProc;
            })
            .on('progress', (progress) => {
                if (this.handleCancellation(outputPath)) return;
                event.sender.send('export-progress', progress);
            })
            .on('end', () => {
                this.cleanup();
                resolve({ success: true });
            })
            .on('error', (err) => {
                this.cleanup();
                console.error('Export error:', err);

                // Clean up partial file on error (unless cancelled)
                if (!this.isCancelled && fsSync.existsSync(outputPath)) {
                    try {
                        fsSync.unlinkSync(outputPath);
                    } catch (cleanupErr) {
                        console.warn('Failed to clean up failed export file:', cleanupErr);
                    }
                }

                if (this.isCancelled) {
                    event.sender.send('export-progress', { status: 'cancelled' });
                } else {
                    reject(err);
                }
            });
    }

    /**
     * Handle cancellation during export
     */
    handleCancellation(outputPath) {
        if (!this.isCancelled) return false;

        if (this.currentProcess) {
            this.currentProcess.kill('SIGKILL');
        }

        // Clean up partial file
        if (fsSync.existsSync(outputPath)) {
            try {
                fsSync.unlinkSync(outputPath);
            } catch (cleanupErr) {
                console.warn('Failed to clean up partial export file:', cleanupErr);
            }
        }

        return true;
    }

    /**
     * Cancel current export
     */
    cancel(event) {
        this.isCancelled = true;

        // Resolve with cancelled result
        if (this.currentResolve) {
            this.currentResolve({ cancelled: true });
            this.currentResolve = null;
        }

        if (this.currentProcess) {
            this.currentProcess.kill('SIGKILL');
            this.currentProcess = null;
        }

        // Clean up partial file
        if (this.currentOutputPath && fsSync.existsSync(this.currentOutputPath)) {
            try {
                fsSync.unlinkSync(this.currentOutputPath);
            } catch (err) {
                console.warn('Failed to clean up partial export file:', err);
            }
        }

        this.currentOutputPath = null;
        event.sender.send('export-progress', { status: 'cancelled' });
    }

    /**
     * Reset state for new export
     */
    reset(outputPath, resolve) {
        this.isCancelled = false;
        this.currentOutputPath = outputPath;
        this.currentResolve = resolve;
    }

    /**
     * Clean up process references
     */
    cleanup() {
        this.currentProcess = null;
        this.currentOutputPath = null;
        this.currentResolve = null;
    }
}

module.exports = ExportService;
