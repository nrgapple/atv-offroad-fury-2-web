//! Audio output for the ride: cpal pulls the shared mixer (CoreAudio,
//! WASAPI, ALSA natively; Web Audio in browsers), started by the first key
//! or pointer press as browser autoplay rules require. The soundtrack is the
//! original VAG decoded in Rust on desktop and the hosted AAC stream in a
//! media element on the web, as the JavaScript game plays it.
use atv_content::Content;
use atv_game::{
    audio::{
        bank,
        mixer::Mixer,
        music::Catalog,
        ride::{RideAudio, SoundMode},
    },
    ride::sound::AudioStep,
    venue::Venue,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex},
};

/// Browser contexts run at this rate; desktop uses it when the device can.
pub const RATE: u32 = 48000;

pub struct Output {
    pub mixer: Arc<Mutex<Mixer>>,
    stream: cpal::Stream,
    pub rate: u32,
    pub started: Cell<bool>,
}

impl Output {
    /// Opens the default output device; the stream stays paused until
    /// `start` (a browser AudioContext is created suspended).
    pub fn open() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let default = device.default_output_config().map_err(|e| e.to_string())?;
        let rate = if cfg!(target_arch = "wasm32")
            || device.supported_output_configs().is_ok_and(|mut configs| {
                configs.any(|c| {
                    c.channels() == default.channels()
                        && c.min_sample_rate() <= RATE
                        && c.max_sample_rate() >= RATE
                        && c.sample_format() == default.sample_format()
                })
            }) {
            RATE
        } else {
            default.sample_rate()
        };
        let channels = default.channels().max(1) as usize;
        let config = cpal::StreamConfig {
            channels: default.channels(),
            sample_rate: rate,
            buffer_size: if cfg!(target_arch = "wasm32") {
                cpal::BufferSize::Fixed(2048)
            } else {
                cpal::BufferSize::Default
            },
        };
        let mut mixer = Mixer::new(rate);
        // Gesture-time output can run before the bank is ready. Installation
        // applies the ride's current lifecycle before either bus becomes live.
        mixer.active = false;
        mixer.music_active = false;
        let mixer = Arc::new(Mutex::new(mixer));
        let shared = mixer.clone();
        let mut stereo = Vec::new();
        // Mix in stereo, then fan out to the device's channel layout.
        let mut fill = move |out: &mut [f32]| {
            let frames = out.len() / channels;
            stereo.resize(frames * 2, 0.);
            match shared.lock() {
                Ok(mut mixer) => mixer.render(&mut stereo),
                Err(_) => stereo.fill(0.),
            }
            for (frame, pair) in out
                .chunks_exact_mut(channels)
                .zip(stereo.as_chunks::<2>().0)
            {
                for (c, sample) in frame.iter_mut().enumerate() {
                    *sample = pair[c.min(1)].clamp(-1., 1.);
                }
            }
        };
        let error = |e: cpal::Error| eprintln!("audio output: {e}");
        let stream = match default.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config,
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| fill(out),
                error,
                None,
            ),
            cpal::SampleFormat::I16 => {
                let mut scratch = Vec::new();
                device.build_output_stream(
                    config,
                    move |out: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        scratch.resize(out.len(), 0.);
                        fill(&mut scratch);
                        for (o, s) in out.iter_mut().zip(&scratch) {
                            *o = (s * 32767.) as i16;
                        }
                    },
                    error,
                    None,
                )
            }
            other => return Err(format!("unsupported audio sample format {other}")),
        }
        .map_err(|e| e.to_string())?;
        // Native streams may start playing on creation; hold until `start`.
        stream.pause().ok();
        Ok(Self {
            mixer,
            stream,
            rate,
            started: Cell::new(false),
        })
    }

    /// Called from a key or pointer press (Web Audio autoplay policy).
    pub fn start(&self) {
        // A later gesture must also resume a context suspended by the browser.
        if self.stream.play().is_ok() {
            self.started.set(true);
        }
    }
}

/// Output survives the asynchronous bank load so the actual activating event
/// can resume the context and prime the same music element the ride will use.
#[derive(Clone)]
pub struct Activation(Rc<RefCell<ActivationState>>);

struct ActivationState {
    output: Option<Rc<Output>>,
    #[cfg(target_arch = "wasm32")]
    music: bool,
    #[cfg(target_arch = "wasm32")]
    web: Option<Rc<RefCell<WebMusic>>>,
}

impl Activation {
    pub fn new(music: bool) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = music;
        Self(Rc::new(RefCell::new(ActivationState {
            output: None,
            #[cfg(target_arch = "wasm32")]
            music,
            #[cfg(target_arch = "wasm32")]
            web: None,
        })))
    }

    fn output(&self) -> Result<Rc<Output>, String> {
        let mut state = self.0.borrow_mut();
        if state.output.is_none() {
            state.output = Some(Rc::new(Output::open()?));
        }
        Ok(state.output.as_ref().unwrap().clone())
    }

    #[cfg(target_arch = "wasm32")]
    fn web(&self) -> Option<Rc<RefCell<WebMusic>>> {
        let mut state = self.0.borrow_mut();
        if state.music && state.web.is_none() {
            state.web = WebMusic::new().map(|web| Rc::new(RefCell::new(web)));
        }
        state.web.clone()
    }

    /// Call synchronously from a key/pointer/touch event, including while the
    /// venue or bank is still loading. A saved boolean cannot unlock playback.
    pub fn activate(&self) -> Result<(), String> {
        self.output()?.start();
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = self.web() {
            web.borrow_mut().prime();
        }
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
struct WebMusic {
    element: web_sys::HtmlAudioElement,
    playing: bool,
    pending: Rc<std::cell::Cell<bool>>,
    retry: web_time::Instant,
    primed: Rc<Cell<bool>>,
    silent: Option<String>,
}

#[cfg(target_arch = "wasm32")]
impl WebMusic {
    fn new() -> Option<Self> {
        let element = web_sys::HtmlAudioElement::new().ok()?;
        element.set_volume(f64::from(
            atv_game::audio::mixer::MUSIC_VOLUME * atv_game::audio::mixer::MASTER,
        ));
        Some(Self {
            element,
            playing: false,
            pending: Rc::new(Cell::new(false)),
            retry: web_time::Instant::now(),
            primed: Rc::new(Cell::new(false)),
            silent: None,
        })
    }

    /// The original web/music.js primes one element with a tiny silent WAV in
    /// the gesture, then reuses it when the soundtrack catalog becomes ready.
    fn prime(&mut self) {
        if self.primed.get() || self.pending.get() {
            return;
        }
        if self.element.src().is_empty() {
            let Some(url) = silent_url() else { return };
            self.element.set_src(&url);
            self.silent = Some(url);
        }
        let Ok(promise) = self.element.play() else {
            return;
        };
        self.primed.set(true);
        self.pending.set(true);
        let (pending, primed) = (self.pending.clone(), self.primed.clone());
        let (element, silent) = (self.element.clone(), self.silent.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(promise).await;
            pending.set(false);
            if result.is_err() {
                primed.set(false);
            } else if silent.as_deref() == Some(element.src().as_str()) {
                let _ = element.pause();
            }
        });
    }

    fn resume(&mut self, gesture: bool) {
        if self.pending.get() || (!gesture && self.retry.elapsed().as_millis() < 250) {
            return;
        }
        self.retry = web_time::Instant::now();
        if let Ok(promise) = self.element.play() {
            self.pending.set(true);
            let pending = self.pending.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Autoplay denial/blur can interrupt a pending play. Keep the
                // selected song and retry, rather than leaving it stuck silent.
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                pending.set(false);
            });
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for WebMusic {
    fn drop(&mut self) {
        let _ = self.element.pause();
        self.element.remove_attribute("src").ok();
        self.element.load();
        if let Some(url) = &self.silent {
            web_sys::Url::revoke_object_url(url).ok();
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn silent_url() -> Option<String> {
    // 50 ms, mono signed 16-bit PCM at 8 kHz; authored silence, no game data.
    let mut bytes = [0_u8; 844];
    for (offset, value) in [(0, *b"RIFF"), (8, *b"WAVE"), (12, *b"fmt "), (36, *b"data")] {
        bytes[offset..offset + 4].copy_from_slice(&value);
    }
    for (offset, value) in [(4, 836_u32), (16, 16), (24, 8000), (28, 16000), (40, 800)] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (offset, value) in [(20, 1_u16), (22, 1), (32, 2), (34, 16)] {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes.as_slice()));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("audio/wav");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).ok()?;
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}

/// The ride's sound: decisions, output and soundtrack.
pub struct Sound {
    pub output: Rc<Output>,
    pub ride: RideAudio,
    content: Rc<Content>,
    catalog: Option<Catalog>,
    music_enabled: bool,
    focused: bool,
    #[cfg(not(target_arch = "wasm32"))]
    song: Option<atv_game::audio::music::VagMusic>,
    #[cfg(target_arch = "wasm32")]
    web: Option<Rc<RefCell<WebMusic>>>,
    #[cfg(target_arch = "wasm32")]
    diagnostics: bool,
    pub title: Option<String>,
    /// CPU spent on sound decisions per rendered frame (ms), for the meter.
    pub cpu: f64,
    /// Mixer render time per rendered frame (ms), averaged over a second.
    pub mix: f64,
    mix_mark: (f64, u32, web_time::Instant),
}

pub struct Settings {
    pub mode: SoundMode,
    pub music: bool,
    /// The ridden vehicle and rider: engine bank, rider vocals.
    pub selection: atv_game::ride::Selection,
}

impl Sound {
    pub async fn load(
        content: Rc<Content>,
        environment: &str,
        settings: Settings,
        activation: Activation,
    ) -> Result<Self, String> {
        let output = activation.output()?;
        let course = atv_game::launch::course_resource(environment, false)?;
        let bank = bank::load(
            &content,
            &course.archive,
            environment,
            u32::from(settings.selection.quad),
            settings.selection.rider == 1,
            output.rate,
        )
        .await?;
        let decoded = format!(
            "{} samples, {:.0} ms decode",
            bank.samples.len(),
            bank.decode_ms
        );
        let ride = RideAudio::new(bank, settings.mode);
        ride.apply_mode(&mut *output.mixer.lock().map_err(|_| "mixer lock")?);
        let seed = (web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(1, |d| d.as_millis() as u64)
            & 0x7fff_ffff)
            .max(1);
        let catalog = if settings.music {
            Catalog::load(&content, seed).await.ok()
        } else {
            None
        };
        crate::status(&format!("Sounds ready: {decoded}"));
        Ok(Self {
            output,
            ride,
            content,
            catalog,
            music_enabled: settings.music,
            focused: true,
            #[cfg(not(target_arch = "wasm32"))]
            song: None,
            #[cfg(target_arch = "wasm32")]
            web: if settings.music {
                activation.web()
            } else {
                None
            },
            #[cfg(target_arch = "wasm32")]
            diagnostics: web_sys::window()
                .and_then(|w| w.location().search().ok())
                .is_some_and(|q| {
                    q.trim_start_matches('?')
                        .split('&')
                        .any(|p| p == "audioDiagnostics=1")
                }),
            title: None,
            cpu: 0.,
            mix: 0.,
            mix_mark: (0., 0, web_time::Instant::now()),
        })
    }

    pub fn start(&mut self) {
        self.output.start();
        #[cfg(target_arch = "wasm32")]
        self.web_music(true);
    }

    pub fn configure(&mut self, venue: &Venue, ride: &atv_game::ride::Ride) -> Result<(), String> {
        self.ride.configure_echo(
            venue.scene.crowd_present,
            |x, z| ride.ground(x, z),
            ride.frame().origin,
        )?;
        self.ride
            .apply_mode(&mut *self.output.mixer.lock().map_err(|_| "mixer lock")?);
        Ok(())
    }

    pub fn set_lifecycle(&mut self, focused: bool, paused: bool) {
        self.focused = focused;
        // Backgrounding may suspend the browser audio context independently
        // of our mixer. Restore an already activated output when returning.
        if focused && self.output.started.get() {
            self.output.start();
        }
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.active = focused && !paused;
            mixer.music_active = focused;
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &self.web {
            let mut web = web.borrow_mut();
            if focused && web.playing {
                web.resume(true);
            } else if !focused {
                let _ = web.element.pause();
            }
        }
    }

    pub fn step(&mut self, step: Option<&AudioStep>) {
        let (Some(step), Ok(mut mixer)) = (step, self.output.mixer.lock()) else {
            return;
        };
        if let Err(error) = self.ride.step(&mut mixer, step) {
            eprintln!("sound: {error}");
        }
    }

    pub fn frame(&mut self, step: Option<&AudioStep>) {
        let started = web_time::Instant::now();
        if let Ok(mut mixer) = self.output.mixer.lock()
            && let Err(error) = self.ride.frame(&mut mixer, step)
        {
            eprintln!("sound: {error}");
        }
        self.cpu = started.elapsed().as_secs_f64() * 1000.;
        let (seconds, frames, since) = &mut self.mix_mark;
        *frames += 1;
        if since.elapsed().as_secs_f64() >= 1.
            && let Ok(mixer) = self.output.mixer.lock()
        {
            self.mix = (mixer.render_seconds - *seconds) * 1000. / f64::from(*frames);
            *seconds = mixer.render_seconds;
            *frames = 0;
            *since = web_time::Instant::now();
        }
    }

    /// Keep the soundtrack fed independently of paused simulation steps.
    pub fn maintain(&mut self) {
        if self.focused && self.output.started.get() && self.music_enabled {
            #[cfg(not(target_arch = "wasm32"))]
            self.native_music();
            #[cfg(target_arch = "wasm32")]
            self.web_music(false);
        }
    }

    /// Opt-in browser test evidence; normal gameplay serializes no audio state.
    pub fn report(&self, steps: u64) {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = steps;
        #[cfg(target_arch = "wasm32")]
        if self.diagnostics
            && let Ok(mixer) = self.output.mixer.lock()
        {
            let state = serde_json::json!({
                "steps": steps, "started": self.output.started.get(),
                "active": mixer.active, "musicActive": mixer.music_active,
                "clock": mixer.seconds(), "voices": mixer.voices(),
                "engineCommands": self.ride.stats.engine_commands,
                "musicTitle": self.title,
                "musicTime": self.web.as_ref().map(|w| w.borrow().element.current_time()),
                "musicPaused": self.web.as_ref().map(|w| w.borrow().element.paused()),
            });
            if let Some(root) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.document_element())
            {
                let _ = root.set_attribute("data-engine-audio", &state.to_string());
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn native_music(&mut self) {
        let Some(catalog) = &mut self.catalog else {
            return;
        };
        let content = self.content.clone();
        let mixer = self.output.mixer.clone();
        let rate = self.output.rate;
        let result: Result<(), String> = pollster::block_on(async {
            if self.song.as_ref().is_none_or(|s| s.finished()) {
                // Next song once the previous one has played out.
                if self.song.is_some() && mixer.lock().map_err(|_| "lock")?.music_queued() > 0.05 {
                    return Ok(());
                }
                let Some(song) = catalog.next_song()? else {
                    return Ok(());
                };
                self.title = Some(format!("{} – {}", song.title, song.artist));
                self.song =
                    Some(atv_game::audio::music::VagMusic::open(&content, song, rate).await?);
            }
            if let Some(song) = &mut self.song {
                song.pump(&content, &mixer, 2.).await?;
            }
            Ok(())
        });
        if let Err(error) = result {
            eprintln!("music: {error}");
            self.music_enabled = false;
        }
    }

    /// The hosted AAC stream through one media element (MusicStream in
    /// web/music.js): volume 0.45 under the 0.65 master; the next shuffle
    /// pick starts when a song ends.
    #[cfg(target_arch = "wasm32")]
    fn web_music(&mut self, gesture: bool) {
        if !self.focused || !self.music_enabled || !self.output.started.get() {
            return;
        }
        let Some(catalog) = &mut self.catalog else {
            return;
        };
        let Some(web) = &self.web else { return };
        let mut web = web.borrow_mut();
        // Let gesture-time priming settle before replacing its source: changing
        // src during play() would abort the unlock on stricter mobile browsers.
        if !web.playing && web.pending.get() {
            return;
        }
        if web.playing && !web.element.ended() {
            if web.element.paused() {
                web.resume(gesture);
            }
            return;
        }
        let Ok(Some(song)) = catalog.next_song() else {
            return;
        };
        let path = atv_game::audio::music::path(song);
        let Some(url) = self
            .content
            .manifest
            .record(&path)
            .and_then(|r| r.stream.as_ref())
            .map(|s| s.url.clone())
        else {
            self.music_enabled = false;
            return;
        };
        self.title = Some(format!("{} – {}", song.title, song.artist));
        web.element.set_src(&url);
        web.resume(true);
        web.playing = true;
    }
}
