//! libmpv wrapper. All video decoding/preview goes through here (H.265, ProRes, 10-bit... just work).
//!
//! Rendering: mpv draws into our own FBO the size of the video rectangle; we then blit that
//! FBO into egui's framebuffer inside an `egui::PaintCallback`.

use eframe::egui;
use eframe::egui_glow;
use eframe::glow::{self, HasContext};
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::Mpv;
use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CStr, CString};
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

type GetProc = Arc<dyn Fn(&CStr) -> *const c_void + Send + Sync>;

fn gpa(ctx: &GetProc, name: &str) -> *mut c_void {
    match CString::new(name) {
        Ok(c) => ctx(&c) as *mut c_void,
        Err(_) => std::ptr::null_mut(),
    }
}

struct Fbo {
    fbo: glow::Framebuffer,
    tex: glow::Texture,
    w: i32,
    h: i32,
}

/// Shared with the paint callback (which must be `Send + Sync`).
/// SAFETY: the render context is only ever touched from the GL/UI thread:
/// `render()` is called inside the paint callback, which eframe runs on the main thread.
struct Shared {
    render: ManuallyDrop<RenderContext<'static>>,
    fbo: Mutex<Option<Fbo>>,
    /// The part of the picture (u0, v0, u1, v1 in 0..1, as displayed) the loupe wants to see.
    loupe_req: Mutex<Option<[f32; 4]>>,
    loupe_out: Mutex<Option<LoupeImg>>,
}

/// A piece of the displayed picture, read back for the crop loupe (RGBA, top row first).
pub struct LoupeImg {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    /// The part of the picture it shows (u0, v0, u1, v1).
    pub region: [f32; 4],
}
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

/// Talks to mpv from a helper thread so that the UI never waits for it.
/// * seeks are "latest wins": while mpv is busy decoding, older targets are dropped, the preview catches up as it can;
/// * `time-pos` / `pause` / `eof-reached` are polled there and published as atomics (reading a property can block
///   for as long as a seek takes, which made scrubbing stutter).
struct Bridge {
    pending: Mutex<Option<(f64, bool)>>, // (target, exact)
    wake: Condvar,
    time: AtomicU64,
    paused: AtomicBool,
    eof: AtomicBool,
}

struct MpvRef(&'static Mpv);
// SAFETY: libmpv's client API is documented thread-safe.
unsafe impl Send for MpvRef {}

fn spawn_bridge(mpv: &'static Mpv) -> Arc<Bridge> {
    let b = Arc::new(Bridge {
        pending: Mutex::new(None),
        wake: Condvar::new(),
        time: AtomicU64::new(0f64.to_bits()),
        paused: AtomicBool::new(true),
        eof: AtomicBool::new(false),
    });
    let (bb, m) = (b.clone(), MpvRef(mpv));
    std::thread::Builder::new()
        .name("mpv-bridge".into())
        .spawn(move || {
            let mpv = m.0;
            loop {
                let job = {
                    let mut g = bb.pending.lock().unwrap();
                    if g.is_none() {
                        g = bb.wake.wait_timeout(g, Duration::from_millis(15)).unwrap().0;
                    }
                    g.take()
                };
                if let Some((t, exact)) = job {
                    let mode = if exact { "absolute+exact" } else { "absolute+keyframes" };
                    let _ = mpv.command("seek", &[&format!("{t:.3}"), mode]);
                }
                if let Ok(t) = mpv.get_property::<f64>("time-pos") {
                    bb.time.store(t.to_bits(), Ordering::Relaxed);
                }
                bb.paused.store(mpv.get_property::<bool>("pause").unwrap_or(true), Ordering::Relaxed);
                bb.eof.store(mpv.get_property::<bool>("eof-reached").unwrap_or(false), Ordering::Relaxed);
            }
        })
        .expect("mpv bridge thread");
    b
}

pub struct Player {
    bridge: Arc<Bridge>,
    /// Where the user last asked to go. Shown as the playhead right away (the real position follows).
    target: Cell<Option<(f64, Instant)>>,
    mpv: &'static Mpv, // leaked on purpose: lives as long as the process
    shared: Arc<Shared>,
    pub loaded: bool,
    pub duration: f64,
    /// After any command mpv needs a few UI frames to deliver the new picture.
    seek_gen: Cell<u64>,
    settle_until: Cell<Instant>,
    /// Last rotate/flip pushed to mpv. Re-setting the video filter chain drops the paused frame
    /// (black picture until the next seek), so we only touch mpv when something really changed.
    applied: RefCell<(i64, String)>,
    /// Mirror the drawn picture horizontally / vertically.
    flip: Cell<(bool, bool)>,
    /// mpv drops the first frame if nobody renders it within ~200 ms (slow first paint, e.g. a file
    /// opened from the command line). One re-seek shortly after loading brings the picture back.
    kick_at: Cell<Option<Instant>>,
    /// Further re-seeks after the first one: on a slow first paint one kick is not always enough.
    kicks_left: Cell<u8>,
}

impl Player {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Player, String> {
        let get_proc: GetProc = cc.get_proc_address.clone().ok_or("no OpenGL context (glow renderer required)")?;

        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("hwdec", "auto-safe")?;
            init.set_property("keep-open", "yes")?; // stay on the last frame instead of unloading
            init.set_property("osc", false)?;
            init.set_property("input-default-bindings", false)?;
            init.set_property("input-vo-keyboard", false)?;
            init.set_property("idle", "yes")?;
            init.set_property("audio-display", "no")?;
            // Preview only: plain scaling and no extra passes keep every frame cheap, so the picture is
            // always rendered within mpv's ~200 ms window (slow GL stacks otherwise lose frames).
            for (k, v) in [
                ("scale", "bilinear"),
                ("cscale", "bilinear"),
                ("dscale", "bilinear"),
                ("dither-depth", "no"),
                ("sigmoid-upscaling", "no"),
                ("correct-downscaling", "no"),
                ("interpolation", "no"),
                ("deband", "no"),
                ("hdr-compute-peak", "no"),
                ("sub-auto", "no"),
            ] {
                init.set_property(k, v)?;
            }
            init.set_property("video-timing-offset", 0i64)?; // don't block the UI thread waiting for vsync
            if let Some(f) = std::env::var_os("FFCUT_MPV_LOG") {
                init.set_property("log-file", f.to_string_lossy().as_ref())?;
                init.set_property("msg-level", "all=v")?;
            }
            // Decoder direct-rendering into GPU buffers produced torn/stuck frames after rotate on
            // software GL (llvmpipe). A plain copy costs a few % CPU for preview and is reliable.
            // Override with FFCUT_MPV_DR=auto|yes to experiment.
            let dr = std::env::var("FFCUT_MPV_DR").unwrap_or_else(|_| "no".into());
            init.set_property("vd-lavc-dr", dr.as_str())?;
            if std::env::var_os("FFCUT_NO_AUDIO").is_some() {
                init.set_property("ao", "null")?;
            }
            Ok(())
        })
        .map_err(|e| format!("cannot start libmpv: {e}"))?;
        let mpv: &'static Mpv = Box::leak(Box::new(mpv));

        let mut render = mpv
            .create_render_context(vec![
                RenderParam::ApiType(RenderParamApiType::OpenGl),
                RenderParam::InitParams(OpenGLInitParams { get_proc_address: gpa, ctx: get_proc }),
            ])
            .map_err(|e| format!("cannot create mpv render context: {e}"))?;

        // mpv tells us when a new frame is ready -> wake the UI.
        let egui_ctx = cc.egui_ctx.clone();
        render.set_update_callback(move || egui_ctx.request_repaint());

        Ok(Player {
            bridge: spawn_bridge(mpv),
            target: Cell::new(None),
            mpv,
            shared: Arc::new(Shared { render: ManuallyDrop::new(render), fbo: Mutex::new(None), loupe_req: Mutex::new(None), loupe_out: Mutex::new(None) }),
            loaded: false,
            duration: 0.0,
            seek_gen: Cell::new(0),
            settle_until: Cell::new(Instant::now()),
            applied: RefCell::new((0, String::new())),
            flip: Cell::new((false, false)),
            kick_at: Cell::new(None),
            kicks_left: Cell::new(0),
        })
    }

    /// True for a moment after something changed: the UI keeps repainting so the new frame shows up.
    pub fn settling(&self) -> bool {
        if let Some(t) = self.kick_at.get() {
            if Instant::now() >= t {
                let left = self.kicks_left.get();
                if self.paused() {
                    // re-seek to where we are: makes mpv deliver the frame again
                    let pos = self.real_time().max(0.0);
                    self.queue_seek(pos, true);
                }
                if left > 0 {
                    self.kicks_left.set(left - 1);
                    self.kick_at.set(Some(Instant::now() + Duration::from_millis(900)));
                } else {
                    self.kick_at.set(None);
                }
            }
        }
        self.kick_at.get().is_some() || Instant::now() < self.settle_until.get()
    }
    fn bump(&self) {
        self.settle_until.set(Instant::now() + Duration::from_millis(1200));
    }

    pub fn open(&mut self, path: &Path, duration: f64) {
        let p = path.to_string_lossy();
        // a mix graph of the previous file refers to tracks the new file does not have
        let _ = self.mpv.set_property("lavfi-complex", "");
        let _ = self.mpv.command("loadfile", &[&p, "replace"]);
        let _ = self.mpv.set_property("pause", true);
        self.loaded = true;
        self.target.set(None);
        self.duration = duration;
        self.bump();
        self.kicks_left.set(4);
        self.kick_at.set(Some(Instant::now() + Duration::from_millis(500)));
        self.reset_view();
    }

    /// Counts every seek / frame step the app asked for.
    pub fn seek_gen(&self) -> u64 {
        self.seek_gen.get()
    }

    /// The libmpv handle (for the multi-track audio preview).
    pub fn mpv(&self) -> &Mpv {
        self.mpv
    }

    pub fn reset_view(&self) {
        self.set_transform(0, false, false);
        self.set_loop(None);
    }

    // ---- state ----
    /// Position to show. Right after a seek / during scrubbing it is the requested position (instant, smooth);
    /// otherwise what mpv reports.
    pub fn time(&self) -> f64 {
        if let Some((t, at)) = self.target.get() {
            let el = at.elapsed();
            if el < Duration::from_millis(350) {
                // playing: the requested position moves on, like the real one will
                return if self.paused() { t } else { t + el.as_secs_f64() };
            }
            self.target.set(None);
        }
        self.real_time()
    }
    /// What mpv really shows (may lag behind `time()` while scrubbing).
    pub fn real_time(&self) -> f64 {
        f64::from_bits(self.bridge.time.load(Ordering::Relaxed))
    }
    pub fn paused(&self) -> bool {
        self.bridge.paused.load(Ordering::Relaxed)
    }
    pub fn at_end(&self) -> bool {
        self.bridge.eof.load(Ordering::Relaxed)
    }

    // ---- control ----
    pub fn set_paused(&self, p: bool) {
        if !p && self.at_end() {
            self.seek(0.0);
        }
        let _ = self.mpv.set_property("pause", p);
        self.bridge.paused.store(p, Ordering::Relaxed);
    }
    fn queue_seek(&self, t: f64, exact: bool) {
        self.seek_gen.set(self.seek_gen.get() + 1);
        let t = t.clamp(0.0, self.duration.max(0.0));
        self.bump();
        self.target.set(Some((t, Instant::now())));
        self.bridge.eof.store(false, Ordering::Relaxed);
        *self.bridge.pending.lock().unwrap() = Some((t, exact));
        self.bridge.wake.notify_one();
    }
    pub fn seek(&self, t: f64) {
        self.queue_seek(t, true);
    }
    /// Cheap seek (snaps to keyframes) for scrubbing; finish with `seek` for the exact frame.
    pub fn seek_fast(&self, t: f64) {
        self.queue_seek(t, false);
    }
    pub fn step_frame(&self, forward: bool) {
        self.seek_gen.set(self.seek_gen.get() + 1);
        self.bump();
        self.target.set(None);
        let _ = self.mpv.set_property("pause", true);
        let _ = self.mpv.command(if forward { "frame-step" } else { "frame-back-step" }, &[]);
    }
    pub fn set_volume(&self, v: f64) {
        let _ = self.mpv.set_property("volume", v.clamp(0.0, 130.0));
    }
    pub fn set_mute(&self, m: bool) {
        let _ = self.mpv.set_property("mute", m);
    }
    pub fn set_loop(&self, range: Option<(f64, f64)>) {
        match range {
            Some((a, b)) => {
                let _ = self.mpv.set_property("ab-loop-a", a);
                let _ = self.mpv.set_property("ab-loop-b", b);
            }
            None => {
                let _ = self.mpv.set_property("ab-loop-a", "no");
                let _ = self.mpv.set_property("ab-loop-b", "no");
            }
        }
    }

    /// Live preview of rotate/flip (crop is drawn as an overlay by the UI).
    /// Rotation goes to mpv (GPU, no filter). Flips are applied when the frame is drawn (see `paint`): a
    /// software video filter (`vf=hflip/vflip`) broke hardware-decoded frames (solid blue picture).
    pub fn set_transform(&self, rotate_deg: i64, flip_h: bool, flip_v: bool) {
        if self.flip.get() != (flip_h, flip_v) {
            self.flip.set((flip_h, flip_v));
            self.bump();
        }
        let mut cur = self.applied.borrow_mut();
        if cur.0 == rotate_deg {
            return;
        }
        self.bump();
        let _ = self.mpv.set_property("video-rotate", rotate_deg);
        cur.0 = rotate_deg;
    }

    /// Asks for a piece of the picture for the loupe (it arrives with the next frame).
    pub fn request_loupe(&self, region: [f32; 4]) {
        *self.shared.loupe_req.lock().unwrap() = Some(region);
    }

    pub fn take_loupe(&self) -> Option<LoupeImg> {
        self.shared.loupe_out.lock().unwrap().take()
    }

    /// Draws the current frame into `rect`.
    pub fn paint(&self, ui: &egui::Ui, rect: egui::Rect) {
        let shared = self.shared.clone();
        let (flip_h, flip_v) = self.flip.get();
        let cb = egui_glow::CallbackFn::new(move |info, painter| {
            let vp = info.viewport_in_pixels();
            let (w, h) = (vp.width_px.max(1), vp.height_px.max(1));
            let gl = painter.gl();
            unsafe {
                let mut slot = shared.fbo.lock().unwrap();
                if slot.as_ref().is_none_or(|f| f.w != w || f.h != h) {
                    if let Some(old) = slot.take() {
                        gl.delete_framebuffer(old.fbo);
                        gl.delete_texture(old.tex);
                    }
                    let tex = gl.create_texture().unwrap();
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                    let fbo = gl.create_framebuffer().unwrap();
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
                    *slot = Some(Fbo { fbo, tex, w, h });
                }
                let f = slot.as_ref().unwrap();

                // 1) mpv -> our FBO. `update()` makes mpv pick up the newest decoded frame; without
                //    it render() keeps drawing "no frame" (black) forever.
                let _ = shared.render.update();
                if let Err(e) = shared.render.render::<GetProc>(f.fbo.0.get() as i32, w, h, true) {
                    if std::env::var_os("FFCUT_DEBUG").is_some() {
                        eprintln!("mpv render error: {e:?} (fbo {w}x{h})");
                    }
                }
                if std::env::var_os("FFCUT_DEBUG").is_some() {
                    let st = gl.check_framebuffer_status(glow::FRAMEBUFFER);
                    let err = gl.get_error();
                    let mut px = [0u8; 4];
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(f.fbo));
                    gl.read_pixels(w / 2, h / 2, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
                    eprintln!("fbo status {st:#x} gl err {err:#x} {w}x{h} centre pixel {px:?}");
                }

                // 1b) the crop loupe: read back a small part of the picture (as displayed, so flips are undone)
                if let Some(region) = shared.loupe_req.lock().unwrap().take() {
                    let [mut u0, mut v0, mut u1, mut v1] = region;
                    if flip_h {
                        (u0, u1) = (1.0 - u1, 1.0 - u0);
                    }
                    if flip_v {
                        (v0, v1) = (1.0 - v1, 1.0 - v0);
                    }
                    let px0 = ((u0 * w as f32).floor() as i32).clamp(0, w - 1);
                    let px1 = ((u1 * w as f32).ceil() as i32).clamp(px0 + 1, w);
                    let py0 = (((1.0 - v1) * h as f32).floor() as i32).clamp(0, h - 1);
                    let py1 = (((1.0 - v0) * h as f32).ceil() as i32).clamp(py0 + 1, h);
                    let (cw, ch) = ((px1 - px0) as usize, (py1 - py0) as usize);
                    let mut buf = vec![0u8; cw * ch * 4];
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(f.fbo));
                    gl.read_pixels(px0, py0, cw as i32, ch as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut buf)));
                    // GL rows run bottom-up: put the top row first, and mirror what the display mirrors
                    let mut rgba = Vec::with_capacity(buf.len());
                    for row in 0..ch {
                        let src = if flip_v { row } else { ch - 1 - row };
                        let line = &buf[src * cw * 4..(src + 1) * cw * 4];
                        if flip_h {
                            for px in line.as_chunks::<4>().0.iter().rev() {
                                rgba.extend_from_slice(px);
                            }
                        } else {
                            rgba.extend_from_slice(line);
                        }
                    }
                    *shared.loupe_out.lock().unwrap() = Some(LoupeImg { w: cw, h: ch, rgba, region });
                }

                // 2) our FBO -> screen, at the egui viewport position
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(f.fbo));
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, painter.intermediate_fbo());
                // swapping the destination corners mirrors the picture
                let (x0, x1) = if flip_h { (vp.left_px + w, vp.left_px) } else { (vp.left_px, vp.left_px + w) };
                let (y0, y1) = if flip_v { (vp.from_bottom_px + h, vp.from_bottom_px) } else { (vp.from_bottom_px, vp.from_bottom_px + h) };
                gl.blit_framebuffer(0, 0, w, h, x0, y0, x1, y1, glow::COLOR_BUFFER_BIT, glow::NEAREST);
                gl.bind_framebuffer(glow::FRAMEBUFFER, painter.intermediate_fbo());
            }
        });
        ui.painter().add(egui::PaintCallback { rect, callback: Arc::new(cb) });
    }
}
