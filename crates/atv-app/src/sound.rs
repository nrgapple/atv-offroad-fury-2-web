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
            held: false,
            hold_fade: None,
            music_volume: atv_game::audio::mixer::MUSIC_VOLUME,
            player: Player::default(),
            cpu: 0.,
            remote: Default::default(),
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
            web.borrow().element.set_volume(f64::from(
                gain * self.music_volume * atv_game::audio::mixer::MASTER,
            ));
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
