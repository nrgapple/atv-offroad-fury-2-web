//! Android: a NativeActivity (winit's android-activity backend, no Java or
//! Gradle) runs the same [`crate::App`] as the desktop and web builds.
//!
//! - Content: the APK carries none. On launch the hosted package the browser
//!   autoloads (furyremastered.com `/content/`) is mirrored into the app's
//!   internal storage ([`crate::content_sync`]) behind a progress screen, then
//!   read with `Store::Directory`. A developer may `adb push` a `content.pack`
//!   or a package directory (`content/manifest.json`) to the app's external
//!   files directory instead.
//! - Lifecycle: Android destroys the window's surface whenever the app leaves
//!   the screen. `Suspended` releases every GPU surface (the devices and their
//!   resources stay); `Resumed` makes new surfaces on the new window.
//! - Touch is the control scheme (the viewport reports a coarse, hover-less
//!   pointer, like a phone browser); the Back key acts as Escape.
//!
//! See research/ANDROID.md.
use crate::content_sync::{self, Background, Progress};
use atv_content::Store;
use atv_render::Gpu;
use atv_ui::{Color, DisplayList, Font, Fonts, Rect, Renderer, TextStyle};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use web_time::Instant;
use winit::{
    event::{TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
    platform::android::activity::AndroidApp,
    window::Window,
};

static ACTIVITY: OnceLock<AndroidApp> = OnceLock::new();

/// The package directory to sync before the game boots (None: a developer
/// pushed content, or the sync already ran).
static PENDING_SYNC: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The running activity (window flags, storage paths).
pub fn activity() -> Option<&'static AndroidApp> {
    ACTIVITY.get()
}

/// The entry point android-activity calls on its own thread.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    log_to_logcat();
    let internal = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp/fury"));
    let _ = std::fs::create_dir_all(&internal);
    // Settings live in the app's private storage (atv-ui settings.rs reads
    // FURY_SETTINGS). Set before any other thread of ours starts.
    // SAFETY: no thread of this process reads the environment concurrently.
    unsafe { std::env::set_var("FURY_SETTINGS", internal.join("settings.json")) };
    let store = choose_store(&app, &internal);
    // The screen stays on while the game is in front: controller and
    // keyboard play produce no touches. Set before the event loop runs:
    // android-activity 0.6 takes a write lock for window flags that its own
    // event dispatch holds, so changing them later from a ride deadlocks.
    {
        use winit::platform::android::activity::WindowManagerFlags;
        app.set_window_flags(WindowManagerFlags::KEEP_SCREEN_ON, WindowManagerFlags::empty());
    }
    let _ = ACTIVITY.set(app);
    crate::run(crate::Options {
        store,
        venue: "env01".into(),
        camera: None,
        weather: None,
        steps: None,
        graphics: atv_render::Graphics::default(),
        graphics_set: false,
        crash: atv_game::crash::CrashMode::Original,
        camera_choice: (true, false),
        capture: None,
        capture_ui: false,
        gpu_frames: None,
        ride: true,
        selection: Default::default(),
        appearance: Default::default(),
        race: None,
        script: None,
        ui_backdrop: None,
        sound: Some((atv_game::audio::ride::SoundMode::Original, true)),
        autodrive: None,
        profile: false,
        bench: None,
        front: Some(crate::FrontStart::default()),
        room: None,
        relay: Some("https://furyremastered.com".into()),
    });
}

/// A developer's pushed content first (external files directory, writable
/// with `adb push`), else the synced package in internal storage.
fn choose_store(app: &AndroidApp, internal: &std::path::Path) -> Store {
    if let Some(external) = app.external_data_path() {
        let pack = external.join("content.pack");
        if pack.is_file() {
            match atv_content::pack::Pack::open(&pack) {
                Ok(pack) => {
                    eprintln!(
                        "fury: content from {}",
                        external.join("content.pack").display()
                    );
                    return Store::Pack(pack);
                }
                Err(error) => eprintln!("fury: {}: {error}", pack.display()),
            }
        }
        let dir = external.join("content");
        if dir.join("manifest.json").is_file() {
            eprintln!("fury: content from {}", dir.display());
            return Store::Directory(dir);
        }
    }
    let dir = internal.join("content");
    *PENDING_SYNC.lock().unwrap() = Some(dir.clone());
    Store::Directory(dir)
}

/// Routes stdout and stderr (status lines, panics) to logcat under "fury".
fn log_to_logcat() {
    use std::io::{BufRead, BufReader};
    use std::os::fd::FromRawFd;
    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(
            priority: libc::c_int,
            tag: *const libc::c_char,
            text: *const libc::c_char,
        ) -> libc::c_int;
    }
    let mut fds = [0; 2];
    // SAFETY: plain POSIX calls on descriptors this function owns.
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return;
        }
        libc::dup2(fds[1], 1);
        libc::dup2(fds[1], 2);
        libc::close(fds[1]);
    }
    // SAFETY: the read end is ours alone from here on.
    let reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if let Ok(text) = std::ffi::CString::new(line) {
                // SAFETY: both strings are NUL terminated; 4 is ANDROID_LOG_INFO.
                unsafe { __android_log_write(4, c"fury".as_ptr(), text.as_ptr()) };
            }
        }
    });
}

/// The first-launch content download, drawn over black with the UI renderer.
pub struct SyncScreen {
    job: Background,
    gpu: Option<Gpu>,
    ui: Option<(Renderer, Fonts)>,
    progress: Progress,
    failed: Option<String>,
    /// Bytes done at the last frame drawn, and when (the speed line).
    rate: Option<(u64, Instant, f64)>,
    drawn: Option<(u64, bool)>,
}

const HOSTED_MB: &str = "about 540 MB";

impl SyncScreen {
    fn start(dir: PathBuf) -> Self {
        Self {
            job: Background::start(
                Arc::new(content_sync::Https::new(content_sync::HOSTED)),
                dir,
            ),
            gpu: None,
            ui: None,
            progress: Progress::default(),
            failed: None,
            rate: None,
            drawn: None,
        }
    }

    /// A GPU on the window (first show), or a new surface for it (return
    /// from the background).
    fn attach(&mut self, window: &Arc<Window>, display: winit::event_loop::OwnedDisplayHandle) {
        let size = window.inner_size();
        match &mut self.gpu {
            Some(gpu) => {
                if let Err(error) = gpu.recreate_surface(window.clone(), size.width, size.height) {
                    eprintln!("fury: download screen surface: {error}");
                }
            }
            None => match pollster::block_on(crate::open_gpu(window.clone(), display)) {
                Ok(gpu) => {
                    let renderer = Renderer::new(&gpu.device, gpu.config.format);
                    self.ui = Some((renderer, Fonts::new()));
                    self.gpu = Some(gpu);
                }
                Err(error) => eprintln!("fury: download screen graphics: {error}"),
            },
        }
        self.drawn = None;
    }

    fn detach(&mut self) {
        if let Some(gpu) = &mut self.gpu {
            gpu.release_surface();
        }
    }

    /// The sync finished with a complete package.
    fn poll(&mut self) -> bool {
        let (progress, outcome) = self.job.poll();
        self.progress = progress;
        match outcome {
            Some(Ok(())) => true,
            Some(Err(error)) => {
                if self.failed.is_none() {
                    eprintln!("fury: content download failed: {error}");
                }
                self.failed = Some(error);
                false
            }
            None => false,
        }
    }

    /// A tap after a failure starts the download again (it resumes).
    fn retry(&mut self) {
        if self.failed.take().is_some() {
            let dir = self.job.dir.clone();
            self.job = Background::start(
                Arc::new(content_sync::Https::new(content_sync::HOSTED)),
                dir,
            );
            self.rate = None;
            self.drawn = None;
        }
    }

    fn lines(&mut self) -> (String, String, Option<f32>) {
        let p = &self.progress;
        let mb = |bytes: u64| bytes as f64 / 1e6;
        if let Some(error) = &self.failed {
            let mut error = error.clone();
            if error.len() > 110 {
                error.truncate(110);
                error.push('…');
            }
            return (
                "The game content could not download.".into(),
                format!("{error}. Tap to try again."),
                (p.total > 0).then(|| p.done as f32 / p.total as f32),
            );
        }
        if p.total == 0 {
            return (
                "Checking the game content…".into(),
                format!(
                    "The first launch downloads the original game's runtime resources ({HOSTED_MB}) from furyremastered.com. Wi-Fi recommended."
                ),
                None,
            );
        }
        let now = Instant::now();
        let speed = match self.rate {
            Some((bytes, at, speed)) => {
                let seconds = (now - at).as_secs_f64();
                if seconds >= 1. {
                    let measured = (p.done.saturating_sub(bytes)) as f64 / seconds;
                    let speed = if speed > 0. {
                        speed * 0.6 + measured * 0.4
                    } else {
                        measured
                    };
                    self.rate = Some((p.done, now, speed));
                    speed
                } else {
                    speed
                }
            }
            None => {
                self.rate = Some((p.done, now, 0.));
                0.
            }
        };
        let fraction = p.done as f32 / p.total.max(1) as f32;
        let detail = if speed > 0. {
            format!(
                "{:.0} of {:.0} MB · {:.1} MB/s · stays on this device; later launches skip this.",
                mb(p.done),
                mb(p.total),
                speed / 1e6
            )
        } else {
            format!(
                "{:.0} of {:.0} MB · stays on this device; later launches skip this.",
                mb(p.done),
                mb(p.total)
            )
        };
        (
            format!("Downloading the game content… {:.0}%", fraction * 100.),
            detail,
            Some(fraction),
        )
    }

    fn draw(&mut self, window: &Window) {
        let key = (self.progress.done, self.failed.is_some());
        if self.drawn == Some(key) {
            return;
        }
        let (headline, detail, fraction) = self.lines();
        let (Some(gpu), Some((renderer, fonts))) = (&self.gpu, &mut self.ui) else {
            return;
        };
        if gpu.surface_stale() {
            return;
        }
        let scale = window.scale_factor().max(0.5) as f32;
        let (width, height) = (gpu.config.width, gpu.config.height);
        let (w, h) = (width as f32 / scale, height as f32 / scale);
        let gold = Color::hex("#e5d360");
        let silver = Color::hex("#b9b9b9");
        let mut list = DisplayList::default();
        list.rect(Rect::new(0., 0., w, h), Color::hex("#080808"));
        let mut centred = |list: &mut DisplayList, text: &str, font: Font, color: Color, y: f32| {
            let style = TextStyle::new(font, color);
            let width = fonts.measure(text, &font, 0.).width.min(w - 32.);
            let style = TextStyle {
                max_width: Some(w - 32.),
                ..style
            };
            list.text(text, &style, ((w - width) / 2.).max(16.), y);
        };
        let middle = h / 2.;
        centred(
            &mut list,
            "FURY REMASTERED",
            Font::display(40., 800, true),
            gold,
            middle - 54.,
        );
        centred(
            &mut list,
            &headline,
            Font::display(20., 600, false),
            Color::hex("#e9e9e9"),
            middle - 6.,
        );
        let bar = Rect::new(w * 0.15, middle + 14., w * 0.7, 10.);
        list.rect(bar, Color::hex("#333333"));
        if let Some(fraction) = fraction {
            let fill = if self.failed.is_some() {
                Color::hex("#b04030")
            } else {
                gold
            };
            list.rect(
                Rect::new(bar.x, bar.y, bar.w * fraction.clamp(0., 1.), bar.h),
                fill,
            );
        }
        centred(
            &mut list,
            &detail,
            Font::display(15., 400, false),
            silver,
            middle + 52.,
        );
        renderer.prepare(&gpu.device, &gpu.queue, fonts, &list, width, height, scale);
        let renderer = &*renderer;
        if let Err(error) = gpu.present_with(&mut |encoder, view| renderer.draw(encoder, view)) {
            eprintln!("fury: download screen: {error}");
        }
        self.drawn = Some(key);
    }
}

impl crate::App {
    /// Android's `Resumed`: the first one makes the window (and starts the
    /// content sync when needed); later ones remake the GPU surfaces the
    /// background destroyed. True while the download screen holds the boot.
    pub(crate) fn android_resumed(&mut self, event_loop: &ActiveEventLoop) -> bool {
        if let Some(window) = self.window.clone() {
            self.resize_surface();
            if let Some(sync) = &mut self.sync {
                sync.attach(&window, self.display.clone());
            }
            window.request_redraw();
            return self.sync.is_some();
        }
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("Fury Remastered"))
                .expect("create window"),
        );
        self.window = Some(window.clone());
        let Some(dir) = PENDING_SYNC.lock().unwrap().take() else {
            return false;
        };
        let mut screen = SyncScreen::start(dir);
        screen.attach(&window, self.display.clone());
        self.sync = Some(screen);
        window.request_redraw();
        true
    }

    /// Android's `Suspended`: the window's surface is about to go. Every GPU
    /// surface is released; frames present nothing until `Resumed`.
    pub(crate) fn android_suspended(&mut self) {
        atv_render::window_surface_lost();
        if let Some(gpu) = &mut self.gpu {
            gpu.release_surface();
        }
        if let Some(ready) = &mut self.ready {
            ready.gpu.release_surface();
        }
        if let Some(sync) = &mut self.sync {
            sync.detach();
        }
    }

    /// The download screen's turn of the event loop. Once the package is
    /// complete the screen closes and the game boots as on desktop.
    pub(crate) fn android_sync_tick(&mut self, event_loop: &ActiveEventLoop) {
        let Some(sync) = &mut self.sync else { return };
        if sync.poll() && !self.suspended {
            eprintln!(
                "fury: content ready ({:.1} MB{})",
                sync.progress.total as f64 / 1e6,
                if sync.progress.offline {
                    ", offline"
                } else {
                    ""
                }
            );
            // The screen's device and surface go before the game's open.
            self.sync = None;
            winit::application::ApplicationHandler::resumed(self, event_loop);
            return;
        }
        if let Some(window) = self.window.clone()
            && !self.suspended
        {
            sync.draw(&window);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + std::time::Duration::from_millis(100),
        ));
    }

    /// Window events while the download screen shows: a tap retries a
    /// failed download; the rest waits for the game.
    pub(crate) fn android_sync_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        let Some(sync) = &mut self.sync else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Touch(touch) if touch.phase == TouchPhase::Ended => sync.retry(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut sync.gpu {
                    gpu.resize(size.width, size.height);
                }
                sync.drawn = None;
            }
            WindowEvent::RedrawRequested => {
                if let Some(window) = self.window.clone() {
                    sync.drawn = None;
                    sync.draw(&window);
                }
            }
            _ => {}
        }
    }
}

/// The Back key acts as Escape (pause menu, menu back), as the keyboard's
/// Escape does on desktop.
pub fn back_as_escape(event: &winit::event::KeyEvent) -> Option<winit::event::KeyEvent> {
    (event.logical_key == Key::Named(NamedKey::BrowserBack)).then(|| {
        let mut event = event.clone();
        event.physical_key = PhysicalKey::Code(KeyCode::Escape);
        event.logical_key = Key::Named(NamedKey::Escape);
        event
    })
}
