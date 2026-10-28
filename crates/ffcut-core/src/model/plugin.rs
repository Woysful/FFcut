//! A LADSPA, LV2 or frei0r plugin used as one effect (ffmpeg hosts them with its `ladspa`, `lv2` and
//! `frei0r` filters). `plugins` finds the installed ones and turns this into filter text.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginKind {
    Ladspa,
    Lv2,
    Frei0r,
}

impl PluginKind {
    pub fn label(self) -> &'static str {
        match self {
            PluginKind::Ladspa => "LADSPA",
            PluginKind::Lv2 => "LV2",
            PluginKind::Frei0r => "frei0r",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginFx {
    pub kind: PluginKind,
    /// LADSPA: the library file; LV2: the plugin URI; frei0r: the file name without its extension.
    pub id: String,
    /// LADSPA: the label of the plugin inside the library (one file can hold several).
    #[serde(default)]
    pub label: String,
    /// The plugin's own name, kept so that a plugin that is gone is still recognisable.
    #[serde(default)]
    pub name: String,
    /// The controls the user changed (and those that have no default), as text: LADSPA `c0`, `c1`...
    /// (ffmpeg's numbering of the input controls), LV2 port symbols, frei0r parameter numbers `0`, `1`...
    #[serde(default)]
    pub params: Vec<(String, String)>,
}

impl PluginFx {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    /// `None` = back to the plugin's default.
    pub fn set(&mut self, key: &str, value: Option<String>) {
        match (self.params.iter().position(|(k, _)| k == key), value) {
            (Some(i), Some(v)) => self.params[i].1 = v,
            (Some(i), None) => {
                self.params.remove(i);
            }
            (None, Some(v)) => self.params.push((key.to_string(), v)),
            (None, None) => {}
        }
    }
    pub fn title(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else if !self.label.is_empty() {
            &self.label
        } else {
            &self.id
        }
    }
}
