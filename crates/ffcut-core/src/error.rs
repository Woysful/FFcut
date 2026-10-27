//! The error type of the crate.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// Reading or writing a file failed.
    Io(std::io::Error),
    /// An external tool (ffmpeg, ffprobe) could not be started.
    Spawn { tool: String, source: std::io::Error },
    /// An external tool ran and failed; `stderr` is what it said, trimmed.
    Tool { tool: String, stderr: String },
    /// Data that could not be understood (a tool's output, a JSON file...). The text says what and why.
    Parse(String),
    /// The file has neither a video nor an audio stream.
    NoStreams,
    /// The input is readable but cannot be used ("the track is silent", "no presets found"...).
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Spawn { tool, source } => write!(f, "cannot start {tool}: {source}"),
            Error::Tool { tool, stderr } if stderr.is_empty() => write!(f, "{tool} failed"),
            Error::Tool { tool, stderr } => write!(f, "{tool} failed: {stderr}"),
            Error::Parse(m) | Error::Invalid(m) => f.write_str(m),
            Error::NoStreams => f.write_str("no video or audio stream in this file"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) | Error::Spawn { source: e, .. } => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
