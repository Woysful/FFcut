//! An ffmpeg filter the user picked by name, used as one effect of an audio or a video stack.
//! `filters` lists what the user's ffmpeg offers and turns this into filter text.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FilterFx {
    /// ffmpeg's name of the filter (`colorbalance`, `aecho`...).
    pub name: String,
    /// The options the user changed, as ffmpeg text, in the order they were first set. Every other option keeps
    /// ffmpeg's default (which may differ between ffmpeg versions, so it is not written down).
    #[serde(default)]
    pub opts: Vec<(String, String)>,
}

impl FilterFx {
    pub fn new(name: impl Into<String>) -> Self {
        FilterFx { name: name.into(), opts: Vec::new() }
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.opts.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    /// `None` = back to ffmpeg's default.
    pub fn set(&mut self, key: &str, value: Option<String>) {
        match (self.opts.iter().position(|(k, _)| k == key), value) {
            (Some(i), Some(v)) => self.opts[i].1 = v,
            (Some(i), None) => {
                self.opts.remove(i);
            }
            (None, Some(v)) => self.opts.push((key.to_string(), v)),
            (None, None) => {}
        }
    }
}
