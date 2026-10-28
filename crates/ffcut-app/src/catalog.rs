//! What the user's ffmpeg offers as effects: its filters and their options, read in the background the first
//! time they are needed and kept for the session (a new ffmpeg in the settings starts a new catalog).

use eframe::egui;
use ffcut_core::filters::{self, FilterHelp, FilterInfo, Media};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub enum Load<T> {
    Loading,
    Ready(Arc<T>),
    Failed(String),
}

#[derive(Default)]
struct Inner {
    filters: Option<Load<Vec<FilterInfo>>>,
    help: HashMap<String, Load<FilterHelp>>,
}

/// Cheap to clone: every clone shares the same cache.
#[derive(Clone, Default)]
pub struct Catalog {
    ffmpeg: Option<PathBuf>,
    ctx: Option<egui::Context>,
    inner: Arc<Mutex<Inner>>,
}

impl Catalog {
    pub fn new(ffmpeg: Option<PathBuf>, ctx: &egui::Context) -> Self {
        Catalog { ffmpeg, ctx: Some(ctx.clone()), inner: Default::default() }
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
    pub fn filters(&self) -> Load<Vec<FilterInfo>> {
        if let Some(l) = &self.inner.lock().unwrap().filters {
            return l.clone();
        }
        self.inner.lock().unwrap().filters = Some(Load::Loading);
        self.spawn(|ff| filters::list_filters(ff).map_err(|e| e.to_string()), |i, l| i.filters = Some(l));
        self.inner.lock().unwrap().filters.clone().unwrap_or(Load::Loading)
    }

    /// The filters of one kind, once the list is in.
    pub fn filters_of(&self, media: Media) -> Load<Vec<FilterInfo>> {
        match self.filters() {
            Load::Ready(l) => Load::Ready(Arc::new(l.iter().filter(|f| f.media == media).cloned().collect())),
            Load::Loading => Load::Loading,
            Load::Failed(e) => Load::Failed(e),
        }
    }

    /// One line about a filter (from the list), if it is in.
    pub fn desc(&self, name: &str) -> Option<String> {
        match &self.inner.lock().unwrap().filters {
            Some(Load::Ready(l)) => l.iter().find(|f| f.name == name).map(|f| f.desc.clone()),
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
}
