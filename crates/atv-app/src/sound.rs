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
    rc::Rc,
    sync::{Arc, Mutex},
};

/// Browser contexts run at this rate; desktop uses it when the device can.
pub const RATE: u32 = 48000;

pub struct Output {
    pub mixer: Arc<Mutex<Mixer>>,
    stream: cpal::Stream,
    pub rate: u32,
    pub started: bool,
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
        let mixer = Arc::new(Mutex::new(Mixer::new(rate)));
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
            started: false,
        })
    }

    /// Called from a key or pointer press (Web Audio autoplay policy).
    pub fn start(&mut self) {
        if !self.started && self.stream.play().is_ok() {
            self.started = true;
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct WebMusic {
    element: web_sys::HtmlAudioElement,
    playing: bool,
}

/// The ride's sound: decisions, output and soundtrack.
pub struct Sound {
    pub output: Output,
    pub ride: RideAudio,
    content: Rc<Content>,
    catalog: Option<Catalog>,
    music_enabled: bool,
    #[cfg(not(target_arch = "wasm32"))]
    song: Option<atv_game::audio::music::VagMusic>,
    #[cfg(target_arch = "wasm32")]
    web: Option<WebMusic>,
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
}

impl Sound {
    pub async fn load(
        content: Rc<Content>,
        venue: &Venue,
        ride_state: &atv_game::ride::Ride,
        settings: Settings,
    ) -> Result<Self, String> {
        let output = Output::open()?;
        let course = atv_game::launch::course_resource(&venue.environment, false)?;
        let bank = bank::load(
            &content,
            &course.archive,
            &venue.environment,
            1,
            false,
            output.rate,
        )
        .await?;
        let decoded = format!(
            "{} samples, {:.0} ms decode",
            bank.samples.len(),
            bank.decode_ms
        );
        let mut ride = RideAudio::new(bank, settings.mode);
        let origin = ride_state.frame().origin;
        ride.configure_echo(
            venue.scene.crowd_present,
            |x, z| ride_state.ground(x, z),
            origin,
        )?;
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
            #[cfg(not(target_arch = "wasm32"))]
            song: None,
            #[cfg(target_arch = "wasm32")]
            web: None,
            title: None,
            cpu: 0.,
            mix: 0.,
            mix_mark: (0., 0, web_time::Instant::now()),
        })
    }

    pub fn start(&mut self) {
        self.output.start();
        #[cfg(target_arch = "wasm32")]
        self.web_music();
    }

    pub fn set_active(&mut self, active: bool) {
        if let Ok(mut mixer) = self.output.mixer.lock() {
            mixer.active = active;
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(web) = &mut self.web {
            if active && web.playing {
                let _ = web.element.play();
            } else if !active {
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
        if self.output.started && self.music_enabled {
            #[cfg(not(target_arch = "wasm32"))]
            self.native_music();
            #[cfg(target_arch = "wasm32")]
            self.web_music();
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
    fn web_music(&mut self) {
        if !self.music_enabled || !self.output.started {
            return;
        }
        let Some(catalog) = &mut self.catalog else {
            return;
        };
        if self.web.is_none() {
            let Ok(element) = web_sys::HtmlAudioElement::new() else {
                self.music_enabled = false;
                return;
            };
            element.set_volume(f64::from(
                atv_game::audio::mixer::MUSIC_VOLUME * atv_game::audio::mixer::MASTER,
            ));
            self.web = Some(WebMusic {
                element,
                playing: false,
            });
        }
        let web = self.web.as_mut().unwrap();
        if web.playing && !web.element.ended() {
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
        let _ = web.element.play();
        web.playing = true;
    }
}
