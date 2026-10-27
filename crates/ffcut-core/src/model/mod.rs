//! Plain data: what the UI edits and the builders read. No I/O, no ffmpeg text.
//!
//! * [`source`]: what ffprobe found in the opened file;
//! * [`export`]: the encoding choices and the clip edits of the picture;
//! * [`audio`]: the audio tracks of the timeline and their effects;
//! * [`video`]: video effects, covered areas, text and caption bars (also re-exported by `videofx`).

pub mod audio;
pub mod export;
pub mod source;
pub mod video;

pub use audio::*;
pub use export::*;
pub use source::*;
