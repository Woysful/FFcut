//! What the user's ffmpeg offers as effects: its filters and their options, and the installed plugins, read
//! in the background the first time they are needed and kept for the session (a new ffmpeg or new plugin
//! folders in the settings start a new catalog).

use eframe::egui;
use ffcut_core::filters::{self, FilterHelp, FilterInfo, Media};
use ffcut_core::plugins::{self, Dirs, PluginFx, PluginInfo, PluginKind, Scan};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub enum Load<T> {
    Loading,
    Ready(Arc<T>),
    Failed(String),
}

// not derived: that would ask for `T: Clone`, and only the `Arc` is cloned
impl<T> Clone for Load<T> {
    fn clone(&self) -> Self {
        match self {
            Load::Loading => Load::Loading,
            Load::Ready(v) => Load::Ready(v.clone()),
            Load::Failed(e) => Load::Failed(e.clone()),
        }
    }
}

/// `ffmpeg -filters`: the offered filters, and the plugin hosts ffmpeg was built with.
pub struct Filters {
    pub list: Vec<FilterInfo>,
    pub hosts: Vec<String>,
}

#[derive(Default)]
struct Inner {
    filters: Option<Load<Filters>>,
    help: HashMap<String, Load<FilterHelp>>,
    plugins: Option<Load<Scan>>,
}

/// Cheap to clone: every clone shares the same cache.
#[derive(Clone, Default)]
pub struct Catalog {
    ffmpeg: Option<PathBuf>,
    dirs: Dirs,
    ctx: Option<egui::Context>,
    inner: Arc<Mutex<Inner>>,
}

impl Catalog {
    pub fn new(ffmpeg: Option<PathBuf>, dirs: Dirs, ctx: &egui::Context) -> Self {
        Catalog { ffmpeg, dirs, ctx: Some(ctx.clone()), inner: Default::default() }
    }

    /// Runs `work` with the ffmpeg path on a thread and stores the result with `put` (`Failed` without ffmpeg).
    fn spawn<T: Send + Sync + 'static>(
        &self,
        work: impl FnOnce(&Path) -> Result<T, String> + Send + 'static,
        put: impl FnOnce(&mut Inner, Load<T>) + Send + 'static,
    ) {
        let Some(ffmpeg) = self.ffmpeg.clone() else {
            put(&mut self.inner.lock().unwrap(), Load::Failed("ffmpeg was not found".into()));
            return;
        };
        let (inner, ctx) = (self.inner.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = match work(&ffmpeg) {
                Ok(v) => Load::Ready(Arc::new(v)),
                Err(e) => Load::Failed(e),
            };
            put(&mut inner.lock().unwrap(), r);
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    /// Every offered filter of ffmpeg (audio and video).
    pub fn filters(&self) -> Load<Filters> {
        if let Some(l) = &self.inner.lock().unwrap().filters {
            return l.clone();
        }
        self.inner.lock().unwrap().filters = Some(Load::Loading);
        self.spawn(
            |ff| filters::list_filters_and_hosts(ff).map(|(list, hosts)| Filters { list, hosts }).map_err(|e| e.to_string()),
            |i, l| i.filters = Some(l),
        );
        self.inner.lock().unwrap().filters.clone().unwrap_or(Load::Loading)
    }

    /// The filters of one kind, once the list is in.
    pub fn filters_of(&self, media: Media) -> Load<Vec<FilterInfo>> {
        match self.filters() {
            Load::Ready(l) => Load::Ready(Arc::new(l.list.iter().filter(|f| f.media == media).cloned().collect())),
            Load::Loading => Load::Loading,
            Load::Failed(e) => Load::Failed(e),
        }
    }

    /// One line about a filter (from the list), if it is in.
    pub fn desc(&self, name: &str) -> Option<String> {
        match &self.inner.lock().unwrap().filters {
            Some(Load::Ready(l)) => l.list.iter().find(|f| f.name == name).map(|f| f.desc.clone()),
            _ => None,
        }
    }

    /// ffmpeg was built without the filter that runs these plugins (`None` while that is not known yet).
    pub fn lacks_host(&self, kind: PluginKind) -> Option<bool> {
        let name = match kind {
            PluginKind::Ladspa => "ladspa",
            PluginKind::Lv2 => "lv2",
            PluginKind::Frei0r => "frei0r",
        };
        match self.filters() {
            Load::Ready(f) => Some(!f.hosts.iter().any(|h| h == name)),
            _ => None,
        }
    }

    /// The options of filter `name`.
    pub fn help(&self, name: &str) -> Load<FilterHelp> {
        if let Some(l) = self.inner.lock().unwrap().help.get(name) {
            return l.clone();
        }
        self.inner.lock().unwrap().help.insert(name.to_string(), Load::Loading);
        let (n1, n2) = (name.to_string(), name.to_string());
        self.spawn(
            move |ff| filters::filter_help(ff, &n1).map_err(|e| e.to_string()),
            move |i, l| {
                i.help.insert(n2, l);
            },
        );
        self.inner.lock().unwrap().help.get(name).cloned().unwrap_or(Load::Loading)
    }

    /// The installed plugins (every kind, usable or not).
    pub fn plugins(&self) -> Load<Scan> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(l) = &inner.plugins {
            return l.clone();
        }
        inner.plugins = Some(Load::Loading);
        drop(inner);
        let (slot, ctx, dirs) = (self.inner.clone(), self.ctx.clone(), self.dirs.clone());
        std::thread::spawn(move || {
            let r = match std::env::current_exe() {
                Ok(exe) => Load::Ready(Arc::new(plugins::scan(&exe, &dirs))),
                Err(e) => Load::Failed(format!("cannot find FFcut's own program to read the plugins: {e}")),
            };
            slot.lock().unwrap().plugins = Some(r);
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        Load::Loading
    }

    /// Reads the plugin folders again (after installing a plugin).
    pub fn rescan_plugins(&self) {
        self.inner.lock().unwrap().plugins = None;
        self.plugins();
    }

    /// One line about the plugin of an effect, if it is known.
    pub fn plugin_desc(&self, fx: &PluginFx) -> Option<String> {
        match self.plugin(fx) {
            Load::Ready(p) => {
                p.as_ref().as_ref().map(|p| if p.desc.is_empty() { p.kind.label().to_string() } else { format!("{} · {}", p.kind.label(), p.desc) })
            }
            _ => None,
        }
    }

    /// What is known about the plugin of an effect: `Ready` with `None` when it is not installed (any more).
    pub fn plugin(&self, fx: &PluginFx) -> Load<Option<PluginInfo>> {
        match self.plugins() {
            Load::Ready(s) => Load::Ready(Arc::new(s.plugins.iter().find(|p| p.is(fx)).cloned())),
            Load::Loading => Load::Loading,
            Load::Failed(e) => Load::Failed(e),
        }
    }
}
