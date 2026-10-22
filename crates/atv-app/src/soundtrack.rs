//! The soundtrack player (web/music.js MusicStream): the catalog, shuffle
//! and history, the Jukebox controls, a race's hold, and playback (the
//! original VAG decoded in Rust on desktop; the hosted AAC stream in a media
//! element on the web, falling back to the VAGs). One player serves the page:
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
    song: Option<atv_game::audio::music::VagMusic>,
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
        let seed = (web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(1, |d| d.as_millis() as u64)
            & 0x7fff_ffff)
            .max(1);
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
            "soundtrackActive": self.active,
        });
        #[cfg(not(target_arch = "wasm32"))]
        serde_json::json!({
            "musicTitle": self.title, "musicQueued": queued, "soundtrackActive": self.active,
        })
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

    /// MusicStream.hold: no new song; a fade stops the current one.
    pub fn hold(&mut self, fade: f32) {
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
        self.set_song_gain(1.);
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
        #[cfg(target_arch = "wasm32")]
        if self.web.is_none() && !matches!(action, JukeboxAction::Leave | JukeboxAction::Seek(_)) {
            self.web = self.activation.enable_music();
        }
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
        // The menus also hold it paused (`set_active(false)`).
        let paused = self.player.paused || !self.active;
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
