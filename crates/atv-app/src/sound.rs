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

    /// A hidden page suspends the output (audio.js setActive(false) ->
    /// context.suspend()); the next lifecycle `start` resumes it.
    pub fn suspend(&self) {
        self.stream.pause().ok();
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

    /// The output once a gesture has started it (frontend music needs it).
    pub fn started_output(&self) -> Option<Rc<Output>> {
        self.0.borrow().output.clone().filter(|o| o.started.get())
    }

    fn output(&self) -> Result<Rc<Output>, String> {
        let mut state = self.0.borrow_mut();
        if state.output.is_none() {
            let output = Rc::new(Output::open()?);
            #[cfg(target_arch = "wasm32")]
            background::register_output(&output);
            state.output = Some(output);
        }
        Ok(state.output.as_ref().unwrap().clone())
    }

    #[cfg(target_arch = "wasm32")]
    fn web(&self) -> Option<Rc<RefCell<WebMusic>>> {
        let mut state = self.0.borrow_mut();
        if state.music && state.web.is_none() {
            state.web = WebMusic::new().map(|web| Rc::new(RefCell::new(web)));
            if let Some(web) = &state.web {
                background::register_music(web);
            }
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

/// The streamed soundtrack's Web Audio graph (MediaOutput.graph in
/// web/music.js): element -> gain (volume) -> lowpass -> muffle level ->
/// destination, with the Jukebox analyser tapping the gain. Built only once
/// Alpha Speed & air muffle or the Jukebox asks for it, so the default
/// soundtrack keeps playing straight from the element.
#[cfg(target_arch = "wasm32")]
struct MusicGraph {
    context: web_sys::AudioContext,
    gain: web_sys::GainNode,
    filter: web_sys::BiquadFilterNode,
    level: web_sys::GainNode,
    analyser: web_sys::AnalyserNode,
    _source: web_sys::MediaElementAudioSourceNode,
}

#[cfg(target_arch = "wasm32")]
impl MusicGraph {
    fn new(element: &web_sys::HtmlAudioElement) -> Option<Self> {
        let context = web_sys::AudioContext::new().ok()?;
        let gain = context.create_gain().ok()?;
        let filter = context.create_biquad_filter().ok()?;
        filter.set_type(web_sys::BiquadFilterType::Lowpass);
        filter.frequency().set_value(22_000.);
        filter.q().set_value(0.7);
        let level = context.create_gain().ok()?;
        let source = context.create_media_element_source(element).ok()?;
        source.connect_with_audio_node(&gain).ok()?;
        gain.connect_with_audio_node(&filter).ok()?;
        filter.connect_with_audio_node(&level).ok()?;
        level.connect_with_audio_node(&context.destination()).ok()?;
        // music.js analyser(): a passive tap on the music gain.
        let analyser = context.create_analyser().ok()?;
        analyser.set_fft_size(64);
        analyser.set_smoothing_time_constant(0.78);
        gain.connect_with_audio_node(&analyser).ok()?;
        Some(Self {
            context,
            gain,
            filter,
            level,
            analyser,
            _source: source,
        })
    }

    fn resume(&self) {
        if self.context.state() == web_sys::AudioContextState::Suspended
            && let Ok(promise) = self.context.resume()
        {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            });
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct WebMusic {
    graph: Option<MusicGraph>,
    /// The element volume the soundtrack wants (gain node once graphed).
    volume: f32,
    element: web_sys::HtmlAudioElement,
    playing: bool,
    pending: Rc<std::cell::Cell<bool>>,
    retry: web_time::Instant,
    primed: Rc<Cell<bool>>,
    silent: Option<String>,
    /// music.js park(): the song unloaded while the page is hidden (url,
    /// position), restored on the next resume.
    parked: Option<(String, f64)>,
}

#[cfg(target_arch = "wasm32")]
impl WebMusic {
    fn new() -> Option<Self> {
        let element = web_sys::HtmlAudioElement::new().ok()?;
        let volume = atv_game::audio::mixer::MUSIC_VOLUME * atv_game::audio::mixer::MASTER;
        element.set_volume(f64::from(volume));
        Some(Self {
            graph: None,
            volume,
            element,
            playing: false,
            pending: Rc::new(Cell::new(false)),
            retry: web_time::Instant::now(),
            primed: Rc::new(Cell::new(false)),
            silent: None,
            parked: None,
        })
    }

    /// music.js park(): a hidden page unloads the song (iOS then drops its
    /// Now Playing entry) and suspends the graph; resume reloads it at the
    /// same position.
    fn park(&mut self) {
        if let Some(graph) = &self.graph
            && let Ok(promise) = graph.context.suspend()
        {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            });
        }
        let src = self.element.src();
        let _ = self.element.pause();
        if self.parked.is_some() || src.is_empty() || self.silent.as_deref() == Some(src.as_str()) {
            return;
        }
        self.parked = Some((src, self.element.current_time()));
        self.element.remove_attribute("src").ok();
        self.element.load();
    }

    /// The original web/music.js primes one element with a tiny silent WAV in
    /// the gesture, then reuses it when the soundtrack catalog becomes ready.
    fn prime(&mut self) {
        if let Some(graph) = &self.graph {
            graph.resume();
        }
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

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        match &self.graph {
            Some(graph) => graph.gain.gain().set_value(volume),
            None => self.element.set_volume(f64::from(volume)),
        }
    }

    /// Moves the soundtrack into its Web Audio graph (once).
    fn graph(&mut self) -> Option<&MusicGraph> {
        if self.graph.is_none() {
            self.graph = MusicGraph::new(&self.element);
            if self.graph.is_some() {
                self.element.set_volume(1.);
                let volume = self.volume;
                self.set_volume(volume);
            }
        }
        self.graph.as_ref()
    }

    /// setHype on the streamed soundtrack: the graph's lowpass and level
    /// (created at the first muffle; the element then plays at full volume
    /// into it).
    fn set_hype(&mut self, muffle: f32, fast: bool) {
        if self.graph.is_none() && muffle <= 0. {
            return;
        }
        let Some(graph) = self.graph() else { return };
        graph.resume();
        let amount = muffle.clamp(0., 1.);
        let cutoff = 22_000. * atv_core::math::Det::powf_d(320f32 / 22_000., amount);
        let tc = if fast { 0.02 } else { 0.06 };
        let at = graph.context.current_time();
        let _ = graph.filter.frequency().set_target_at_time(cutoff, at, tc);
        let _ = graph
            .level
            .gain()
            .set_target_at_time(1. - 0.3 * amount, at, tc);
    }

    fn resume(&mut self, gesture: bool) {
        if let Some(graph) = &self.graph {
            graph.resume();
        }
        if self.pending.get() || (!gesture && self.retry.elapsed().as_millis() < 250) {
            return;
        }
        if let Some((url, time)) = self.parked.take() {
            self.element.set_src(&url);
            // Seek once the reloaded song knows its duration.
            let element = self.element.clone();
            let seek = wasm_bindgen::closure::Closure::once_into_js(move || {
                element.set_current_time(time);
            });
            let options = web_sys::AddEventListenerOptions::new();
            options.set_once(true);
            let _ = self
                .element
                .add_event_listener_with_callback_and_add_event_listener_options(
                    "loadedmetadata",
                    wasm_bindgen::JsCast::unchecked_ref(&seek),
                    &options,
                );
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
    /// The original-VAG soundtrack when no stream can play (music.js fallback).
    #[cfg(target_arch = "wasm32")]
    vag: VagFallback,
    #[cfg(target_arch = "wasm32")]
    diagnostics: bool,
    pub title: Option<String>,
    /// A race holds the soundtrack (intro, countdown, race over), with an
    /// optional fade: (started, seconds).
    held: bool,
    hold_fade: Option<(web_time::Instant, f32)>,
    /// The soundtrack volume (Settings → Audio → Music volume).
    music_volume: f32,
    /// The Jukebox's view of the soundtrack (MusicStream's selection,
    /// history and transient pause).
    pub player: Player,
    /// CPU spent on sound decisions per rendered frame (ms), for the meter.
    pub cpu: f64,
    /// Other riders' engines and impacts in a multiplayer room.
    pub remote: atv_game::audio::remote::RemoteAudio,
    /// Mixer render time per rendered frame (ms), averaged over a second.
    pub mix: f64,
    mix_mark: (f64, u32, web_time::Instant),
    /// The last Crazy/Gorn crash boom (0.3 s apart) and how many played.
    boom: (Option<web_time::Instant>, u32),
    /// The Jukebox spectrum's smoothing (native soundtrack).
    #[cfg(not(target_arch = "wasm32"))]
    analyser: atv_game::audio::spectrum::Analyser,
}

/// What the next song is (MusicStream `request`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pick {
    Next,
    Select(usize),
    History(usize, usize),
}

/// The soundtrack player state the Jukebox drives (web/music.js
/// MusicStream: `select`, `next`, `previous`, `setPaused`, `seek`, history).
#[derive(Clone, Debug, Default)]
pub struct Player {
    /// The playing (or selected) catalog index.
    pub selected: Option<usize>,
    /// Jukebox Pause: a transient pause, unlike the saved Music on/off.
    pub paused: bool,
    /// Played filenames (persisted, at most 64) and the playing position.
    pub history: Vec<String>,
    pub history_index: i32,
    pending: Option<Pick>,
    /// A fresh session has not started a song yet.
    started: bool,
    /// The history changed since the host last saved it.
    pub dirty: bool,
}

pub use atv_ui::menu::jukebox::{JukeboxAction, MusicView};

pub struct Settings {
    pub mode: SoundMode,
    pub music: bool,
    /// The ridden vehicle and rider: engine bank, rider vocals.
    pub selection: atv_game::ride::Selection,
    /// A race's lamp, IntOut and crowd samples (`RaceState::samples`).
    pub race_samples: Vec<String>,
    /// ENV08's train car sounds (`Venue::train_sounds`).
    pub train_sounds: Vec<atv_core::spatial_audio::Definition>,
    /// Active Rain weather (the rain loop).
    pub rain: bool,
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
        let mut bank = bank;
        bank.load_samples(&content, &settings.race_samples).await?;
        let decoded = format!(
            "{} samples, {:.0} ms decode",
            bank.samples.len(),
            bank.decode_ms
        );
        // Train and rain samples: missing ones leave the ride silent there,
        // as the browser reports and rides on.
        let mut world: Vec<String> = settings
            .train_sounds
            .iter()
            .map(|d| d.sample.clone())
            .collect();
        if settings.rain {
            world.push("rain.vag".into());
        }
        if let Err(error) = bank.load_samples(&content, &world).await {
            crate::status(&format!("Train/rain sound: {error}"));
        }
        let mut ride = RideAudio::new(bank, settings.mode);
        ride.train_sounds = settings.train_sounds.clone();
        ride.rain = settings.rain;
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
            vag: VagFallback::default(),
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
            held: false,
            hold_fade: None,
            music_volume: atv_game::audio::mixer::MUSIC_VOLUME,
            player: Player::default(),
            cpu: 0.,
            remote: Default::default(),
            mix: 0.,
            mix_mark: (0., 0, web_time::Instant::now()),
            boom: (None, 0),
            #[cfg(not(target_arch = "wasm32"))]
            analyser: Default::default(),
        })
    }

    pub fn start(&mut self) {
        self.output.start();
        #[cfg(target_arch = "wasm32")]
        {
            // A graphed soundtrack resumes its context on every gesture
            // (stricter browsers start one created outside a gesture suspended).
            if let Some(web) = &self.web
                && let Some(graph) = &web.borrow().graph
            {
                graph.resume();
            }
            self.web_music(true);
        }
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

    /// Settings → Audio: effect volume, soundtrack volume and on/off
    /// (GameAudio.setVolume, MusicStream.setVolume/setEnabled).
    pub fn set_volumes(&mut self, sfx: f32, music: f32, enabled: bool) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.sfx_volume = sfx.clamp(0., 1.);
            mixer.music_volume = music.clamp(0., 1.);
        }
        self.music_volume = music.clamp(0., 1.);
        if enabled != self.music_enabled {
            self.music_enabled = enabled;
            if !enabled {
                self.stop_song();
            }
        }
        #[cfg(target_arch = "wasm32")]
        if !self.held {
            self.set_song_gain(1.);
        }
    }

    /// Other riders' engines and body impacts this frame; returns the
    /// remote quads whose engine banks should load.
    pub fn remote_frame(&mut self, riders: &[atv_game::audio::remote::Rider]) -> Vec<u16> {
        let wanted = self.remote.wanted(riders);
        let Some(listener) = self.ride.listener() else {
            return wanted;
        };
        let audible = self.output.started.get() && self.focused;
        if let Ok(mut mixer) = self.output.mixer.lock() {
            let bank = &self.ride.bank;
            let impact = |name: &str| bank.get(name).cloned();
            if let Err(error) = self.remote.frame(
                &mut mixer,
                bank.effects.as_ref(),
                &impact,
                &listener,
                riders,
                audible,
            ) {
                eprintln!("remote sound: {error}");
            }
        }
        wanted
    }

    /// Loads a remote quad's engine bank at this output's rate.
    pub fn load_remote_engine(
        &self,
        quad: u16,
    ) -> impl std::future::Future<Output = Result<atv_game::audio::remote::Engine, String>> + 'static
    {
        let content = self.content.clone();
        let rate = self.ride.bank.rate();
        async move {
            let (config, samples) =
                atv_game::audio::bank::load_remote_engine(&content, u32::from(quad), rate).await?;
            Ok(atv_game::audio::remote::Engine { config, samples })
        }
    }

    /// GameAudio.setHype (Alpha Speed & air muffle): the music lowpass and
    /// level, and the wind rush on the SFX bus.
    pub fn set_hype(&mut self, muffle: f32, wind: f32, fast: bool) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.hype.set(muffle, wind, fast);
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &self.web {
            web.borrow_mut().set_hype(muffle, fast);
        }
    }

    /// stopHype: the muffle toggle went off.
    pub fn stop_hype(&mut self) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.hype.stop();
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &self.web {
            web.borrow_mut().set_hype(0., true);
        }
    }

    /// The streamed soundtrack: (graphed through Web Audio, element time,
    /// paused); None natively or without a song element.
    pub fn web_state(&self) -> Option<(bool, f64, bool)> {
        #[cfg(target_arch = "wasm32")]
        {
            let web = self.web.as_ref()?.borrow();
            Some((
                web.graph.is_some(),
                web.element.current_time(),
                web.element.paused(),
            ))
        }
        #[cfg(not(target_arch = "wasm32"))]
        None
    }

    /// The Jukebox spectrum while a song plays: 32 analyser bytes (the
    /// web build graphs its soundtrack for it; natively the mixer's tap).
    pub fn spectrum(&mut self) -> Option<[u8; atv_game::audio::spectrum::BINS]> {
        if !self.output.started.get() || self.player.paused || !self.music_enabled {
            return None;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let web = self.web.as_ref()?;
            let mut web = web.borrow_mut();
            if !web.playing || web.element.paused() {
                return None;
            }
            let graph = web.graph()?;
            graph.resume();
            let mut bytes = [0u8; atv_game::audio::spectrum::BINS];
            graph.analyser.get_byte_frequency_data(&mut bytes);
            Some(bytes)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.song.as_ref()?;
            let samples = self.output.mixer.lock().ok()?.analysis;
            Some(self.analyser.bytes(&samples))
        }
    }

    /// The music muffle in effect (diagnostics).
    pub fn muffle(&self) -> f32 {
        self.output.mixer.lock().map_or(0., |m| m.hype.muffle)
    }

    /// GameAudio.landingThump (Alpha Landing burst) on the SFX bus of a
    /// running output; true when it played.
    pub fn landing_thump(&mut self, severity: f32) -> bool {
        if !self.output.started.get() {
            return false;
        }
        let Some(pcm) = atv_game::audio::synth::landing_thump(severity, self.ride.bank.rate())
        else {
            return false;
        };
        let Ok(mut mixer) = self.output.mixer.lock() else {
            return false;
        };
        mixer.play(
            &std::sync::Arc::new(pcm),
            atv_game::audio::mixer::Looping::Once,
            [1., 1.],
            1.,
            atv_game::audio::mixer::Bus::Sfx,
            false,
            "thump",
        );
        true
    }

    /// GameAudio.crashBoom (Crazy/Gorn crashes): a synthesized boom on the
    /// SFX bus for an impact of 40 ft/s or more, at most every 0.3 s.
    pub fn crash_boom(&mut self, severity: f32) {
        if !self.output.started.get()
            || self.boom.0.is_some_and(|t| t.elapsed().as_secs_f32() < 0.3)
        {
            return;
        }
        let Some(pcm) = atv_game::audio::synth::crash_boom(severity, self.ride.bank.rate()) else {
            return;
        };
        let Ok(mut mixer) = self.output.mixer.lock() else {
            return;
        };
        mixer.play(
            &std::sync::Arc::new(pcm),
            atv_game::audio::mixer::Looping::Once,
            [1., 1.],
            1.,
            atv_game::audio::mixer::Bus::Sfx,
            false,
            "boom",
        );
        self.boom = (Some(web_time::Instant::now()), self.boom.1 + 1);
    }

    /// Crash booms played (diagnostics).
    pub fn crash_booms(&self) -> u32 {
        self.boom.1
    }

    /// One race step's sounds: lamp/gate one-shots and crowd cues on a
    /// running output (the browser drops them on a suspended context), and
    /// the race music events.
    pub fn race(&mut self, events: &atv_game::ride::race::StepEvents) {
        if self.output.started.get()
            && (!events.sounds.is_empty() || events.crowd.is_some())
            && let Ok(mut mixer) = self.output.mixer.lock()
        {
            let ride = &mut self.ride;
            for name in &events.sounds {
                ride.race.one_shot(&ride.bank, &mut mixer, name);
            }
            if let Some(crowd) = &events.crowd {
                ride.race.crowd_cue(&ride.bank, &mut mixer, crowd);
            }
        }
        if let Some(event) = events.music {
            self.race_music(event);
        }
    }

    /// GameAudio.raceMusic: IntOut stingers and the soundtrack hold.
    fn race_music(&mut self, event: atv_game::ride::race::MusicEvent) {
        use atv_game::ride::race::MusicEvent;
        let cues: Vec<(String, bool)> = match event {
            MusicEvent::Go => {
                if let Ok(mut mixer) = self.output.mixer.lock() {
                    self.ride.race.stop_int_out(&mut mixer);
                }
                self.release_music();
                return;
            }
            MusicEvent::Intro => {
                self.hold_music(0.);
                // A random INT001-INT004 (Math.random in the browser).
                let pick = web_time::SystemTime::now()
                    .duration_since(web_time::UNIX_EPOCH)
                    .map_or(0, |d| d.subsec_nanos())
                    % 4;
                vec![(format!("intout/int00{}.vag", pick + 1), false)]
            }
            MusicEvent::Restart => {
                self.hold_music(0.);
                Vec::new()
            }
            MusicEvent::Over { won } => {
                self.hold_music(1.5);
                if won {
                    vec![
                        ("intout/out004.vag".into(), false),
                        ("intout/out001.vag".into(), true),
                    ]
                } else {
                    vec![("intout/out001.vag".into(), true)]
                }
            }
        };
        if let Ok(mut mixer) = self.output.mixer.lock() {
            self.ride.race.set_int_out(&mut mixer, cues);
        }
    }

    /// MusicStream.hold: no new song; a fade stops the current one.
    fn hold_music(&mut self, fade: f32) {
        if self.held {
            return;
        }
        self.held = true;
        if fade > 0. && self.sounding() {
            self.hold_fade = Some((web_time::Instant::now(), fade));
        } else {
            self.stop_song();
        }
    }

    /// MusicStream.release: the next song starts (GO).
    fn release_music(&mut self) {
        if !self.held {
            return;
        }
        self.held = false;
        self.hold_fade = None;
        self.set_song_gain(1.);
    }

    fn sounding(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.output
                .mixer
                .lock()
                .is_ok_and(|mixer| mixer.music_queued() > 0.)
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.web
                .as_ref()
                .is_some_and(|w| w.borrow().playing && !w.borrow().element.paused())
        }
    }

    fn set_song_gain(&mut self, gain: f32) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.stream_gain = gain;
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &self.web {
            web.borrow_mut()
                .set_volume(gain * self.music_volume * atv_game::audio::mixer::MASTER);
        }
    }

    /// The held song ends; release picks the next one.
    fn stop_song(&mut self) {
        self.hold_fade = None;
        self.title = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.song = None;
            if let Ok(mut mixer) = self.output.mixer.lock() {
                mixer.clear_music();
            }
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &self.web {
            let mut web = web.borrow_mut();
            let _ = web.element.pause();
            web.playing = false;
        }
        self.set_song_gain(1.);
    }

    /// Keep the soundtrack fed independently of paused simulation steps.
    pub fn maintain(&mut self) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            let ride = &mut self.ride;
            ride.race.update(&ride.bank, &mut mixer, self.music_enabled);
        }
        if let Some((started, seconds)) = self.hold_fade {
            let t = started.elapsed().as_secs_f32() / seconds;
            if t >= 1. {
                self.stop_song();
            } else {
                self.set_song_gain(1. - t);
            }
        }
        if self.held {
            return;
        }
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
                "vagFallback": self.vag.fallback, "musicQueued": mixer.music_queued(),
                // Event sounds started so far (the sound audit).
                "events": {
                    "vocals": self.ride.stats.vocals, "chassis": self.ride.stats.chassis,
                    "drive": self.ride.stats.drive, "impacts": self.ride.stats.impacts,
                    "ambient": self.ride.stats.ambient, "soultree": self.ride.stats.soultree,
                    "train": self.ride.stats.train, "rain": self.ride.stats.rain,
                    "crashBooms": self.boom.1, "history": self.ride.stats.history,
                },
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
        if self.held || self.player.paused {
            return;
        }
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
                let Some(index) = self.player.pick(catalog)? else {
                    return Ok(());
                };
                let song = &catalog.library.songs[index];
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
        // A race holds the soundtrack until GO; the Jukebox may pause it.
        if self.held
            || self.player.paused
            || !self.focused
            || !self.music_enabled
            || !self.output.started.get()
        {
            return;
        }
        if self.vag.fallback {
            self.vag_fallback(None);
            return;
        }
        // A stream that cannot play (decode or network error) also falls
        // back, as music.js does for errors other than autoplay denial.
        if let Some(web) = &self.web
            && web.borrow().playing
            && web.borrow().element.error().is_some()
        {
            let web = web.borrow();
            let _ = web.element.pause();
            drop(web);
            self.vag.fallback = true;
            self.vag_fallback(None);
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
        let Ok(Some(index)) = self.player.pick(catalog) else {
            return;
        };
        let song = &catalog.library.songs[index];
        let path = atv_game::audio::music::path(song);
        let song_title = song.title.clone();
        self.title = Some(format!("{} – {}", song.title, song.artist));
        let Some(url) = self
            .content
            .manifest
            .record(&path)
            .and_then(|r| r.stream.as_ref())
            .map(|s| s.url.clone())
        else {
            // MusicStream.fallback: no hosted stream, so decode the original
            // VAG for the rest of the session instead of falling silent.
            drop(web);
            self.vag_fallback(Some((path, song_title)));
            return;
        };
        web.element.set_src(&url);
        web.resume(true);
        web.playing = true;
    }
}

/// The browser's original-VAG soundtrack fallback state.
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct VagFallback {
    fallback: bool,
    song: Rc<RefCell<Option<atv_game::audio::music::VagMusic>>>,
    busy: Rc<Cell<bool>>,
    failed: Rc<Cell<bool>>,
}

#[cfg(target_arch = "wasm32")]
impl Sound {
    /// Decodes the soundtrack's VAGs into the mixer's music stream, a song
    /// at a time about two seconds ahead (the browser's decoder mode). `next`
    /// names the song already picked; otherwise the next pick follows when
    /// the current one has played out.
    fn vag_fallback(&mut self, next: Option<(String, String)>) {
        self.vag.fallback = true;
        if self.vag.busy.get() || self.vag.failed.get() {
            return;
        }
        let queued = self.output.mixer.lock().map_or(0., |m| m.music_queued());
        let finished = self.vag.song.borrow().as_ref().is_none_or(|s| s.finished());
        let next = match next {
            Some(next) => Some(next),
            None if finished && queued <= 0.05 => {
                let Some(catalog) = &mut self.catalog else {
                    return;
                };
                let Ok(Some(index)) = self.player.pick(catalog) else {
                    return;
                };
                let song = &catalog.library.songs[index];
                self.title = Some(format!("{} – {}", song.title, song.artist));
                Some((atv_game::audio::music::path(song), song.title.clone()))
            }
            None => None,
        };
        let (content, mixer, rate) = (
            self.content.clone(),
            self.output.mixer.clone(),
            self.output.rate,
        );
        let (slot, busy, failed) = (
            self.vag.song.clone(),
            self.vag.busy.clone(),
            self.vag.failed.clone(),
        );
        busy.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let result: Result<(), String> = async {
                if let Some((path, title)) = next {
                    let song =
                        atv_game::audio::music::VagMusic::open_path(&content, path, title, rate)
                            .await?;
                    *slot.borrow_mut() = Some(song);
                }
                // The song leaves the slot while it decodes (no borrow across awaits).
                let taken = slot.borrow_mut().take();
                if let Some(mut song) = taken {
                    let pumped = song.pump(&content, &mixer, 2.).await;
                    *slot.borrow_mut() = Some(song);
                    pumped?;
                }
                Ok(())
            }
            .await;
            if let Err(error) = result {
                web_sys::console::warn_1(&format!("music fallback: {error}").into());
                failed.set(true);
            }
            busy.set(false);
        });
    }

    /// Whether the soundtrack plays from the decoded VAGs (diagnostics).
    pub fn vag_fallback_active(&self) -> bool {
        self.vag.fallback
    }
}

impl Player {
    /// MusicStream `beginSong`: the pending choice or the next shuffle pick,
    /// recorded in the history.
    fn pick(&mut self, catalog: &mut Catalog) -> Result<Option<usize>, String> {
        let pick = self.pending.take().unwrap_or(Pick::Next);
        let index = match pick {
            Pick::Select(i) | Pick::History(_, i) => {
                catalog.select(i)?;
                Some(i)
            }
            Pick::Next => {
                let mut next = catalog.next_index()?;
                // A fresh session avoids repeating the persisted last track.
                let last = usize::try_from(self.history_index)
                    .ok()
                    .and_then(|i| self.history.get(i));
                if !self.started
                    && catalog.library.songs.len() > 1
                    && next.is_some_and(|n| Some(&catalog.library.songs[n].filename) == last)
                {
                    next = catalog.next_index()?;
                }
                next
            }
        };
        let Some(index) = index else {
            return Ok(None);
        };
        self.started = true;
        self.selected = Some(index);
        let filename = catalog.library.songs[index].filename.clone();
        if let Pick::History(cursor, _) = pick {
            self.history_index = cursor as i32;
        } else {
            self.history
                .truncate((self.history_index + 1).max(0) as usize);
            self.history.push(filename);
            let excess = self.history.len().saturating_sub(64);
            self.history.drain(..excess);
            self.history_index = self.history.len() as i32 - 1;
        }
        self.dirty = true;
        Ok(Some(index))
    }

    /// `playingHistoryIndex`.
    fn playing_history_index(&self, catalog: &Catalog) -> i32 {
        match self.selected {
            Some(i) => {
                let name = &catalog.library.songs[i].filename;
                let upto = ((self.history_index + 1).max(0) as usize).min(self.history.len());
                self.history[..upto]
                    .iter()
                    .rposition(|f| f == name)
                    .map_or(-1, |p| p as i32)
            }
            None => self.history_index,
        }
    }

    /// The saved history (`atv-soundtrack-v1` v3 `history`, `historyIndex`),
    /// restricted to the catalog's songs.
    pub fn restore(&mut self, history: Vec<String>, index: i32, catalog: Option<&Catalog>) {
        self.history = history;
        if let Some(c) = catalog {
            self.history
                .retain(|f| c.library.songs.iter().any(|s| &s.filename == f));
        }
        let excess = self.history.len().saturating_sub(64);
        self.history.drain(..excess);
        self.history_index = index.clamp(-1, self.history.len() as i32 - 1);
    }
}

impl Sound {
    /// The content package the sounds came from.
    pub fn content(&self) -> Rc<Content> {
        self.content.clone()
    }

    /// The saved soundtrack history (call once after load).
    pub fn restore_history(&mut self, history: Vec<String>, index: i32) {
        let catalog = self.catalog.as_ref();
        self.player.restore(history, index, catalog);
    }

    /// The Jukebox's live view.
    pub fn music_view(&self) -> MusicView {
        let songs = self.catalog.as_ref().map_or_else(Vec::new, |c| {
            c.library
                .songs
                .iter()
                .map(|s| (s.title.clone(), s.artist.clone(), s.genre.clone()))
                .collect()
        });
        let mut view = MusicView {
            ready: self.catalog.is_some(),
            enabled: self.music_enabled,
            paused: self.player.paused,
            volume: self.music_volume,
            songs,
            playing_index: self.player.selected,
            ..Default::default()
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            view.mode = "decoder";
            if let Some(song) = &self.song {
                let (decoded, duration) = song.decoded_seconds();
                let queued = self.output.mixer.lock().map_or(0., |m| m.music_queued());
                // Quarter seconds: the Jukebox lays out at most at 4 Hz for it.
                view.position = ((decoded - queued).clamp(0., duration) * 4.).floor() / 4.;
                view.duration = duration;
                view.playing = !self.player.paused && !self.held && queued > 0.;
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            view.mode = "stream";
            if let Some(web) = &self.web {
                let web = web.borrow();
                if web.playing && self.player.selected.is_some() {
                    // Quarter seconds: the Jukebox lays out at most at 4 Hz for it.
                    view.position = (web.element.current_time() as f32 * 4.).floor() / 4.;
                    let d = web.element.duration();
                    view.duration = if d.is_finite() { d as f32 } else { 0. };
                    view.seekable = true;
                    view.playing = !web.element.paused();
                }
            }
        }
        view
    }

    /// A Jukebox control (frontend.js `run` for song:/jukeplay/jukeprev/
    /// jukenext, the seek slider, and `stopJukebox`).
    pub fn jukebox(&mut self, action: JukeboxAction) {
        let Some(catalog) = &self.catalog else {
            return;
        };
        let request = |sound: &mut Self, pick: Pick| {
            sound.player.pending = Some(pick);
            if let Pick::Select(i) | Pick::History(_, i) = pick {
                sound.player.selected = Some(i);
            }
            sound.stop_song();
        };
        match action {
            JukeboxAction::Select(i) if i < catalog.library.songs.len() => {
                self.music_enabled = true;
                self.set_paused(false);
                request(self, Pick::Select(i));
            }
            JukeboxAction::Select(_) => {}
            JukeboxAction::Toggle => {
                let playing = self.music_enabled && !self.player.paused;
                if !playing {
                    self.music_enabled = true;
                }
                self.set_paused(playing);
            }
            JukeboxAction::Next => {
                let cursor = (self.player.playing_history_index(catalog) + 1) as usize;
                let pick = match self.player.history.get(cursor) {
                    Some(name) => catalog
                        .library
                        .songs
                        .iter()
                        .position(|s| &s.filename == name)
                        .map_or(Pick::Next, |i| Pick::History(cursor, i)),
                    None => Pick::Next,
                };
                request(self, pick);
            }
            JukeboxAction::Previous => {
                if self.player.history.is_empty() {
                    request(self, Pick::Select(0));
                } else {
                    let cursor = (self.player.playing_history_index(catalog) - 1).max(0) as usize;
                    let name = &self.player.history[cursor];
                    if let Some(i) = catalog
                        .library
                        .songs
                        .iter()
                        .position(|s| &s.filename == name)
                    {
                        request(self, Pick::History(cursor, i));
                    }
                }
            }
            JukeboxAction::Seek(seconds) => {
                #[cfg(target_arch = "wasm32")]
                if let Some(web) = &self.web {
                    let web = web.borrow();
                    let d = web.element.duration();
                    if web.playing && seconds.is_finite() && d.is_finite() {
                        web.element
                            .set_current_time(f64::from(seconds).clamp(0., (d - 0.25).max(0.)));
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                let _ = seconds;
            }
            JukeboxAction::Leave => {
                if self.player.paused {
                    self.player.paused = false;
                    self.apply_pause();
                }
            }
        }
    }

    fn set_paused(&mut self, paused: bool) {
        if self.player.paused != paused {
            self.player.paused = paused;
            self.apply_pause();
        }
    }

    fn apply_pause(&mut self) {
        let paused = self.player.paused;
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.stream_paused = paused;
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &self.web {
            let mut web = web.borrow_mut();
            if paused {
                let _ = web.element.pause();
            } else if web.playing {
                web.resume(true);
            }
        }
    }
}

impl Drop for Sound {
    /// Leaving the ride (GameAudio.raceMusic('leave')): the stingers, crowd
    /// and soundtrack hold end with it.
    fn drop(&mut self) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            self.ride.race.stop(&mut mixer);
            self.remote.stop_all(&mut mixer);
            mixer.stream_gain = 1.;
            mixer.stream_paused = false;
        }
        #[cfg(target_arch = "wasm32")]
        if self.held {
            self.set_song_gain(1.);
        }
    }
}

/// Leaving the page (iOS app switch, tab hide, pagehide): every audio
/// output suspends and the soundtrack element unloads at once, from the DOM
/// event itself (music.js park on visibilitychange/pagehide, audio.js
/// setActive(false)). The game publishes no Media Session entry
/// (releaseMediaSession). Returning resumes through the normal lifecycle.
#[cfg(target_arch = "wasm32")]
mod background {
    use super::{Output, WebMusic};
    use std::{
        cell::{Cell, RefCell},
        rc::{Rc, Weak},
    };
    use wasm_bindgen::{JsCast, closure::Closure};

    thread_local! {
        static OUTPUTS: RefCell<Vec<Weak<Output>>> = const { RefCell::new(Vec::new()) };
        static MUSIC: RefCell<Vec<Weak<RefCell<WebMusic>>>> = const { RefCell::new(Vec::new()) };
        static INSTALLED: Cell<bool> = const { Cell::new(false) };
    }

    pub fn register_output(output: &Rc<Output>) {
        install();
        OUTPUTS.with_borrow_mut(|list| list.push(Rc::downgrade(output)));
    }

    pub fn register_music(music: &Rc<RefCell<WebMusic>>) {
        install();
        MUSIC.with_borrow_mut(|list| list.push(Rc::downgrade(music)));
    }

    fn hidden() -> bool {
        web_sys::window()
            .and_then(|w| w.document())
            .is_some_and(|d| d.visibility_state() != web_sys::VisibilityState::Visible)
    }

    /// Suspends every live output and parks every soundtrack element.
    pub fn park_all() {
        OUTPUTS.with_borrow_mut(|list| {
            list.retain(|w| w.strong_count() > 0);
            for output in list.iter().filter_map(Weak::upgrade) {
                output.suspend();
            }
        });
        MUSIC.with_borrow_mut(|list| {
            list.retain(|w| w.strong_count() > 0);
            for music in list.iter().filter_map(Weak::upgrade) {
                if let Ok(mut music) = music.try_borrow_mut() {
                    music.park();
                }
            }
        });
    }

    /// releaseMediaSession: no Now Playing metadata and no system actions.
    fn release_media_session() {
        use js_sys::Reflect;
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(session) = Reflect::get(&window.navigator(), &"mediaSession".into()) else {
            return;
        };
        if !session.is_object() {
            return;
        }
        if let Ok(set) = Reflect::get(&session, &"setActionHandler".into())
            && let Ok(set) = set.dyn_into::<js_sys::Function>()
        {
            for action in [
                "play",
                "pause",
                "stop",
                "nexttrack",
                "previoustrack",
                "seekto",
                "seekbackward",
                "seekforward",
            ] {
                let _ = set.call2(&session, &action.into(), &wasm_bindgen::JsValue::NULL);
            }
        }
        let _ = Reflect::set(&session, &"metadata".into(), &wasm_bindgen::JsValue::NULL);
        let _ = Reflect::set(&session, &"playbackState".into(), &"none".into());
    }

    fn install() {
        if INSTALLED.get() {
            return;
        }
        INSTALLED.set(true);
        release_media_session();
        let Some(window) = web_sys::window() else {
            return;
        };
        let on_visibility = Closure::<dyn FnMut()>::new(|| {
            if hidden() {
                park_all();
            }
        });
        if let Some(document) = window.document() {
            let _ = document.add_event_listener_with_callback(
                "visibilitychange",
                on_visibility.as_ref().unchecked_ref(),
            );
        }
        on_visibility.forget();
        let on_pagehide = Closure::<dyn FnMut()>::new(park_all);
        let _ = window
            .add_event_listener_with_callback("pagehide", on_pagehide.as_ref().unchecked_ref());
        on_pagehide.forget();
    }
}
