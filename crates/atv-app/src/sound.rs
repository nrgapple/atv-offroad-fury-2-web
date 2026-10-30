//! Audio output for the ride: cpal pulls the shared mixer (CoreAudio,
//! WASAPI, ALSA natively; Web Audio in browsers), started by the first key
//! or pointer press as browser autoplay rules require. The soundtrack is the
//! hosted AAC stream in a media element on the web, as the JavaScript game
//! plays it, and the same AAC files bundled with the desktop app and decoded
//! in Rust there (soundtrack.rs).
pub use crate::soundtrack::Soundtrack;
use atv_content::Content;
use atv_game::{
    audio::{
        bank,
        mixer::Mixer,
        ride::{RideAudio, SoundMode},
    },
    ride::sound::AudioStep,
    venue::Venue,
};
pub use atv_ui::menu::jukebox::{JukeboxAction, MusicView};
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

    pub(crate) fn output(&self) -> Result<Rc<Output>, String> {
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
    pub(crate) fn web(&self) -> Option<Rc<RefCell<WebMusic>>> {
        let mut state = self.0.borrow_mut();
        if state.music && state.web.is_none() {
            state.web = WebMusic::new().map(|web| Rc::new(RefCell::new(web)));
            if let Some(web) = &state.web {
                background::register_music(web);
            }
        }
        state.web.clone()
    }

    /// The song element even when Music was off at start (a Jukebox control
    /// turns it on, from inside the control's gesture).
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn enable_music(&self) -> Option<Rc<RefCell<WebMusic>>> {
        self.0.borrow_mut().music = true;
        let web = self.web()?;
        web.borrow_mut().prime();
        Some(web)
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
pub(crate) struct MusicGraph {
    context: web_sys::AudioContext,
    gain: web_sys::GainNode,
    filter: web_sys::BiquadFilterNode,
    level: web_sys::GainNode,
    pub(crate) analyser: web_sys::AnalyserNode,
    /// Game Settings → Sound Configuration after the muffle level:
    /// Swapped crosses a splitter into a merger; Mono folds through a
    /// one-channel gain (speakers down-mix, (L + R) / 2, duplicated back
    /// to both speakers), like the mixer's `Configuration::apply`.
    splitter: web_sys::ChannelSplitterNode,
    merger: web_sys::ChannelMergerNode,
    mono: web_sys::GainNode,
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
        let splitter = context
            .create_channel_splitter_with_number_of_outputs(2)
            .ok()?;
        let merger = context
            .create_channel_merger_with_number_of_inputs(2)
            .ok()?;
        let mono = context.create_gain().ok()?;
        mono.set_channel_count(1);
        mono.set_channel_count_mode(web_sys::ChannelCountMode::Explicit);
        mono.set_channel_interpretation(web_sys::ChannelInterpretation::Speakers);
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
            splitter,
            merger,
            mono,
            _source: source,
        })
    }

    /// Routes the muffle level's output for the Sound Configuration.
    fn route(&self, configuration: atv_game::audio::mixer::Configuration) {
        use atv_game::audio::mixer::Configuration as C;
        let _ = self.level.disconnect();
        let _ = self.splitter.disconnect();
        let _ = self.merger.disconnect();
        let _ = self.mono.disconnect();
        let destination = self.context.destination();
        let _ = match configuration {
            C::Stereo | C::Dolby => self.level.connect_with_audio_node(&destination),
            C::Swapped => self
                .level
                .connect_with_audio_node(&self.splitter)
                .and_then(|_| {
                    self.splitter
                        .connect_with_audio_node_and_output_and_input(&self.merger, 0, 1)
                })
                .and_then(|_| {
                    self.splitter
                        .connect_with_audio_node_and_output_and_input(&self.merger, 1, 0)
                })
                .and_then(|_| self.merger.connect_with_audio_node(&destination)),
            C::Mono => self
                .level
                .connect_with_audio_node(&self.mono)
                .and_then(|_| self.mono.connect_with_audio_node(&destination)),
        };
    }

    pub(crate) fn resume(&self) {
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
pub(crate) struct WebMusic {
    pub(crate) graph: Option<MusicGraph>,
    /// The element volume the soundtrack wants (gain node once graphed).
    volume: f32,
    pub(crate) element: web_sys::HtmlAudioElement,
    pub(crate) playing: bool,
    pub(crate) pending: Rc<std::cell::Cell<bool>>,
    retry: web_time::Instant,
    primed: Rc<Cell<bool>>,
    silent: Option<String>,
    /// music.js park(): the song unloaded while the page is hidden (url,
    /// position), restored on the next resume.
    parked: Option<(String, f64)>,
    /// Game Settings → Sound Configuration (graphed once not Stereo).
    configuration: atv_game::audio::mixer::Configuration,
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
            configuration: Default::default(),
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
    pub(crate) fn prime(&mut self) {
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

    pub(crate) fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        match &self.graph {
            Some(graph) => graph.gain.gain().set_value(volume),
            None => self.element.set_volume(f64::from(volume)),
        }
    }

    /// Moves the soundtrack into its Web Audio graph (once).
    pub(crate) fn graph(&mut self) -> Option<&MusicGraph> {
        if self.graph.is_none() {
            self.graph = MusicGraph::new(&self.element);
            if let Some(graph) = &self.graph {
                graph.route(self.configuration);
                self.element.set_volume(1.);
                let volume = self.volume;
                self.set_volume(volume);
            }
        }
        self.graph.as_ref()
    }

    /// The configuration the graph routes, if graphed.
    pub(crate) fn route(&self) -> Option<String> {
        self.graph
            .as_ref()
            .map(|_| format!("{:?}", self.configuration))
    }

    /// Game Settings → Sound Configuration on the streamed soundtrack: a
    /// plain element plays Stereo, so another configuration graphs it.
    pub(crate) fn set_configuration(
        &mut self,
        configuration: atv_game::audio::mixer::Configuration,
    ) {
        use atv_game::audio::mixer::Configuration as C;
        if configuration == self.configuration {
            return;
        }
        self.configuration = configuration;
        if self.graph.is_none() && matches!(configuration, C::Stereo | C::Dolby) {
            return;
        }
        if let Some(graph) = self.graph() {
            graph.route(configuration);
        }
    }

    /// setHype on the streamed soundtrack: the graph's lowpass and level
    /// (created at the first muffle; the element then plays at full volume
    /// into it).
    pub(crate) fn set_hype(&mut self, muffle: f32, fast: bool) {
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

    pub(crate) fn resume(&mut self, gesture: bool) {
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

/// audio.js applyAudioSession: iOS/iPadOS Safari (16.4+) mutes Web Audio with
/// the Ring/Silent switch unless the page declares playback audio. Settings →
/// Extras → Silent switch 'play' (default) sets `playback`, like a native game;
/// 'mute' sets `ambient`, which mixes with other apps' audio and follows the
/// switch. Records whether the session exists so the menu shows its row; the
/// type is written only when it changes. Also records fullscreen support.
pub fn sync_audio_session(prefs: &mut atv_ui::menu::prefs::Preferences) {
    #[cfg(target_arch = "wasm32")]
    {
        // frontend.js shows Fullscreen only where `document.fullscreenEnabled`.
        prefs.fullscreen = web_sys::window()
            .and_then(|w| w.document())
            .is_some_and(|d| d.fullscreen_enabled());
        thread_local! {
            static APPLIED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
        }
        let session = web_sys::window()
            .and_then(|w| js_sys::Reflect::get(&w.navigator(), &"audioSession".into()).ok())
            .filter(|s| s.is_object());
        prefs.audio_session = session.is_some();
        let Some(session) = session else {
            return;
        };
        let kind = if prefs.silent == "mute" {
            "ambient"
        } else {
            "playback"
        };
        APPLIED.with_borrow_mut(|applied| {
            if applied.as_deref() != Some(kind)
                && js_sys::Reflect::set(&session, &"type".into(), &kind.into()).unwrap_or(false)
            {
                *applied = Some(kind.into());
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        prefs.audio_session = false;
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

/// The ride's sound: decisions and output, with the page's soundtrack.
pub struct Sound {
    pub output: Rc<Output>,
    pub ride: RideAudio,
    content: Rc<Content>,
    /// The page's soundtrack player (shared with the frontend Jukebox).
    pub music: Rc<RefCell<Soundtrack>>,
    focused: bool,
    #[cfg(target_arch = "wasm32")]
    diagnostics: bool,
    /// CPU spent on sound decisions per rendered frame (ms), for the meter.
    pub cpu: f64,
    /// Other riders' engines and impacts in a multiplayer room.
    pub remote: atv_game::audio::remote::RemoteAudio,
    /// Race CPUs' vocals, body impacts and chassis sounds (their engines
    /// play through `remote`).
    pub cpu_riders: atv_game::audio::cpu::CpuAudio,
    /// Player 1's quad (slot 0 of the race sound table).
    quad: u16,
    /// Mixer render time per rendered frame (ms), averaged over a second.
    pub mix: f64,
    mix_mark: (f64, u32, web_time::Instant),
    /// The last Crazy/Gorn crash boom (0.3 s apart) and how many played.
    boom: (Option<web_time::Instant>, u32),
}

pub struct Settings {
    pub mode: SoundMode,
    pub music: bool,
    /// The ridden vehicle and rider: engine bank, rider vocals.
    pub selection: atv_game::ride::Selection,
    /// A race's lamp, IntOut and crowd samples (`RaceState::samples`).
    pub race_samples: Vec<String>,
    /// ENV08's train car sounds (`Venue::train_sounds`).
    pub train_sounds: Vec<atv_core::spatial_audio::Definition>,
    /// The keyframed actors' loop sounds (`Venue::actor_sounds`).
    pub actor_sounds: Vec<atv_core::spatial_audio::Definition>,
    /// Active Rain weather (the rain loop).
    pub rain: bool,
}

impl Sound {
    pub async fn load(
        content: Rc<Content>,
        environment: &str,
        settings: Settings,
        activation: Activation,
        music: Option<Rc<RefCell<Soundtrack>>>,
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
        // The actors' engine loops load on their own so a package without
        // them keeps the train and rain.
        let actors: Vec<String> = settings
            .actor_sounds
            .iter()
            .map(|d| d.sample.clone())
            .collect();
        if !actors.is_empty()
            && let Err(error) = bank.load_samples(&content, &actors).await
        {
            crate::status(&format!("Actor sound: {error}"));
        }
        let mut ride = RideAudio::new(bank, settings.mode);
        ride.train_sounds = settings.train_sounds.clone();
        ride.actor_sounds = settings.actor_sounds.clone();
        ride.rain = settings.rain;
        ride.apply_mode(&mut *output.mixer.lock().map_err(|_| "mixer lock")?);
        // The page's player (the host started a new session for this ride).
        let music = match music {
            Some(music) => music,
            None => Rc::new(RefCell::new(
                Soundtrack::load(content.clone(), &activation, settings.music).await?,
            )),
        };
        music.borrow_mut().set_active(true);
        crate::status(&format!("Sounds ready: {decoded}"));
        Ok(Self {
            output,
            ride,
            content,
            music,
            focused: true,
            #[cfg(target_arch = "wasm32")]
            diagnostics: web_sys::window()
                .and_then(|w| w.location().search().ok())
                .is_some_and(|q| {
                    q.trim_start_matches('?')
                        .split('&')
                        .any(|p| p == "audioDiagnostics=1")
                }),
            cpu: 0.,
            remote: Default::default(),
            cpu_riders: Default::default(),
            quad: settings.selection.quad,
            mix: 0.,
            mix_mark: (0., 0, web_time::Instant::now()),
            boom: (None, 0),
        })
    }

    pub fn start(&mut self) {
        self.output.start();
        self.music.borrow_mut().start();
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
        self.music.borrow_mut().set_focused(focused);
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
        self.music.borrow_mut().set_volume(music, enabled);
    }

    /// Diagnostics: the configuration the streamed soundtrack's graph
    /// routes, None while it plays straight from its element (Stereo).
    pub fn music_route(&self) -> Option<String> {
        self.music.borrow().music_route()
    }

    /// Game Settings → Sound Configuration for the mixed output and the
    /// streamed soundtrack.
    pub fn set_configuration(&mut self, value: &str) {
        let configuration = atv_game::audio::mixer::Configuration::parse(value);
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.configuration = configuration;
        }
        self.music.borrow_mut().set_configuration(configuration);
    }

    /// Other riders' engines and body impacts this frame; returns the
    /// engine banks that should load. Like game.js's multiplayer listener,
    /// they are heard from the drawn camera (after the chase, crash and
    /// big-air presentation) moving with the player's vehicle; the player's
    /// own and venue sounds keep the follow camera.
    ///
    /// `views` are split screen's players 2-4 cameras: each voice is heard
    /// from the nearest of every player's camera (`001742f0` picks the
    /// nearest listener; research/SPLIT_SCREEN.md "Split audio").
    ///
    /// `cpus` are a race's CPU riders (player 1's race, which split screen
    /// shares): their engines join the other riders' voices with the set
    /// the race sound table `00213210` assigns each slot, and their vocals,
    /// body impacts and chassis sounds play through `cpu_riders`.
    pub fn remote_frame(
        &mut self,
        riders: &[atv_game::audio::remote::Rider],
        cpus: &[atv_game::ride::cpu::CpuSound],
        own: [f32; 3],
        camera: &atv_render::Camera,
        velocity: [f32; 3],
        views: &[atv_core::spatial_audio::Listener],
    ) -> Vec<atv_game::audio::remote::EngineKey> {
        let mut riders = riders.to_vec();
        riders.extend(cpus.iter().map(cpu_rider));
        assign_engine_sets(&mut riders, self.quad, &self.ride.bank.engine_sets);
        let wanted = self.remote.wanted(&riders);
        let listeners: Vec<_> = std::iter::once(atv_core::spatial_audio::Listener {
            position: camera.eye(),
            forward: camera.axes().forward,
            velocity,
        })
        .chain(views.iter().copied())
        .collect();
        let audible = self.output.started.get() && self.focused;
        if let Ok(mut mixer) = self.output.mixer.lock() {
            let bank = &mut self.ride.bank;
            if let Err(error) = self
                .cpu_riders
                .frame(&mut mixer, bank, &listeners, cpus, audible)
            {
                eprintln!("CPU sound: {error}");
            }
            let impact = |name: &str| bank.get(name).cloned();
            if let Err(error) = self.remote.frame(
                &mut mixer,
                bank.effects.as_ref(),
                &impact,
                &listeners,
                Some(own),
                &riders,
                audible,
            ) {
                eprintln!("remote sound: {error}");
            }
        }
        wanted
    }

    /// Loads a remote rider's engine bank at this output's rate.
    pub fn load_remote_engine(
        &self,
        key: atv_game::audio::remote::EngineKey,
    ) -> impl std::future::Future<Output = Result<atv_game::audio::remote::Engine, String>> + 'static
    {
        let content = self.content.clone();
        let rate = self.ride.bank.rate();
        async move {
            let (config, samples) =
                atv_game::audio::bank::load_remote_engine(&content, key, rate).await?;
            Ok(atv_game::audio::remote::Engine { config, samples })
        }
    }

    /// GameAudio.setHype (Alpha Speed & air muffle): the music lowpass and
    /// level, and the wind rush on the SFX bus.
    pub fn set_hype(&mut self, muffle: f32, wind: f32, fast: bool) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.hype.set(muffle, wind, fast);
        }
        self.music.borrow_mut().set_hype(muffle, fast);
    }

    /// stopHype: the muffle toggle went off.
    pub fn stop_hype(&mut self) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.hype.stop();
        }
        self.music.borrow_mut().set_hype(0., true);
    }

    /// The streamed soundtrack: (graphed through Web Audio, element time,
    /// paused); None natively or without a song element.
    pub fn web_state(&self) -> Option<(bool, f64, bool)> {
        self.music.borrow().web_state()
    }

    /// The Jukebox spectrum while a song plays.
    pub fn spectrum(&mut self) -> Option<[u8; atv_game::audio::spectrum::BINS]> {
        self.music.borrow_mut().spectrum()
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

    /// A loaded race/game sample once on the effects bus (Hockey's siren,
    /// puck and crowd), on a running output.
    pub fn one_shot(&mut self, name: &str) {
        if self.output.started.get()
            && let Ok(mut mixer) = self.output.mixer.lock()
        {
            let ride = &mut self.ride;
            ride.race.one_shot(&ride.bank, &mut mixer, name);
        }
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
                self.music.borrow_mut().release();
                return;
            }
            MusicEvent::Intro => {
                self.music.borrow_mut().hold(0.);
                // A random INT001-INT004 (Math.random in the browser).
                let pick = crate::random::below(4);
                vec![(format!("intout/int00{}.vag", pick + 1), false)]
            }
            MusicEvent::Restart => {
                self.music.borrow_mut().hold(0.);
                Vec::new()
            }
            MusicEvent::Over { won } => {
                self.music.borrow_mut().hold(1.5);
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

    /// Keep the soundtrack fed independently of paused simulation steps.
    pub fn maintain(&mut self) {
        let enabled = self.music.borrow().music_enabled;
        if let Ok(mut mixer) = self.output.mixer.lock() {
            let ride = &mut self.ride;
            ride.race.update(&ride.bank, &mut mixer, enabled);
        }
        self.music.borrow_mut().maintain();
    }

    /// Opt-in browser test evidence; normal gameplay serializes no audio state.
    pub fn report(&self, steps: u64) {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = steps;
        #[cfg(target_arch = "wasm32")]
        if self.diagnostics
            && let Ok(mixer) = self.output.mixer.lock()
        {
            let mut state = serde_json::json!({
                "steps": steps, "started": self.output.started.get(),
                "active": mixer.active, "musicActive": mixer.music_active,
                "clock": mixer.seconds(), "voices": mixer.voices(),
                "intOut": mixer.snapshot().iter().filter(|v| v.0 == "intout").count(),
                "engineCommands": self.ride.stats.engine_commands,
                // Race CPUs: engine voices (part of the other riders'),
                // their starts and range stops, and the rider/quad cues.
                "cpu": {
                    "engines": self.remote.cpu_voices(),
                    "engineStarts": self.remote.cpu_starts,
                    "engineCulls": self.remote.cpu_culls,
                    "vocals": self.cpu_riders.stats.vocals,
                    "impacts": self.cpu_riders.stats.impacts,
                    "chassis": self.cpu_riders.stats.chassis,
                    "voices": mixer.snapshot().iter()
                        .filter(|v| v.0.starts_with("cpu ")).count(),
                    "levels": self.remote.levels.iter().filter(|l| l.1)
                        .map(|l| serde_json::json!([l.0, l.2, l.3])).collect::<Vec<_>>(),
                },
                // Keyframed actor loops: defined, their samples in the
                // package, and voices playing now.
                "actorSounds": {
                    "defined": self.ride.actor_sounds.len(),
                    "samples": self.ride.actor_sounds.iter()
                        .filter(|d| self.ride.bank.get(&d.sample).is_some()).count(),
                    "voices": mixer.snapshot().iter().filter(|v| v.0 == "actor").count(),
                },
                // Event sounds started so far (the sound audit).
                "events": {
                    "vocals": self.ride.stats.vocals, "chassis": self.ride.stats.chassis,
                    "drive": self.ride.stats.drive, "impacts": self.ride.stats.impacts,
                    "ambient": self.ride.stats.ambient, "soultree": self.ride.stats.soultree,
                    "train": self.ride.stats.train, "actor": self.ride.stats.actor,
                    "rain": self.ride.stats.rain,
                    "crashBooms": self.boom.1, "history": self.ride.stats.history,
                },
            });
            drop(mixer);
            if let (Some(state), serde_json::Value::Object(music)) =
                (state.as_object_mut(), self.music.borrow().report())
            {
                state.extend(music);
            }
            if let Some(root) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.document_element())
            {
                let _ = root.set_attribute("data-engine-audio", &state.to_string());
            }
        }
    }
}

impl Sound {
    /// The content package the sounds came from.
    pub fn content(&self) -> Rc<Content> {
        self.content.clone()
    }

    /// The Jukebox's live view.
    pub fn music_view(&self) -> MusicView {
        self.music.borrow().music_view()
    }

    /// A Jukebox control.
    pub fn jukebox(&mut self, action: JukeboxAction) {
        self.music.borrow_mut().jukebox(action);
    }
}

/// A race CPU as the other riders' engine voices hear it.
fn cpu_rider(cpu: &atv_game::ride::cpu::CpuSound) -> atv_game::audio::remote::Rider {
    atv_game::audio::remote::Rider {
        peer: atv_game::audio::remote::CPU_PEER + cpu.index.min(31) as u8,
        quad: u16::try_from(cpu.quad).unwrap_or(0),
        position: cpu.origin,
        velocity: cpu.velocity,
        rider_position: cpu.origin,
        rider_velocity: cpu.velocity,
        throttle: cpu.throttle,
        speed: cpu.speed,
        riding: cpu.riding,
        // Their body impacts play through CpuAudio's recovered emitter.
        crash_serial: 0,
        stale: false,
        engine: None,
        cpu: true,
    }
}

/// With CPUs in the race, each rider's engine set as `00213210` assigns
/// the race's slots: player 1 (`quad`), then the other humans in order,
/// then the CPUs. Without CPUs, or without the table (a motorcycle bank),
/// everyone keeps their quad's own set.
fn assign_engine_sets(
    riders: &mut [atv_game::audio::remote::Rider],
    quad: u16,
    sets: &[(u32, u32)],
) {
    use atv_game::audio::remote::EngineKey;
    if sets.is_empty() || !riders.iter().any(|r| r.cpu) {
        return;
    }
    let set = |quad: u16| sets.get(usize::from(quad).wrapping_sub(1)).copied();
    let Some(first) = set(quad) else { return };
    // Humans first, then CPUs (a stable order over the slots).
    let mut order: Vec<usize> = (0..riders.len()).collect();
    order.sort_by_key(|&i| riders[i].cpu);
    let mut slots = vec![first];
    let mut mapped = Vec::new();
    for &i in &order {
        if let Some(s) = set(riders[i].quad) {
            slots.push(s);
            mapped.push(i);
        }
    }
    let own = 1 + mapped.iter().filter(|&&i| !riders[i].cpu).count();
    let names = atv_core::engine_audio::assign_sets(&slots, own);
    for (&i, name) in mapped.iter().zip(&names[1..]) {
        riders[i].engine = EngineKey::from_set_name(name);
    }
}

impl Drop for Sound {
    /// Leaving the ride (GameAudio.raceMusic('leave')): the stingers, crowd
    /// and soundtrack hold end with it.
    fn drop(&mut self) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            self.ride.race.stop(&mut mixer);
            self.remote.stop_all(&mut mixer);
            self.cpu_riders.stop_all(&mut mixer);
        }
        if let Ok(mut music) = self.music.try_borrow_mut() {
            music.ride_ended();
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
