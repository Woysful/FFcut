//! Plain data: what the UI edits and the builders read. No I/O, no ffmpeg text.
//!
//! * [`source`]: what ffprobe found in the opened file;
//! * [`export`]: the encoding choices and the clip edits of the picture;
//! * [`audio`]: the audio tracks of the timeline and their effects;
//! * [`video`]: video effects, covered areas, text and caption bars (also re-exported by `videofx`);
//! * [`filter`]: any ffmpeg filter picked by name, as an audio or a video effect;
//! * [`plugin`]: a LADSPA, LV2 or frei0r plugin, as an audio or a video effect.

pub mod audio;
pub mod export;
pub mod filter;
pub mod plugin;
pub mod source;
pub mod video;

pub use audio::*;
pub use export::*;
pub use filter::FilterFx;
pub use plugin::{PluginFx, PluginKind};
pub use source::*;
