//! Web graphics context loss (game.js `webglcontextlost` and
//! `webglcontextrestored`).
//!
//! The browser drops the WebGL context under GPU memory pressure, after a
//! GPU process reset or when iOS reclaims a backgrounded page. Every GPU object
//! goes with it. game.js stopped drawing and simulating and showed "Restoring
//! graphics…". Once the browser restored the context, it rebuilt its GL
//! resources from the CPU data it kept, and the ride carried on with its
//! position, clock, race and room.
//!
//! The engine does the same in place. While the context is lost, it draws no
//! frame, takes no input and runs no load completion that would touch the
//! GPU; those messages wait. The ride is held as on blur: sound suspends and
//! the room stays connected through the presence timer. When the context is
//! restored, a new device opens on the same canvas. The scene, HUD and menus
//! then rebuild their GPU resources from the retained venue, ride and UI
//! state (`SceneRenderer::rebuild`, `Overlay::rebuild_gpu`,
//! `Front::release_gpu`). If no device opens, the page falls back to reopening
//! the ride's link (`fury-resume`).
use crate::{App, Message, status};
use atv_render::Gpu;
use std::{
    cell::RefCell,
    sync::{Arc, Mutex, Weak},
};
use wasm_bindgen::{JsCast, closure::Closure};
use winit::{event_loop::EventLoopProxy, window::Window};

type LostFlag = Arc<Mutex<Option<String>>>;

thread_local! {
    /// Every device opened on the page: a loss marks them all, including one
    /// a load in flight holds (its result then rebuilds on the new device).
    static OPENED: RefCell<Vec<Weak<Mutex<Option<String>>>>> = const { RefCell::new(Vec::new()) };
}

/// The context-loss state the app keeps.
#[derive(Default)]
pub(crate) struct Loss {
    /// Between `webglcontextlost` and the new device.
    pub lost: bool,
    /// Messages held while lost (they may touch the GPU), replayed after.
    deferred: Vec<Message>,
    /// The new device came with no ride on screen: a load (or the boot)
    /// that held the lost device moves its result onto it.
    loading: bool,
}

/// Notes a newly opened device so a later loss can mark it.
pub(crate) fn opened(gpu: &Gpu) {
    let flag: LostFlag = gpu.lost_flag();
    OPENED.with_borrow_mut(|list| {
        list.retain(|w| w.strong_count() > 0);
        list.push(Arc::downgrade(&flag));
    });
}

fn mark_lost() {
    OPENED.with_borrow(|list| {
        for flag in list.iter().filter_map(Weak::upgrade) {
            if let Ok(mut lost) = flag.lock() {
                lost.get_or_insert_with(|| "WebGL context lost".into());
            }
        }
    });
}

/// A device opened before the last loss.
fn stale(gpu: &Gpu) -> bool {
    gpu.lost().is_some()
}

/// Listens for the canvas's context loss and restore (game.js listens on
/// its canvas too). The loss listener's `preventDefault` asks the browser to
/// restore the context.
pub(crate) fn watch(window: &Window, proxy: EventLoopProxy<Message>) {
    use winit::platform::web::WindowExtWebSys;
    let Some(canvas) = window.canvas() else {
        return;
    };
    {
        // Only asked once wgpu has made the context (on a wgpu error).
        let canvas = canvas.clone();
        atv_render::set_context_probe(move || {
            canvas
                .get_context("webgl2")
                .ok()
                .flatten()
                .and_then(|c| c.dyn_into::<web_sys::WebGl2RenderingContext>().ok())
                .is_some_and(|c| c.is_context_lost())
        });
    }
    let lost_proxy = proxy.clone();
    let lost = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        event.prevent_default();
        let _ = lost_proxy.send_event(Message::ContextLost);
    });
    let restored = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
        let _ = proxy.send_event(Message::ContextRestored);
    });
    let _ =
        canvas.add_event_listener_with_callback("webglcontextlost", lost.as_ref().unchecked_ref());
    let _ = canvas.add_event_listener_with_callback(
        "webglcontextrestored",
        restored.as_ref().unchecked_ref(),
    );
    lost.forget();
    restored.forget();
}

/// `#status` as the shell showed it: visible while graphics restore.
fn restoring(on: bool) {
    status(if on {
        "Restoring graphics…"
    } else {
        "Graphics restored."
    });
    if let Some(element) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("status"))
    {
        element.set_class_name(if on { "fatal" } else { "" });
    }
}

impl App {
    /// Handles the context messages and holds GPU work while the context is
    /// lost; a message for the normal handler comes back.
    pub(crate) fn context_filter(&mut self, message: Message) -> Option<Message> {
        match message {
            Message::ContextLost => {
                self.context_lost();
                None
            }
            Message::ContextRestored => {
                self.context_restored();
                None
            }
            Message::GpuRestored(gpu) => {
                self.context_back(*gpu);
                None
            }
            message if self.context.lost => match message {
                // The room's presence and the HUD-free messages go on.
                Message::PresenceTick
                | Message::FreeRideRiders(_)
                | Message::Notice(_)
                | Message::LobbyNotice(_)
                | Message::SoundFailed(_)
                | Message::MenuTune(_)
                | Message::RemoteEngine(..)
                | Message::PeerModel(_)
                | Message::FrontText(..)
                | Message::FrontFocus(_) => Some(message),
                message => {
                    self.context.deferred.push(message);
                    None
                }
            },
            Message::Ready(mut ready) => {
                // A ride loaded on the lost device moves onto the new one.
                if stale(&ready.gpu)
                    && let Some(gpu) = self.gpu.take()
                {
                    self.context.loading = false;
                    drop(std::mem::replace(&mut ready.gpu, gpu));
                    rebuild(&mut ready, None);
                }
                Some(Message::Ready(ready))
            }
            Message::FrontReady(mut booted) => {
                if stale(&booted.gpu)
                    && let Some(gpu) = self.gpu.take()
                {
                    self.context.loading = false;
                    booted.gpu = gpu;
                }
                Some(Message::FrontReady(booted))
            }
            Message::Failed(error) if self.context.loading && self.gpu.is_some() => {
                // The load that held the lost device failed: the menus take
                // the new device, as after any failed load.
                self.context.loading = false;
                let gpu = self.gpu.take().map(Box::new);
                status(&error);
                self.usage.failed();
                gpu.map(Message::FrontGpu)
            }
            message => Some(message),
        }
    }

    /// `webglcontextlost`: stop drawing, hold the ride and say so.
    fn context_lost(&mut self) {
        if self.context.lost {
            return;
        }
        self.context.lost = true;
        mark_lost();
        // Menus draw nothing until the new device: their GPU objects go now,
        // while the browser ignores the calls.
        self.gpu = None;
        if let Some(front) = &self.front {
            front.borrow_mut().release_gpu();
        }
        restoring(true);
        // Input clears and the ride's sound suspends, as on blur.
        self.sync_activity();
    }

    /// `webglcontextrestored`: open a device on the canvas again.
    fn context_restored(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let display = self.display.clone();
        self.spawn(crate::open_gpu(window, display), |result| match result {
            Ok(gpu) => Message::GpuRestored(Box::new(gpu)),
            Err(error) => {
                status(&format!("Unable to restore graphics: {error}"));
                reload();
                Message::Notice(String::new())
            }
        });
    }

    /// The new device: rebuild in place and carry on.
    fn context_back(&mut self, mut gpu: Gpu) {
        if let Some(window) = &self.window {
            let size = crate::render_size(window);
            gpu.resize(size.width, size.height);
        }
        self.context.lost = false;
        if let Some(ready) = &mut self.ready {
            // The old objects go after the new ones exist; WebGL ignores
            // calls on objects of a lost context.
            let old = std::mem::replace(&mut ready.gpu, gpu);
            rebuild(ready, self.overlay.as_mut());
            drop(old);
        } else {
            // The menus, or a load (or the boot) still running on the lost
            // device: its result takes this one.
            self.gpu = Some(gpu);
            self.context.loading = true;
        }
        if let Some(front) = &self.front {
            front.borrow_mut().release_gpu();
        }
        // The Jukebox art registers again on its next showing.
        self.requested.remove("jukebox-art");
        restoring(false);
        self.resize_surface();
        self.sync_activity();
        self.last = web_time::Instant::now();
        for message in std::mem::take(&mut self.context.deferred) {
            let _ = self.proxy.send_event(message);
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Rebuilds a ride's (or venue view's) GPU resources on its new device.
fn rebuild(ready: &mut crate::Ready, overlay: Option<&mut crate::overlay::Overlay>) {
    let crate::Ready {
        gpu,
        venue,
        scene,
        riding,
        split,
        ..
    } = ready;
    let result = match riding {
        Some(riding) => riding.restore_gpu(scene, gpu, venue),
        None => {
            let start = venue.scene.start_position;
            scene.rebuild(gpu, venue, Some([start[0], start[2]]));
            Ok(())
        }
    };
    // Split screen's player 2 rides in a rider slot of the new scene.
    let result = result.and_then(|()| match split {
        Some(second) => second.restore(gpu, venue, scene),
        None => Ok(()),
    });
    if let Err(error) = result {
        status(&format!("Restoring graphics: {error}"));
    }
    if let Some(overlay) = overlay {
        overlay.rebuild_gpu(&gpu.device, &gpu.queue, gpu.config.format, venue);
    }
}

/// No new device: reopen the current ride's link (the engine keeps it in
/// `fury-resume`; a room's invite rejoins its slot) or this page.
fn reload() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let resume = window
        .session_storage()
        .ok()
        .flatten()
        .and_then(|s| s.get_item("fury-resume").ok().flatten());
    let location = window.location();
    let target = resume.or_else(|| location.href().ok());
    if let Some(target) = target {
        let _ = location.replace(&target);
    }
}
