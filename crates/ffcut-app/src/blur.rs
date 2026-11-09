//! Blur behind the see-through window. FFcut only asks for it; the desktop decides how strong it is, or
//! whether to blur at all. KDE Plasma takes the request on X11 (a window property) and on Wayland (its
//! blur protocol), and Wayland compositors with `ext-background-effect` take it too. Others blur
//! see-through windows by their own rules (picom, Hyprland, SwayFX) or not at all (GNOME).
//!
//! The dialogs (separate windows) get the blur on X11 only: there FFcut finds them among the windows of
//! the screen, while on Wayland the windowing library keeps their surfaces to itself.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub struct Blur {
    imp: Imp,
    on: bool,
    dialogs: Dialogs,
}

/// The request on the dialogs: what was asked last, and whether all of them have it.
#[derive(Default)]
struct Dialogs {
    asked: Option<(bool, usize)>,
    tries: u32,
    done: bool,
}

enum Imp {
    None,
    #[cfg(all(unix, not(target_os = "macos")))]
    X11(Box<x11::Kde>),
    #[cfg(all(unix, not(target_os = "macos")))]
    Wayland(Box<wl::Wl>),
}

impl Blur {
    /// Finds out whether the desktop of `window` takes blur requests.
    pub fn new(window: &(impl HasWindowHandle + HasDisplayHandle)) -> Blur {
        Blur { imp: Self::connect(window).unwrap_or(Imp::None), on: false, dialogs: Dialogs::default() }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn connect(window: &(impl HasWindowHandle + HasDisplayHandle)) -> Option<Imp> {
        use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
        let w = window.window_handle().ok()?.as_raw();
        let d = window.display_handle().ok()?.as_raw();
        match (w, d) {
            (RawWindowHandle::Xlib(h), _) => x11::Kde::new(h.window as u32).map(|x| Imp::X11(Box::new(x))),
            (RawWindowHandle::Xcb(h), _) => x11::Kde::new(h.window.get()).map(|x| Imp::X11(Box::new(x))),
            // SAFETY: both come from the live window this app draws in, which outlives the app's state
            (RawWindowHandle::Wayland(h), RawDisplayHandle::Wayland(d)) => {
                unsafe { wl::Wl::new(d.display.as_ptr(), h.surface.as_ptr()) }.map(|w| Imp::Wayland(Box::new(w)))
            }
            _ => None,
        }
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    fn connect(_: &(impl HasWindowHandle + HasDisplayHandle)) -> Option<Imp> {
        None
    }

    /// How the request reaches the desktop; `None` = it does not take one.
    pub fn via(&self) -> Option<&'static str> {
        match &self.imp {
            Imp::None => None,
            #[cfg(all(unix, not(target_os = "macos")))]
            Imp::X11(_) => Some("KDE, X11"),
            #[cfg(all(unix, not(target_os = "macos")))]
            Imp::Wayland(w) => Some(w.name()),
        }
    }

    /// The dialogs get the blur too (X11 only, see the top of this file).
    pub fn reaches_dialogs(&self) -> bool {
        match &self.imp {
            #[cfg(all(unix, not(target_os = "macos")))]
            Imp::X11(_) => true,
            _ => false,
        }
    }

    /// Asks for the blur or takes the request back. The change shows with the next frame drawn.
    pub fn set(&mut self, on: bool) {
        if on == self.on {
            return;
        }
        self.on = on;
        match &mut self.imp {
            Imp::None => {}
            #[cfg(all(unix, not(target_os = "macos")))]
            Imp::X11(x) => x.set(on),
            #[cfg(all(unix, not(target_os = "macos")))]
            Imp::Wayland(w) => w.set(on),
        }
    }

    /// Asks for the blur on the app's other windows (the dialogs), or takes it back; `count` = how many
    /// there are. A new window shows up on the screen a moment after it is made: true = not all of them
    /// were found yet, call again with a later frame.
    pub fn sync_dialogs(&mut self, on: bool, count: usize) -> bool {
        let d = &mut self.dialogs;
        if d.asked != Some((on, count)) {
            *d = Dialogs { asked: Some((on, count)), tries: 0, done: false };
        }
        if d.done {
            return false;
        }
        match &self.imp {
            #[cfg(all(unix, not(target_os = "macos")))]
            Imp::X11(x) => {
                let wins = x.others();
                for &w in &wins {
                    x.apply(w, on);
                }
                d.tries += 1;
                // a dialog that never shows up (no window manager to list it) is not waited for forever
                d.done = wins.len() >= count || d.tries >= 40;
                !d.done
            }
            _ => {
                d.done = true;
                false
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod x11 {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode, Window};
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    /// KWin's blur on X11: an (empty = whole window) region in a property of the window.
    pub struct Kde {
        conn: RustConnection,
        win: Window,
        atom: u32,
        root: Window,
        client_list: u32,
    }

    impl Kde {
        pub fn new(win: Window) -> Option<Kde> {
            let (conn, screen) = x11rb::connect(None).ok()?;
            let atom = conn.intern_atom(false, b"_KDE_NET_WM_BLUR_BEHIND_REGION").ok()?.reply().ok()?.atom;
            let root = conn.setup().roots.get(screen)?.root;
            // KWin announces its blur effect with this property on the root window (KDE's own
            // KWindowEffects checks it the same way)
            let props = conn.list_properties(root).ok()?.reply().ok()?.atoms;
            if !props.contains(&atom) {
                return None;
            }
            let client_list = conn.intern_atom(false, b"_NET_CLIENT_LIST").ok()?.reply().ok()?.atom;
            Some(Kde { conn, win, atom, root, client_list })
        }

        pub fn set(&self, on: bool) {
            self.apply(self.win, on);
        }

        pub fn apply(&self, win: Window, on: bool) {
            if on {
                let _ = self.conn.change_property32(PropMode::REPLACE, win, self.atom, AtomEnum::CARDINAL, &[]);
            } else {
                let _ = self.conn.delete_property(win, self.atom);
            }
            let _ = self.conn.flush();
        }

        /// The app's other top-level windows. Every window the windowing library makes comes from its
        /// one connection, and the server gives all ids of a connection the same high bits.
        pub fn others(&self) -> Vec<Window> {
            let mask = self.conn.setup().resource_id_mask;
            let ours = |w: &Window| *w != self.win && *w & !mask == self.win & !mask;
            // the window manager lists the windows it manages (it puts them into frames of its own);
            // without one they are children of the root
            let listed = self
                .conn
                .get_property(false, self.root, self.client_list, AtomEnum::WINDOW, 0, 4096)
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().map(|v| v.collect::<Vec<_>>()))
                .unwrap_or_default();
            let children = self.conn.query_tree(self.root).ok().and_then(|c| c.reply().ok()).map(|r| r.children).unwrap_or_default();
            let mut v: Vec<Window> = listed.into_iter().chain(children).filter(ours).collect();
            v.sort_unstable();
            v.dedup();
            v
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod wl {
    use std::ffi::c_void;
    use wayland_client::backend::{Backend, ObjectId};
    use wayland_client::globals::{registry_queue_init, GlobalListContents};
    use wayland_client::protocol::{wl_compositor::WlCompositor, wl_region::WlRegion, wl_registry, wl_surface::WlSurface};
    use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
    use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{self, Capability, ExtBackgroundEffectManagerV1};
    use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
    use wayland_protocols_plasma::blur::client::{org_kde_kwin_blur::OrgKdeKwinBlur, org_kde_kwin_blur_manager::OrgKdeKwinBlurManager};

    #[derive(Default)]
    struct State {
        can_blur: bool,
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
        fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
    }
    impl Dispatch<ExtBackgroundEffectManagerV1, ()> for State {
        fn event(st: &mut Self, _: &ExtBackgroundEffectManagerV1, ev: ext_background_effect_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let ext_background_effect_manager_v1::Event::Capabilities { flags: WEnum::Value(c) } = ev {
                st.can_blur = c.contains(Capability::Blur);
            }
        }
    }
    delegate_noop!(State: ExtBackgroundEffectSurfaceV1);
    delegate_noop!(State: WlCompositor);
    delegate_noop!(State: WlRegion);
    delegate_noop!(State: OrgKdeKwinBlurManager);
    delegate_noop!(State: OrgKdeKwinBlur);

    enum Via {
        /// The common protocol: a blur region on the surface.
        Ext { mgr: ExtBackgroundEffectManagerV1, compositor: WlCompositor, fx: Option<ExtBackgroundEffectSurfaceV1> },
        /// KDE's own one (also in older Plasma).
        Kde { mgr: OrgKdeKwinBlurManager, blur: Option<OrgKdeKwinBlur> },
    }

    /// Requests on the app's own Wayland connection, through a queue of their own.
    pub struct Wl {
        conn: Connection,
        // the objects made here belong to this queue: it lives as long as they do
        queue: EventQueue<State>,
        surface: WlSurface,
        via: Via,
    }

    impl Wl {
        /// # Safety
        /// `display` and `surface` are the live `wl_display` and `wl_surface` of the window.
        pub unsafe fn new(display: *mut c_void, surface: *mut c_void) -> Option<Wl> {
            // a foreign display is only borrowed: dropping the connection does not close it
            let conn = Connection::from_backend(unsafe { Backend::from_foreign_display(display.cast()) });
            let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
            let qh = queue.handle();
            let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }.ok()?;
            let surface = WlSurface::from_id(&conn, id).ok()?;
            if let Ok(mgr) = globals.bind::<ExtBackgroundEffectManagerV1, _, _>(&qh, 1..=1, ()) {
                // the capabilities come as an event right after the bind
                let mut st = State::default();
                if queue.roundtrip(&mut st).is_ok() && st.can_blur {
                    if let Ok(compositor) = globals.bind::<WlCompositor, _, _>(&qh, 1..=4, ()) {
                        return Some(Wl { conn, queue, surface, via: Via::Ext { mgr, compositor, fx: None } });
                    }
                }
                mgr.destroy();
            }
            let mgr = globals.bind::<OrgKdeKwinBlurManager, _, _>(&qh, 1..=1, ()).ok()?;
            Some(Wl { conn, queue, surface, via: Via::Kde { mgr, blur: None } })
        }

        pub fn name(&self) -> &'static str {
            match self.via {
                Via::Ext { .. } => "Wayland, ext-background-effect",
                Via::Kde { .. } => "KDE, Wayland",
            }
        }

        /// Applies with the next commit of the surface, which the next frame drawn makes.
        pub fn set(&mut self, on: bool) {
            let qh = self.queue.handle();
            match &mut self.via {
                Via::Ext { mgr, compositor, fx } => {
                    let fx = fx.get_or_insert_with(|| mgr.get_background_effect(&self.surface, &qh, ()));
                    if on {
                        // the compositor clips the region to the surface
                        let region = compositor.create_region(&qh, ());
                        region.add(0, 0, 1 << 15, 1 << 15);
                        fx.set_blur_region(Some(&region));
                        region.destroy();
                    } else {
                        fx.set_blur_region(None);
                    }
                }
                Via::Kde { mgr, blur } => {
                    if on {
                        // no region = the whole surface
                        let b = mgr.create(&self.surface, &qh, ());
                        b.commit();
                        *blur = Some(b);
                    } else {
                        mgr.unset(&self.surface);
                        if let Some(b) = blur.take() {
                            b.release();
                        }
                    }
                }
            }
            let _ = self.conn.flush();
        }
    }
}
