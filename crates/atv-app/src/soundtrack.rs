//! The soundtrack player (web/music.js MusicStream): the catalog, shuffle
//! and history, the Jukebox controls, a race's hold, and playback (the
//! hosted AAC stream in a media element on the web; the same AAC files
//! bundled in the content pack and decoded in Rust on desktop; both fall back
//! to the original VAGs). One player serves the page:
//! a ride's sound and the frontend Jukebox share it, as audio.js shares
//! `this.music` between rides and menus.
#[cfg(target_arch = "wasm32")]
use crate::sound::WebMusic;
use crate::sound::{Activation, Output};
use atv_content::Content;
use atv_game::audio::music::Catalog;
use atv_ui::menu::jukebox::{JukeboxAction, MusicView};
#[cfg(target_arch = "wasm32")]
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Soundtrack {
    pub output: Rc<Output>,
    content: Rc<Content>,
    catalog: Option<Catalog>,
    pub music_enabled: bool,
    focused: bool,
    /// A ride, the Jukebox or an Audio page song choice wants the
    /// soundtrack (audio.js setMusicPreview's `!originalMenu`); the
    /// frontend menus pause it under their own tune (`pauseQueue`).
    active: bool,
    #[cfg(not(target_arch = "wasm32"))]
    song: Option<atv_game::audio::music::NativeSong>,
    /// Desktop: whether the bundled remaster songs can play (a decode error
    /// switches the session to the VAGs, as music.js falls back), and
    /// whether a missing song file was reported.
    #[cfg(not(target_arch = "wasm32"))]
    streams: NativeStreams,
    /// Desktop: a song could not be read (music.js `failed`); the next
    /// selection, Next or Previous tries again.
    #[cfg(not(target_arch = "wasm32"))]
    read_failed: bool,
    #[cfg(target_arch = "wasm32")]
    pub(crate) web: Option<Rc<RefCell<WebMusic>>>,
    #[cfg(target_arch = "wasm32")]
    activation: Activation,
    /// The original-VAG soundtrack when no stream can play (music.js fallback).
    #[cfg(target_arch = "wasm32")]
    vag: VagFallback,
    pub title: Option<String>,
    /// A race holds the soundtrack (intro, countdown, race over), with an
    /// optional fade: (started, seconds).
    pub(crate) held: bool,
    hold_fade: Option<(web_time::Instant, f32)>,
    /// The soundtrack volume (Settings → Audio → Music volume).
    music_volume: f32,
    /// The Jukebox's view of the soundtrack (MusicStream's selection,
    /// history and transient pause).
    pub player: Player,
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

impl Soundtrack {
    /// The catalog and the page's music element; `music` is the saved
    /// Music on/off.
    pub async fn load(
        content: Rc<Content>,
        activation: &Activation,
        music: bool,
    ) -> Result<Self, String> {
        let output = activation.output()?;
        // music.js `sessionSeed`: a random nonzero 32-bit seed per session.
        let seed = (crate::random::bits() & 0xffff_ffff).max(1);
        let catalog = Catalog::load(&content, seed).await.ok();
        Ok(Self {
            output,
            content,
            catalog,
            music_enabled: music,
            focused: true,
            active: false,
            #[cfg(not(target_arch = "wasm32"))]
            song: None,
            #[cfg(not(target_arch = "wasm32"))]
            streams: NativeStreams::default(),
            #[cfg(not(target_arch = "wasm32"))]
            read_failed: false,
            #[cfg(target_arch = "wasm32")]
            web: activation.web(),
            #[cfg(target_arch = "wasm32")]
            activation: activation.clone(),
            #[cfg(target_arch = "wasm32")]
            vag: VagFallback::default(),
            title: None,
            held: false,
            hold_fade: None,
            music_volume: atv_game::audio::mixer::MUSIC_VOLUME,
            player: Player::default(),
            #[cfg(not(target_arch = "wasm32"))]
            analyser: Default::default(),
        })
    }

    /// A gesture: resume the graphed stream's context and start the song.
    pub fn start(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            // A graphed soundtrack resumes its context on every gesture
            // (stricter browsers start one created outside a gesture suspended).
            if let Some(web) = &self.web
                && let Some(graph) = &web.borrow().graph
            {
                graph.resume();
            }
            if self.active {
                self.web_music(true);
            }
        }
    }

    /// The page's focus: the stream pauses while blurred.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &self.web {
            let mut web = web.borrow_mut();
            if focused && web.playing && self.active && !self.player.paused {
                web.resume(true);
            } else if !focused {
                let _ = web.element.pause();
            }
        }
    }

    /// Whether a ride or the Jukebox wants the soundtrack; the menus pause
    /// it in place (music.js `pauseQueue`) and it resumes where it was.
    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.apply_pause();
        } else {
            if let Ok(mut mixer) = self.output.mixer.lock() {
                mixer.stream_paused = true;
            }
            #[cfg(target_arch = "wasm32")]
            if let Some(web) = &self.web {
                let _ = web.borrow().element.pause();
            }
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Settings → Audio: the soundtrack volume and Music on/off.
    pub fn set_volume(&mut self, music: f32, enabled: bool) {
        self.music_volume = music.clamp(0., 1.);
        if enabled != self.music_enabled {
            // music.js `setEnabled`: Off pauses the song where it is
            // (`pauseQueue`); On resumes the same song from there.
            self.music_enabled = enabled;
            self.apply_pause();
        }
        #[cfg(target_arch = "wasm32")]
        if !self.held {
            self.set_song_gain(1.);
        }
    }

    /// Game Settings → Sound Configuration. Native songs play on the
    /// mixer's music bus, which applies it; the web's streamed song
    /// routes its Web Audio graph.
    pub fn set_configuration(&mut self, configuration: atv_game::audio::mixer::Configuration) {
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &self.web {
            web.borrow_mut().set_configuration(configuration);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = configuration;
    }

    /// Diagnostics: the routed Sound Configuration once graphed (web).
    pub fn music_route(&self) -> Option<String> {
        #[cfg(target_arch = "wasm32")]
        return self.web.as_ref().and_then(|w| w.borrow().route());
        #[cfg(not(target_arch = "wasm32"))]
        None
    }

    /// Keep the soundtrack fed (every frame while it is wanted).
    pub fn maintain(&mut self) {
        if let Some((started, seconds)) = self.hold_fade {
            let t = started.elapsed().as_secs_f32() / seconds;
            if t >= 1. {
                self.stop_song();
            } else {
                self.set_song_gain(1. - t);
            }
        }
        if self.held || !self.active {
            return;
        }
        if self.focused && self.output.started.get() && self.music_enabled {
            #[cfg(not(target_arch = "wasm32"))]
            self.native_music();
            #[cfg(target_arch = "wasm32")]
            self.web_music(false);
        }
    }

    /// A ride loads (game.js `loadGame` replaces GameAudio, whose
    /// MusicStream reloads the catalog with a fresh `sessionSeed()`): the
    /// song stops, the shuffle starts over and the first pick avoids the
    /// last song heard (`beginSong` on a fresh stream). The history, volume
    /// and Music setting carry on; a Jukebox pause, a race hold and a stream
    /// fallback do not. An in-place Reset keeps the song (`audio.reset`).
    pub fn new_session(&mut self, seed: u64) {
        self.stop_song();
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.clear_music();
        }
        if let Some(catalog) = &mut self.catalog {
            catalog.reseed(seed);
        }
        self.player.pending = None;
        self.player.selected = None;
        self.player.paused = false;
        self.player.started = false;
        self.held = false;
        self.hold_fade = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.read_failed = false;
        }
        #[cfg(target_arch = "wasm32")]
        {
            // The old fallback decoder (if any) keeps its own slot.
            self.vag = VagFallback::default();
        }
        self.set_song_gain(1.);
        self.apply_pause();
    }

    /// The ride ended (GameAudio.raceMusic('leave')): its hold and the
    /// Jukebox pause end with it.
    pub fn ride_ended(&mut self) {
        self.held = false;
        self.hold_fade = None;
        self.set_song_gain(1.);
        if self.player.paused {
            self.player.paused = false;
        }
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.stream_paused = !self.active;
        }
    }

    /// The streamed soundtrack's Web Audio graph for Alpha muffle (setHype).
    /// Desktop songs play on the mixer's music bus, which the mixer's own
    /// muffle filters (`Sound::set_hype`).
    pub fn set_hype(&mut self, muffle: f32, fast: bool) {
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &self.web {
            web.borrow_mut().set_hype(muffle, fast);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (muffle, fast);
    }

    /// The saved soundtrack history (call once after load).
    pub fn restore_history(&mut self, history: Vec<String>, index: i32) {
        let catalog = self.catalog.as_ref();
        self.player.restore(history, index, catalog);
    }

    /// Diagnostics: the song element's time and pause, the VAG fallback and
    /// the decoded music queued on the mixer.
    pub fn report(&self) -> serde_json::Value {
        let queued = self.output.mixer.lock().map_or(0., |m| m.music_queued());
        #[cfg(target_arch = "wasm32")]
        return serde_json::json!({
            "musicTitle": self.title,
            "musicTime": self.web.as_ref().map(|w| w.borrow().element.current_time()),
            "musicPaused": self.web.as_ref().map(|w| w.borrow().element.paused()),
            "vagFallback": self.vag.fallback, "musicQueued": queued,
            "soundtrackActive": self.active, "musicHeld": self.held,
            "musicEnabled": self.music_enabled,
        });
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (time, duration) = self
                .song
                .as_ref()
                .map(|s| s.decoded_seconds())
                .map_or((None, None), |(d, l)| (Some((d - queued).max(0.)), Some(l)));
            serde_json::json!({
                "musicTitle": self.title, "musicQueued": queued, "soundtrackActive": self.active,
                "musicHeld": self.held, "musicEnabled": self.music_enabled, "readFailed": self.read_failed,
                "musicSource": self.song.as_ref().map(|s| if s.is_stream() { "stream" } else { "vag" }),
                "musicTime": time, "musicDuration": duration, "streamsFailed": self.streams.failed,
            })
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

    /// MusicStream.hold: no new song; a fade stops the current one. A song
    /// with less than the fade left fades over 80% of what remains.
    pub fn hold(&mut self, fade: f32) {
        if self.held {
            return;
        }
        self.held = true;
        let seconds = hold_seconds(fade, self.remaining());
        if seconds > 0. && self.sounding() {
            self.hold_fade = Some((web_time::Instant::now(), seconds));
        } else {
            self.stop_song();
        }
    }

    /// Seconds left in the playing song: the media element's duration less
    /// its time, or (decoded VAGs) the undecoded rest plus what is queued.
    fn remaining(&self) -> Option<f32> {
        let queued = self.output.mixer.lock().map_or(0., |m| m.music_queued());
        let left = |(decoded, duration): (f32, f32)| (duration - decoded).max(0.) + queued;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.song.as_ref().map(|s| left(s.decoded_seconds()))
        }
        #[cfg(target_arch = "wasm32")]
        {
            if self.vag.fallback {
                return self
                    .vag
                    .song
                    .borrow()
                    .as_ref()
                    .map(|s| left(s.decoded_seconds()));
            }
            let web = self.web.as_ref()?.borrow();
            let duration = web.element.duration();
            (web.playing && duration.is_finite())
                .then(|| (duration - web.element.current_time()).max(0.) as f32)
        }
    }

    /// MusicStream.release: the next song starts (GO).
    pub fn release(&mut self) {
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
        // The decoded-VAG song ends too (music.js `request` clears the
        // queue), so a Jukebox choice or a race hold starts afresh.
        #[cfg(target_arch = "wasm32")]
        if self.vag.fallback {
            self.vag
                .generation
                .set(self.vag.generation.get().wrapping_add(1));
            *self.vag.song.borrow_mut() = None;
            if let Ok(mut mixer) = self.output.mixer.lock() {
                mixer.clear_music();
            }
        }
        self.set_song_gain(1.);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn native_music(&mut self) {
        if self.held || self.player.paused || self.read_failed {
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
                self.song = Some(open_native(&content, song, rate, &mut self.streams).await?);
            }
            let Some(song) = &mut self.song else {
                return Ok(());
            };
            match song.pump(&content, &mixer, 2.).await {
                Err(error) if song.is_stream() => {
                    // music.js: a stream that cannot play falls back to the
                    // VAGs for the session; this song continues where it was.
                    eprintln!("music: {error}; the original VAGs play from here");
                    self.streams.failed = true;
                    let at = song.decoded_seconds().0;
                    let index = self.player.selected.ok_or("no song selected")?;
                    let original = &catalog.library.songs[index];
                    let mut vag =
                        atv_game::audio::music::VagMusic::open(&content, original, rate).await?;
                    vag.seek(at)?;
                    self.song = Some(atv_game::audio::music::NativeSong::Vag(vag));
                    Ok(())
                }
                result => result.map(|_| ()),
            }
        });
        if let Err(error) = result {
            // music.js: `failed` stops reads (Music stays on) until the
            // player picks a song again.
            eprintln!("music: {error}");
            self.read_failed = true;
            self.song = None;
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

/// Desktop soundtrack sources (see `Soundtrack::streams`).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct NativeStreams {
    failed: bool,
    missing_reported: bool,
}

/// A desktop song: the bundled remaster the web streams, or the original VAG
/// when the pack lacks the song's file (or it will not open).
#[cfg(not(target_arch = "wasm32"))]
async fn open_native(
    content: &Content,
    song: &atv_core::music::Song,
    rate: u32,
    streams: &mut NativeStreams,
) -> Result<atv_game::audio::music::NativeSong, String> {
    use atv_game::audio::{music, stream::StreamMusic};
    if !streams.failed {
        let path = music::path(song);
        let opened = match content.stream(&path).await {
            Ok(Some(bytes)) => StreamMusic::open(bytes, song.title.clone(), rate).map(Some),
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        };
        match opened {
            Ok(Some(stream)) => {
                eprintln!("music: {} – {} (remaster)", song.title, song.artist);
                return Ok(music::NativeSong::Stream(stream));
            }
            Ok(None) => {}
            Err(error) if !streams.missing_reported => {
                streams.missing_reported = true;
                eprintln!("music: {error}; the original VAG plays for songs without their file");
            }
            Err(_) => {}
        }
    }
    eprintln!("music: {} – {} (original VAG)", song.title, song.artist);
    Ok(music::NativeSong::Vag(
        music::VagMusic::open(content, song, rate).await?,
    ))
}

/// music.js `hold`: the race-over fade, or 80% of the song's remainder when
/// less than the fade is left (unknown remainder: the full fade).
fn hold_seconds(fade: f32, remaining: Option<f32>) -> f32 {
    match remaining {
        Some(remaining) if remaining < fade => remaining * 0.8,
        _ => fade,
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
    /// Bumped when the song stops; a decode that began before keeps none
    /// of its song.
    generation: Rc<Cell<u32>>,
}

#[cfg(target_arch = "wasm32")]
impl Soundtrack {
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
        let (slot, busy, failed, generation) = (
            self.vag.song.clone(),
            self.vag.busy.clone(),
            self.vag.failed.clone(),
            self.vag.generation.clone(),
        );
        let started = generation.get();
        busy.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let result: Result<(), String> = async {
                if let Some((path, title)) = next {
                    let song =
                        atv_game::audio::music::VagMusic::open_path(&content, path, title, rate)
                            .await?;
                    if generation.get() != started {
                        return Ok(());
                    }
                    *slot.borrow_mut() = Some(song);
                }
                // The song leaves the slot while it decodes (no borrow across awaits).
                let taken = slot.borrow_mut().take();
                if let Some(mut song) = taken {
                    let pumped = song.pump(&content, &mixer, 2.).await;
                    if generation.get() != started {
                        // Stopped meanwhile: drop what this decode queued.
                        if let Ok(mut mixer) = mixer.lock() {
                            mixer.clear_music();
                        }
                        return Ok(());
                    }
                    *slot.borrow_mut() = Some(song);
                    pumped?;
                }
                Ok(())
            }
            .await;
            if let Err(error) = result
                && generation.get() == started
            {
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

impl Soundtrack {
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
            // The remaster shows as the web's stream; the VAGs as the decoder.
            view.mode = match &self.song {
                Some(song) if !song.is_stream() => "decoder",
                None if self.streams.failed => "decoder",
                _ => "stream",
            };
            if let Some(song) = &self.song {
                let (decoded, duration) = song.decoded_seconds();
                let queued = self.output.mixer.lock().map_or(0., |m| m.music_queued());
                // Quarter seconds: the Jukebox lays out at most at 4 Hz for it.
                view.position = ((decoded - queued).clamp(0., duration) * 4.).floor() / 4.;
                view.duration = duration;
                view.seekable = true;
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
                    view.duration = self.stream_seconds(web.element.duration()) as f32;
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
        #[cfg(target_arch = "wasm32")]
        if self.web.is_none() && !matches!(action, JukeboxAction::Leave | JukeboxAction::Seek(_)) {
            self.web = self.activation.enable_music();
        }
        let Some(catalog) = &self.catalog else {
            return;
        };
        let request = |sound: &mut Self, pick: Pick| {
            // music.js `request` clears `failed`: a new pick reads again.
            #[cfg(not(target_arch = "wasm32"))]
            {
                sound.read_failed = false;
            }
            #[cfg(target_arch = "wasm32")]
            sound.vag.failed.set(false);
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
                self.apply_pause();
                request(self, Pick::Select(i));
            }
            JukeboxAction::Select(_) => {}
            JukeboxAction::Toggle => {
                let playing = self.music_enabled && !self.player.paused;
                if !playing {
                    self.music_enabled = true;
                }
                self.set_paused(playing);
                self.apply_pause();
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
                    // music.js `seek`: the manifest's length (the element's
                    // until it has one), a quarter second short of the end.
                    let d = self.stream_seconds(web.element.duration());
                    if web.playing && seconds.is_finite() {
                        web.element
                            .set_current_time(f64::from(seconds).clamp(0., (d - 0.25).max(0.)));
                    }
                }
                // Desktop: decoding resumes at `seconds` (the remaster's packet,
                // or the VAG's block; music.js seeks its media element, the
                // same quarter second short of the end) and the queued audio
                // is dropped.
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(song) = &mut self.song
                    && seconds.is_finite()
                {
                    let (_, duration) = song.decoded_seconds();
                    match song.seek(seconds.clamp(0., (duration - 0.25).max(0.))) {
                        Ok(()) => {
                            if let Ok(mut mixer) = self.output.mixer.lock() {
                                mixer.clear_music();
                            }
                        }
                        Err(error) => eprintln!("music seek: {error}"),
                    }
                }
            }
            JukeboxAction::Leave => {
                if self.player.paused {
                    self.player.paused = false;
                    self.apply_pause();
                }
            }
        }
    }

    /// music.js `streams[selectedIndex]?.seconds ?? element.duration`: the
    /// selected song's hosted stream length, else the element's (0 while it
    /// has no metadata).
    #[cfg(target_arch = "wasm32")]
    fn stream_seconds(&self, element: f64) -> f64 {
        let manifest = self
            .catalog
            .as_ref()
            .zip(self.player.selected)
            .and_then(|(catalog, index)| catalog.library.songs.get(index))
            .and_then(|song| {
                self.content
                    .manifest
                    .record(&atv_game::audio::music::path(song))
            })
            .and_then(|record| record.stream.as_ref())
            .map(|stream| stream.seconds);
        manifest.unwrap_or(if element.is_finite() { element } else { 0. })
    }

    fn set_paused(&mut self, paused: bool) {
        if self.player.paused != paused {
            self.player.paused = paused;
            self.apply_pause();
        }
    }

    fn apply_pause(&mut self) {
        // The menus also hold it paused (`set_active(false)`), and so does
        // Music off.
        let paused = self.player.paused || !self.active || !self.music_enabled;
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

#[cfg(test)]
mod tests {
    /// Native acceptance of the bundled remaster on a real output device:
    /// `FURY_PACK=<pack> cargo test -p atv-app --release -- --ignored remaster`
    /// (`FURY_PACK_NOSTREAMS=<pack built --no-streams>` also checks the VAG
    /// fallback).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "needs a content pack (FURY_PACK) and an audio output device"]
    fn desktop_plays_the_bundled_remaster() {
        use super::*;
        use atv_content::{Store, pack::Pack};
        let open = |var: &str| {
            let path = std::env::var_os(var)?;
            let pack = Pack::open(std::path::Path::new(&path)).unwrap();
            Some(Rc::new(
                pollster::block_on(Content::open(Store::Pack(pack))).unwrap(),
            ))
        };
        let content = open("FURY_PACK").expect("set FURY_PACK");
        {
            // Timing of a song start (diagnostic).
            let t = std::time::Instant::now();
            let bytes = pollster::block_on(content.stream("/AUDIO/MUSIC/PARADE.VAG"))
                .unwrap()
                .unwrap();
            let read = t.elapsed().as_secs_f64() * 1000.;
            let t = std::time::Instant::now();
            let mut song =
                atv_game::audio::stream::StreamMusic::open(bytes, String::new(), 48000).unwrap();
            let opened = t.elapsed().as_secs_f64() * 1000.;
            let mixer = std::sync::Arc::new(std::sync::Mutex::new(
                atv_game::audio::mixer::Mixer::new(48000),
            ));
            let t = std::time::Instant::now();
            song.pump(&mixer, 2.).unwrap();
            let pumped = t.elapsed().as_secs_f64() * 1000.;
            eprintln!(
                "song start: read+hash {read:.1} ms, open {opened:.1} ms, first pump {pumped:.1} ms"
            );
        }
        let activation = Activation::new(true);
        let mut music =
            pollster::block_on(Soundtrack::load(content.clone(), &activation, true)).unwrap();
        activation.activate().unwrap();
        // The ride's lifecycle (Sound::set_lifecycle) makes the music bus live.
        music.output.mixer.lock().unwrap().music_active = true;
        music.new_session(7);
        music.set_active(true);
        let run = |music: &mut Soundtrack, seconds: f32| {
            let started = std::time::Instant::now();
            let mut worst = 0f64;
            while started.elapsed().as_secs_f32() < seconds {
                let t = std::time::Instant::now();
                music.maintain();
                worst = worst.max(t.elapsed().as_secs_f64() * 1000.);
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
            worst
        };
        let time = |music: &Soundtrack| music.report()["musicTime"].as_f64().unwrap_or(-1.);
        let first = run(&mut music, 1.5);
        let report = music.report();
        eprintln!("first song: {report} (longest maintain {first:.1} ms)");
        assert_eq!(report["musicSource"], "stream");
        assert!((0.8..2.0).contains(&time(&music)), "{report}");
        // The length is the web's (the manifest's media seconds).
        let index = music.player.selected.unwrap();
        let catalog = music.catalog.as_ref().unwrap();
        let path = atv_game::audio::music::path(&catalog.library.songs[index]);
        let seconds = content
            .manifest
            .record(&path)
            .unwrap()
            .stream
            .as_ref()
            .unwrap()
            .seconds;
        let view = music.music_view();
        assert_eq!(view.mode, "stream");
        assert!(
            (f64::from(view.duration) - seconds).abs() < 0.05,
            "{} vs {seconds}",
            view.duration
        );
        // Jukebox seek.
        music.jukebox(JukeboxAction::Seek(60.));
        run(&mut music, 0.5);
        assert!((60.2..61.).contains(&time(&music)), "{}", music.report());
        // Spectrum bars while it plays.
        assert!(music.spectrum().is_some_and(|b| b.iter().any(|&v| v > 0)));
        // The menus park it in place; it resumes where it was.
        music.set_active(false);
        let parked = time(&music);
        run(&mut music, 0.6);
        assert!(
            (time(&music) - parked).abs() < 0.05,
            "{parked} -> {}",
            time(&music)
        );
        music.set_active(true);
        run(&mut music, 0.6);
        assert!(
            (parked + 0.3..parked + 1.).contains(&time(&music)),
            "{parked} -> {}",
            time(&music)
        );
        // Jukebox Next: the next song, also the remaster.
        let title = music.title.clone();
        music.jukebox(JukeboxAction::Next);
        let next = run(&mut music, 1.);
        eprintln!(
            "next song: {} (longest maintain {next:.1} ms)",
            music.report()
        );
        assert_ne!(music.title, title);
        assert_eq!(music.report()["musicSource"], "stream");
        // Race over: the hold fades the song out over 1.5 s and stops it.
        music.hold(1.5);
        run(&mut music, 2.);
        assert_eq!(music.report()["musicTitle"], serde_json::Value::Null);
        music.release();
        run(&mut music, 1.);
        assert_eq!(music.report()["musicSource"], "stream");
        // A pack without the songs plays the original VAGs.
        if let Some(bare) = open("FURY_PACK_NOSTREAMS") {
            let mut music = pollster::block_on(Soundtrack::load(bare, &activation, true)).unwrap();
            music.new_session(7);
            music.set_active(true);
            run(&mut music, 1.);
            assert_eq!(music.report()["musicSource"], "vag");
            assert_eq!(music.music_view().mode, "decoder");
        }
    }

    #[test]
    fn race_hold_fades_over_what_is_left_of_a_song() {
        assert_eq!(super::hold_seconds(1.5, Some(10.)), 1.5);
        assert_eq!(super::hold_seconds(1.5, None), 1.5);
        assert!((super::hold_seconds(1.5, Some(1.)) - 0.8).abs() < 1e-6);
        assert_eq!(super::hold_seconds(1.5, Some(0.)), 0.);
        // No fade asked: none given.
        assert_eq!(super::hold_seconds(0., Some(0.5)), 0.);
    }
}
