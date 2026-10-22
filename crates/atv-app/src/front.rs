//! The engine's frontend: title, main menu, single player, course select,
//! garage and course loader (atv_ui::front), drawn without a 3D scene. It
//! owns its UI renderer and the original menu artwork; textures are released
//! while riding and uploaded again on return.
use atv_render::preview::{ModelPreview, PreviewTexture};
use atv_ui::{
    DisplayList, Fonts, Rect, Renderer,
    front::{
        self, Img, MODEL_QUAD, MODEL_RIDER,
        assets::{MenuRider, PreviewModel, QuadCatalog, Track},
        controller::{Event, Frontend},
        logo,
        pixels::{Filter, Rgba},
    },
    hud::{Viewport, minimap::MinimapSettings},
    layout,
    menu::{
        self as pause,
        controller::{Context, Event as MenuEvent, PauseMenu},
        prefs::Preferences,
    },
    settings::Settings,
    touch::TouchControls,
};
use std::collections::HashMap;

/// Artwork read after the title opens.
#[derive(Default)]
pub struct Art {
    pub backdrop: Option<Rgba>,
    pub photos: HashMap<String, Rgba>,
    pub tips: Vec<Vec<String>>,
    pub quads: Option<QuadCatalog>,
    pub top_speed: HashMap<u16, f32>,
    /// The loader's action shots for one course.
    pub shots: Option<(String, Vec<Rgba>)>,
    /// Garage preview models: recent vehicles (with a motorcycle's MX rider)
    /// and the PAGE07 menu riders (male, female).
    pub vehicles: Vec<(u16, PreviewModel, Option<MenuRider>)>,
    pub menu_riders: HashMap<u16, MenuRider>,
}

/// Something the frontend needs loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    Vehicle(u16),
    MenuRider(u16),
}

/// One 3D preview: its model, the UI image it renders into, and whether
/// the image needs drawing.
struct Preview {
    key: String,
    model: ModelPreview,
    target: Option<(atv_ui::ImageId, u32, u32)>,
    dirty: bool,
    /// The menu rider's idle playlist (animated) and when it last posed.
    animated: bool,
    posed: f64,
}

pub struct Front {
    fonts: Fonts,
    renderer: Option<Renderer>,
    format: wgpu::TextureFormat,
    pub settings: Settings,
    pub state: Frontend,
    pub view: Viewport,
    list: DisplayList,
    uploaded: DisplayList,
    upload_size: (u32, u32, f32),
    hits: Vec<(Rect, u32)>,
    pub art: Art,
    images: HashMap<String, Img>,
    /// Recently shown course backdrops and heroes, newest last.
    recent: Vec<String>,
    /// Images uploaded for the current viewport (backdrops depend on it).
    image_view: (f32, f32),
    started: web_time::Instant,
    pub prepare_ms: f64,
    /// `prepare_ms` split into artwork, layout and upload.
    split_ms: [f64; 3],
    /// Vehicle (0) and garage rider (1) previews.
    previews: [Option<Preview>; 2],
    /// A pointer turning a preview: (pointer, slot, last x).
    turning: Option<(u64, usize, f32)>,
    /// When the garage opened (the rider's idle clock).
    garage_since: f64,
    /// Settings opened from the main menu.
    pub menu: Option<PauseMenu>,
    menu_hits: Vec<(Rect, u32)>,
    menu_scroll_max: f32,
    menu_reveal: bool,
    prefs: Preferences,
    /// Controller glyph rasters for the bindings pages.
    glyphs: atv_ui::menu::glyphs::GlyphImages,
    minimap: MinimapSettings,
    touch: TouchControls,
    bindings: atv_game::ride::bindings::Bindings,
    pub pad_name: Option<String>,
    /// The loader shows the frame drawn when loading started.
    shot_started: f64,
    /// A tip was picked for the current loader.
    tip_chosen: bool,
}

/// Fixed-seed pick (tips do not need cryptographic randomness).
fn pick(len: usize) -> usize {
    let nanos = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as usize);
    nanos % len.max(1)
}

impl Front {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view: Viewport,
        tracks: Vec<Track>,
        backdrop: Option<Rgba>,
    ) -> Self {
        Self::with_settings(device, format, view, tracks, backdrop, Settings::load())
    }

    /// A frontend over the given saved settings (in-memory for captures).
    pub fn with_settings(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view: Viewport,
        tracks: Vec<Track>,
        backdrop: Option<Rgba>,
        mut settings: Settings,
    ) -> Self {
        let state = Frontend::new(tracks, &settings, cfg!(target_arch = "wasm32"));
        let mut touch = TouchControls::new(&settings, view.coarse);
        touch.set_viewport(view.width, view.height);
        let minimap = MinimapSettings::load(&settings);
        let prefs = Preferences::load(&settings, view.touch_first);
        let bindings = pause::controller::load_bindings(&mut settings);
        let mut front = Self {
            fonts: Fonts::new(),
            renderer: Some(Renderer::new(device, format)),
            format,
            settings,
            state,
            view,
            list: DisplayList::default(),
            uploaded: DisplayList::default(),
            upload_size: (0, 0, 0.),
            hits: Vec::new(),
            art: Art {
                backdrop,
                ..Default::default()
            },
            images: HashMap::new(),
            recent: Vec::new(),
            image_view: (0., 0.),
            started: web_time::Instant::now(),
            prepare_ms: 0.,
            split_ms: [0.; 3],
            previews: [None, None],
            turning: None,
            garage_since: 0.,
            menu: None,
            menu_hits: Vec::new(),
            menu_scroll_max: 0.,
            menu_reveal: false,
            prefs,
            glyphs: Default::default(),
            minimap,
            touch,
            bindings,
            pad_name: None,
            shot_started: 0.,
            tip_chosen: false,
        };
        front.state.refresh(&view);
        front
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.
    }

    pub fn set_view(&mut self, view: Viewport) {
        if self.view != view {
            self.state.cancel_pointer();
        }
        self.view = view;
        self.touch.coarse = view.coarse;
        self.touch.set_viewport(view.width, view.height);
        self.state.refresh(&view);
    }

    /// Frees the GPU textures and renderer while a ride runs.
    pub fn release_gpu(&mut self) {
        self.previews = [None, None];
        self.renderer = None;
        self.images.clear();
        self.recent.clear();
        self.uploaded = DisplayList::default();
    }

    /// Settings → Audio: the soundtrack (and menu tune) on, and its volume.
    pub fn music(&self) -> (bool, f32) {
        (self.prefs.music_enabled, self.prefs.music_volume)
    }

    /// Back from a ride: saved settings may have changed meanwhile.
    pub fn resume(&mut self, page: &str) {
        self.settings = Settings::load();
        self.prefs = Preferences::load(&self.settings, self.view.touch_first);
        self.minimap = MinimapSettings::load(&self.settings);
        self.bindings = pause::controller::load_bindings(&mut self.settings);
        self.state.loading = None;
        self.state.show(page, &self.view);
    }

    pub fn begin_loading(&mut self, environment: &str) {
        self.state.begin_loading(environment, &self.view);
        self.shot_started = self.now();
        self.tip_chosen = false;
        self.choose_tip();
    }

    fn choose_tip(&mut self) {
        if !self.tip_chosen
            && !self.art.tips.is_empty()
            && let Some(loading) = &mut self.state.loading
        {
            loading.tip = self.art.tips[pick(self.art.tips.len())].clone();
            self.tip_chosen = true;
        }
    }

    pub fn set_tips(&mut self, tips: Vec<Vec<String>>) {
        self.art.tips = tips;
        self.choose_tip();
    }

    /// Whether the page animates (Press Start pulse, loader crossfade).
    pub fn animating(&self) -> bool {
        self.menu.is_none()
            && (matches!(self.state.page.as_str(), "title" | "loading")
                || (self.state.page == "garage"
                    && self.previews[1].as_ref().is_some_and(|p| p.animated)))
    }

    /// Models the current page needs that are not loaded yet.
    pub fn wants(&self) -> Vec<Want> {
        let page = self.state.page.as_str();
        let mut wants = Vec::new();
        if self.menu.is_none()
            && matches!(
                page,
                "main" | "single" | "garage" | "multiplayer" | "create-lobby" | "join-lobby"
            )
        {
            let quad = self.state.quad_id;
            if !self.art.vehicles.iter().any(|(q, ..)| *q == quad) {
                wants.push(Want::Vehicle(quad));
            }
            let rider = self.state.rider_id;
            if page == "garage" && quad < 27 && !self.art.menu_riders.contains_key(&rider) {
                wants.push(Want::MenuRider(rider));
            }
        }
        wants
    }

    pub fn add_vehicle(&mut self, quad: u16, model: PreviewModel, rider: Option<MenuRider>) {
        self.art.vehicles.retain(|(q, ..)| *q != quad);
        self.art.vehicles.push((quad, model, rider));
        // Keep the current and the previous vehicle (selectionAssets keeps two).
        while self.art.vehicles.len() > 2 {
            self.art.vehicles.remove(0);
        }
    }

    // ---- input -------------------------------------------------------

    fn with_menu<T>(
        &mut self,
        f: impl FnOnce(&mut PauseMenu, &mut Context, &[(Rect, u32)], f32) -> T,
    ) -> Option<T> {
        let menu = self.menu.as_mut()?;
        let mut ctx = Context {
            settings: &mut self.settings,
            prefs: &mut self.prefs,
            minimap: &mut self.minimap,
            touch: &mut self.touch,
            bindings: Some(&mut self.bindings),
            pad_name: self.pad_name.clone(),
            view: self.view,
            lobby: None,
            music: None,
        };
        Some(f(menu, &mut ctx, &self.menu_hits, self.menu_scroll_max))
    }

    fn menu_event(&mut self, event: Option<MenuEvent>) -> Option<Event> {
        match event? {
            MenuEvent::Resume => {
                self.menu = None;
                self.menu_hits.clear();
                None
            }
            MenuEvent::Leave(_) => Some(Event::DevLog),
            MenuEvent::Fullscreen => Some(Event::Fullscreen),
            _ => None,
        }
    }

    fn front_event(&mut self, event: Option<Event>) -> Option<Event> {
        match event {
            Some(Event::Settings) => {
                self.menu = Some(PauseMenu::settings());
                self.menu_reveal = true;
                None
            }
            other => other,
        }
    }

    pub fn key(&mut self, code: &str) -> Option<Event> {
        if self.menu.is_some() {
            self.menu_reveal = true;
            let event = self.with_menu(|m, ctx, _, _| m.key(code, ctx)).flatten();
            return self.menu_event(event);
        }
        let view = self.view;
        let event = self.state.key(code, &view, &mut self.settings);
        self.front_event(event)
    }

    pub fn key_up(&mut self, code: &str) -> Option<Event> {
        let event = self.with_menu(|m, ctx, _, _| m.key_up(code, ctx)).flatten();
        self.menu_event(event)
    }

    pub fn pad(&mut self, pad: Option<&atv_game::ride::bindings::Pad>) -> Option<Event> {
        let buttons = pad.map_or([0.; 17], |p| p.buttons);
        let axes = pad.map_or([0.; 4], |p| p.axes);
        if self.menu.is_some() {
            self.menu_reveal = true;
            let event = self
                .with_menu(|m, ctx, _, _| m.pad(buttons, axes, pad.is_some(), ctx))
                .flatten();
            return self.menu_event(event);
        }
        let view = self.view;
        let event = self.state.pad(buttons, axes, &view, &mut self.settings);
        self.front_event(event)
    }

    pub fn press(&mut self, pointer: u64, x: f32, y: f32) {
        if self.menu.is_some() {
            self.with_menu(|m, ctx, hits, _| m.press(hits, pointer, x, y, ctx));
            return;
        }
        self.state.press(&self.hits, pointer, x, y);
        // Drag a preview to turn it (ModelView pointer handlers).
        let slot = self
            .hits
            .iter()
            .rev()
            .find(|(r, id)| (*id == MODEL_QUAD || *id == MODEL_RIDER) && r.contains(x, y))
            .map(|(_, id)| usize::from(*id == MODEL_RIDER));
        self.turning = slot.map(|slot| (pointer, slot, x));
    }

    pub fn moved(&mut self, pointer: u64, x: f32, y: f32, mouse: bool) {
        if self.menu.is_some() {
            self.with_menu(|m, ctx, hits, max| {
                m.hover(hits, x, y);
                m.drag(pointer, x, y, max, ctx)
            });
            return;
        }
        if mouse {
            self.state.hover(&self.hits, x, y);
        }
        self.state.drag(pointer, x, y);
        if let Some((p, slot, last)) = self.turning
            && p == pointer
        {
            if let Some(preview) = &mut self.previews[slot] {
                preview.model.yaw -= (x - last) * 0.015;
                preview.dirty = true;
            }
            self.turning = Some((p, slot, x));
        }
    }

    pub fn release(&mut self, pointer: u64, x: f32, y: f32) -> Option<Event> {
        if self.turning.is_some_and(|(p, ..)| p == pointer) {
            self.turning = None;
        }
        if self.menu.is_some() {
            let event = self
                .with_menu(|m, ctx, hits, _| m.release(hits, pointer, x, y, ctx))
                .flatten();
            return self.menu_event(event);
        }
        let view = self.view;
        let event = self
            .state
            .release(&self.hits, pointer, x, y, &view, &mut self.settings);
        self.front_event(event)
    }

    pub fn cancel_pointer(&mut self) {
        self.turning = None;
        self.state.cancel_pointer();
        if let Some(menu) = &mut self.menu {
            menu.cancel_pointer();
        }
    }

    pub fn wheel(&mut self, dy: f32) {
        if self.menu.is_some() {
            self.with_menu(|m, _, _, max| m.wheel(dy, max));
        } else {
            self.state.wheel(dy);
        }
    }

    // ---- artwork -----------------------------------------------------

    /// Uploads `rgba` under `key` once.
    fn image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        make: impl FnOnce(&Art) -> Option<Rgba>,
    ) -> Option<Img> {
        if let Some(img) = self.images.get(key) {
            return Some(*img);
        }
        let renderer = self.renderer.as_mut()?;
        let rgba = make(&self.art)?;
        if rgba.width == 0 || rgba.height == 0 {
            return None;
        }
        let id = renderer.add_image(device, queue, rgba.width, rgba.height, &rgba.data);
        let img = Img {
            id,
            width: rgba.width as f32,
            height: rgba.height as f32,
        };
        self.images.insert(key.to_string(), img);
        Some(img)
    }

    /// Keeps the last few course images of a kind so stepping back and
    /// forth through the list does not redo their filters.
    fn keep_recent(&mut self, key: &str) {
        const KEEP: usize = 6;
        self.recent.retain(|k| k != key);
        self.recent.push(key.to_string());
        let prefix = format!("{}:", key.rsplit_once(':').map_or(key, |(p, _)| p));
        let same: Vec<String> = self
            .recent
            .iter()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        if same.len() > KEEP {
            let stale: Vec<String> = same[..same.len() - KEEP].to_vec();
            self.recent.retain(|k| !stale.contains(k));
            self.drop_images(|k| !stale.iter().any(|s| s == k));
        }
    }

    fn drop_images(&mut self, keep: impl Fn(&str) -> bool) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        self.images.retain(|key, img| {
            let kept = keep(key);
            if !kept {
                renderer.remove_image(img.id);
            }
            kept
        });
    }

    /// CSS `blur(r)` on an image drawn `cover` into a box: (shrink, sigma)
    /// in source texels, shrinking first so large blurs stay cheap.
    fn blur_plan(source: &Rgba, box_w: f32, box_h: f32, blur: f32) -> (u32, f32) {
        let scale = (box_w / source.width as f32).max(box_h / source.height as f32);
        let sigma = blur / scale.max(1e-3);
        let shrink = (sigma / 2.).floor().clamp(1., 8.) as u32;
        (shrink, sigma / shrink as f32)
    }

    fn artwork(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Artwork {
        let view = self.view;
        if self.image_view != (view.width, view.height) {
            // Backdrops are blurred for the viewport; the logo is drawn at size.
            self.drop_images(|k| !k.starts_with("bg:") && !k.starts_with("logo"));
            self.recent.retain(|k| !k.starts_with("bg:"));
            self.image_view = (view.width, view.height);
        }
        let page = self.state.page.clone();
        let (box_w, box_h) = (view.width + 80., view.height + 80.);
        let track_id = match &self.state.loading {
            Some(loading) if page == "loading" => Some(loading.environment.clone()),
            _ => self
                .state
                .tracks
                .get(self.state.track)
                .map(|t| t.id.clone()),
        };
        // Course select, the lobby and the loader show the course photo as
        // backdrop.
        let mut backdrop_track = false;
        let mut backdrop = None;
        if matches!(page.as_str(), "tracks" | "lobby" | "loading")
            && let Some(id) = &track_id
            && self.art.photos.contains_key(id)
        {
            let key = format!("bg:track:{id}");
            backdrop = self.image(device, queue, &key, |art| {
                let photo = art.photos.get(id)?;
                let (shrink, sigma) = Self::blur_plan(photo, box_w, box_h, 16.);
                let mut image = photo.shrink(shrink);
                image.filter(&[
                    Filter::Grayscale(1.),
                    Filter::Contrast(1.1),
                    Filter::Brightness(1.4),
                ]);
                image.blur(sigma);
                // `.ms-backdrop::before`: the scenic tint, multiplied.
                image.multiply([0xb5, 0x16, 0x16]);
                Some(image)
            });
            backdrop_track = backdrop.is_some();
            self.keep_recent(&key);
        }
        if backdrop.is_none() {
            backdrop = self.image(device, queue, "bg:page01", |art| {
                let source = art.backdrop.as_ref()?;
                let (shrink, sigma) = Self::blur_plan(source, box_w, box_h, 9.);
                let mut image = source.shrink(shrink);
                image.filter(&[Filter::Saturate(1.1), Filter::Brightness(0.9)]);
                image.blur(sigma);
                Some(image)
            });
        }
        // The wordmark at its drawn size (title: min(760px, 88%); panel: 460px).
        let logo_css = if page == "title" {
            (0.88 * view.width).min(760.)
        } else if view.height > view.width {
            ((view.width - 2. * (view.width * 0.032).clamp(16., 44.)) * 0.78).min(340.)
        } else {
            460f32.min(view.width * 0.56)
        };
        let px = ((logo_css * view.scale).round() as u32).clamp(64, 2048);
        let logo_key = format!("logo:{px}");
        let mut logo = None;
        if matches!(
            page.as_str(),
            "title" | "main" | "single" | "multiplayer" | "create-lobby" | "join-lobby"
        ) {
            let drawn = self.image(device, queue, &logo_key, |_| Some(logo::raster(px)));
            let shadow_alpha = if page == "title" { 0.6 } else { 0.4 };
            let shadow_key = format!("logo-shadow:{px}:{shadow_alpha}");
            let shadow = self.image(device, queue, &shadow_key, |_| {
                let mut image = logo::raster(px);
                for p in image.data.as_chunks_mut::<4>().0 {
                    p[3] = (f32::from(p[3]) * shadow_alpha).round() as u8;
                    p[0] = 0;
                    p[1] = 0;
                    p[2] = 0;
                }
                Some(image)
            });
            self.drop_images(|k| !k.starts_with("logo") || k == logo_key || k == shadow_key);
            logo = drawn.zip(shadow);
        }
        // Course photos: small thumbnails for the list, the graded hero.
        let mut photos = HashMap::new();
        if matches!(page.as_str(), "tracks" | "lobby") {
            let thumb_px = 72. * view.scale * 1.5;
            // The lobby shows only the room's course (`.ms-track-hero`).
            let ids: Vec<String> = if page == "lobby" {
                track_id.iter().cloned().collect()
            } else {
                self.state
                    .order()
                    .iter()
                    .map(|&i| self.state.tracks[i].id.clone())
                    .collect()
            };
            for id in &ids {
                if let Some(img) = self.image(device, queue, &format!("thumb:{id}"), |art| {
                    let photo = art.photos.get(id)?;
                    let shrink = ((photo.width as f32 / thumb_px).floor() as u32).max(1);
                    Some(photo.shrink(shrink))
                }) {
                    photos.insert(id.clone(), (img, None));
                }
            }
            if let Some(id) = &track_id {
                let key = format!("hero:{id}");
                let hero = self.image(device, queue, &key, |art| {
                    let mut photo = art.photos.get(id)?.clone();
                    photo.filter(&[Filter::Saturate(1.08), Filter::Contrast(1.04)]);
                    Some(photo)
                });
                if let Some(hero) = hero {
                    photos.entry(id.clone()).or_insert((hero, None)).1 = Some(hero);
                }
                self.keep_recent(&key);
            }
        }
        // Garage stats: uiQuad.txt meters, maker logo and top speed.
        let mut stats = None;
        // Motorcycles have no uiQuad.txt stats.
        if page == "garage" && self.state.quad_id < 27 {
            let quad = self.state.quad_id;
            let info = self
                .art
                .quads
                .as_ref()
                .and_then(|q| q.info.get(&quad))
                .cloned();
            if let Some(info) = info {
                let stem = info
                    .logo
                    .trim_end_matches(".tga")
                    .trim_end_matches(".pst")
                    .to_string();
                let logo = self.image(device, queue, &format!("quadlogo:{stem}"), |art| {
                    art.quads.as_ref()?.logos.get(&stem).cloned()
                });
                stats = Some(front::Stats {
                    engine: info.engine,
                    traction: info.traction,
                    suspension: info.suspension,
                    logo,
                    top_speed: self.art.top_speed.get(&quad).copied(),
                });
            }
        }
        // Loader shots: frames 02-07 every 2.5 s with a 0.6 s crossfade.
        let mut shot = (None, None);
        let mut shot_fade = 1.;
        if page == "loading"
            && let Some(env) = self.state.loading.as_ref().map(|l| l.environment.clone())
        {
            let count = match &self.art.shots {
                Some((e, frames)) if *e == env => frames.len(),
                _ => 0,
            };
            let get = |front: &mut Self, i: usize| {
                front.image(device, queue, &format!("shot:{env}:{i}"), |art| {
                    art.shots.as_ref().and_then(|(_, f)| f.get(i).cloned())
                })
            };
            if count > 0 {
                let t = (self.now() - self.shot_started) / 2500.;
                let frame = t.floor() as usize;
                let fade = ((t.fract() * 2500.) / 600.).min(1.) as f32;
                let current = get(self, frame % count);
                let previous = if frame > 0 && fade < 1. {
                    get(self, (frame - 1) % count)
                } else {
                    None
                };
                shot = (current, previous.map(|p| (p, 1.)));
                shot_fade = if previous.is_some() { fade } else { 1. };
            } else if let Some(id) = &track_id {
                // Training has no action shots: the course photo instead.
                shot.0 = self.image(device, queue, &format!("hero:{id}"), |art| {
                    art.photos.get(id).cloned()
                });
            }
        }
        Artwork {
            backdrop,
            backdrop_track,
            logo,
            photos,
            stats,
            shot,
            shot_fade,
        }
    }

    // ---- frame -------------------------------------------------------

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) {
        let started = web_time::Instant::now();
        if self.renderer.is_none() {
            self.renderer = Some(Renderer::new(device, self.format));
            self.images.clear();
            self.glyphs.clear();
            self.recent.clear();
            self.uploaded = DisplayList::default();
        }
        let view = self.view;
        let art = self.artwork(device, queue);
        let art_ms = started.elapsed().as_secs_f64() * 1000.;
        let now = self.now();
        // ms-pulse: brightness 1 → 1.18 → 1 over 1.8 s, ease-in-out.
        let phase = (now / 1800.).fract() as f32;
        let wave = 0.5 - 0.5 * atv_core::math::Det::cos_d(phase * std::f32::consts::TAU);
        let pulse = 1. + 0.18 * wave;
        let photos = &art.photos;
        let photo = |id: &str| photos.get(id).map(|(thumb, _)| *thumb);
        self.update_previews(device, queue, now);
        let mut screen = self.state.screen(
            art.backdrop,
            art.backdrop_track,
            art.logo,
            &photo,
            art.stats.clone(),
            art.shot,
            pulse,
        );
        if let Some(id) = &screen.current {
            screen.hero = photos.get(id).and_then(|(_, hero)| *hero);
        }
        screen.shot_fade = art.shot_fade;
        if self.menu.is_some() {
            screen.page = "backdrop".into();
        }
        let image = |p: &Option<Preview>| {
            p.as_ref().and_then(|p| p.target).map(|(id, w, h)| Img {
                id,
                width: w as f32,
                height: h as f32,
            })
        };
        screen.quad_model = image(&self.previews[0]);
        screen.rider_model = image(&self.previews[1]);
        self.list.clear();
        self.hits.clear();
        layout::render(
            &front::element(&view, &screen),
            &mut self.fonts,
            view.width,
            view.height,
            &mut self.list,
            &mut self.hits,
        );
        // Both lists keep 8px of padding under their last item.
        if self.state.laid_out(&self.hits, 8.) {
            screen.list_scroll = self.state.list_scroll;
            self.list.clear();
            self.hits.clear();
            layout::render(
                &front::element(&view, &screen),
                &mut self.fonts,
                view.width,
                view.height,
                &mut self.list,
                &mut self.hits,
            );
        }
        // Preview targets follow their slots (canvas size x min(DPR, 2)).
        if self.fit_previews(device) {
            screen.quad_model = image(&self.previews[0]);
            screen.rider_model = image(&self.previews[1]);
            self.list.clear();
            self.hits.clear();
            layout::render(
                &front::element(&view, &screen),
                &mut self.fonts,
                view.width,
                view.height,
                &mut self.list,
                &mut self.hits,
            );
        }
        for preview in self.previews.iter_mut().flatten() {
            if let (true, Some((_, w, h))) = (preview.dirty, preview.target) {
                preview.model.prepare(device, queue, w, h);
            }
        }
        if self.menu.is_some() {
            self.prepare_menu(device, queue);
        }
        let layout_ms = started.elapsed().as_secs_f64() * 1000. - art_ms;
        let upload_size = (width, height, view.scale);
        if let Some(renderer) = &mut self.renderer
            && (self.list != self.uploaded || self.upload_size != upload_size)
        {
            renderer.prepare(
                device,
                queue,
                &mut self.fonts,
                &self.list,
                width,
                height,
                view.scale,
            );
            self.uploaded.clone_from(&self.list);
            self.upload_size = upload_size;
        }
        self.prepare_ms = started.elapsed().as_secs_f64() * 1000.;
        self.split_ms = [art_ms, layout_ms, self.prepare_ms - art_ms - layout_ms];
    }

    fn prepare_menu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let view = self.view;
        let start = self.list.items.len();
        let Some(state) = self.with_menu(|m, ctx, _, _| {
            m.refresh(ctx);
            m.state(ctx)
        }) else {
            return;
        };
        if let Some(renderer) = &mut self.renderer {
            self.glyphs
                .prepare(renderer, device, queue, state.pad.as_ref(), view.scale);
        }
        let mut hits = Vec::new();
        layout::render(
            &pause::element(&view, &state, &self.glyphs),
            &mut self.fonts,
            view.width,
            view.height,
            &mut self.list,
            &mut hits,
        );
        let rect = |id| hits.iter().find(|(_, i)| *i == id).map(|(r, _)| *r);
        if let (Some(panel), Some(content)) = (rect(pause::PANEL), rect(pause::CONTENT)) {
            // Full-screen panel: clamp(12px, 3vh, 28px) above, 20px below.
            let padding = (view.height * 0.03).clamp(12., 28.) + 20.;
            self.menu_scroll_max = (content.h - (panel.h - padding)).max(0.);
        }
        if std::mem::take(&mut self.menu_reveal)
            && let Some(menu) = &mut self.menu
        {
            let before = menu.scroll;
            menu.reveal(&hits);
            menu.scroll = menu.scroll.min(self.menu_scroll_max);
            if menu.scroll != before {
                self.list.items.truncate(start);
                self.menu_hits = hits;
                return self.prepare_menu(device, queue);
            }
        }
        self.menu_hits = hits;
    }

    pub fn draw(&mut self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let Some(renderer) = &self.renderer else {
            return;
        };
        for preview in self.previews.iter_mut().flatten() {
            if let (true, Some((id, ..))) = (preview.dirty, preview.target)
                && let Some(view) = renderer.target_view(id)
            {
                preview.model.draw(encoder, &view);
                preview.dirty = false;
            }
        }
        renderer.draw(encoder, target);
    }

    /// The Host/Join fields' boxes from the latest layout (CSS pixels).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn field_rects(&self) -> Vec<Rect> {
        let mut rects: Vec<(u32, Rect)> = self
            .hits
            .iter()
            .filter(|(_, id)| (atv_ui::front::FIELD..atv_ui::front::FIELD + 4).contains(id))
            .map(|(r, id)| (*id, *r))
            .collect();
        rects.sort_by_key(|(id, _)| *id);
        rects.into_iter().map(|(_, r)| r).collect()
    }

    /// Builds the previews the page shows and poses the menu rider.
    fn update_previews(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, now: f64) {
        let page = self.state.page.clone();
        let shown = self.menu.is_none()
            && matches!(
                page.as_str(),
                "main" | "single" | "garage" | "multiplayer" | "create-lobby" | "join-lobby"
            );
        let quad = self.state.quad_id;
        let texture = |textures: &HashMap<String, front::assets::ModelTexture>| {
            let textures = textures.clone();
            move |name: &str| {
                textures
                    .get(name)
                    .or_else(|| textures.get(&name.to_lowercase()))
                    .map(|t| PreviewTexture {
                        width: t.width,
                        height: t.height,
                        rgba: t.rgba.clone(),
                        cutout: t.cutout,
                    })
            }
        };
        // Slot 0: the vehicle in its bind (or MX resting) pose.
        let key = format!("vehicle:{quad}");
        if !shown {
            self.previews = [None, None];
        } else if self.previews[0].as_ref().is_none_or(|p| p.key != key)
            && let Some((_, model, _)) = self.art.vehicles.iter().find(|(q, ..)| *q == quad)
        {
            let lookup = texture(&model.textures);
            match ModelPreview::new(device, queue, &model.mesh, &lookup, model.pose.as_deref()) {
                Ok(preview) => self.set_preview(0, key, preview, false),
                Err(error) => crate::status(&format!("Model preview: {error}")),
            }
        }
        // Slot 1: the garage rider's idle playlist (PAGE07, or the MX rider).
        if page != "garage" || !shown {
            if let Some(old) = self.previews[1].take()
                && let (Some(renderer), Some((id, ..))) = (&mut self.renderer, old.target)
            {
                renderer.remove_image(id);
            }
            self.garage_since = now;
            return;
        }
        let rider = if quad >= 27 {
            self.art
                .vehicles
                .iter()
                .find(|(q, ..)| *q == quad)
                .and_then(|(_, _, r)| r.as_ref())
        } else {
            self.art.menu_riders.get(&self.state.rider_id)
        };
        let key = if quad >= 27 {
            format!("mx:{quad}")
        } else {
            format!("rider:{}", self.state.rider_id)
        };
        let time = ((now - self.garage_since) / 1000.).max(0.) as f32;
        if let Some(rider) = rider {
            if self.previews[1].as_ref().is_none_or(|p| p.key != key) {
                let lookup = texture(&rider.textures);
                let pose = rider.rider.pose(time).ok().map(|(_, p)| p.world_matrices);
                match ModelPreview::new(device, queue, &rider.rider.mesh, &lookup, pose.as_deref())
                {
                    Ok(mut preview) => {
                        preview.yaw = rider.yaw;
                        self.set_preview(1, key, preview, true);
                    }
                    Err(error) => crate::status(&format!("Rider preview: {error}")),
                }
            } else if let Some(preview) = &mut self.previews[1]
                && now - preview.posed >= 33.
                && let Ok((_, pose)) = rider.rider.pose(time)
            {
                // The browser steps the idle clips at ~30 Hz.
                preview.model.set_pose(&pose.world_matrices);
                preview.posed = now;
                preview.dirty = true;
            }
        }
    }

    fn set_preview(&mut self, slot: usize, key: String, model: ModelPreview, animated: bool) {
        // Keep the old image until the new one is sized and drawn.
        let target = self.previews[slot].take().and_then(|p| p.target);
        self.previews[slot] = Some(Preview {
            key,
            model,
            target,
            dirty: true,
            animated,
            posed: 0.,
        });
    }

    /// Re-creates preview targets whose slot size changed; true when the
    /// page must be laid out again with the new images.
    fn fit_previews(&mut self, device: &wgpu::Device) -> bool {
        let Some(renderer) = &mut self.renderer else {
            return false;
        };
        let scale = self.view.scale.min(2.);
        let mut changed = false;
        for (slot, id) in [(0, MODEL_QUAD), (1, MODEL_RIDER)] {
            let Some(preview) = &mut self.previews[slot] else {
                continue;
            };
            let Some(rect) = self.hits.iter().find(|(_, i)| *i == id).map(|(r, _)| *r) else {
                continue;
            };
            let (w, h) = (
                ((rect.w * scale).round() as u32).clamp(1, 2048),
                ((rect.h * scale).round() as u32).clamp(1, 2048),
            );
            if preview.target.is_some_and(|(_, tw, th)| (tw, th) == (w, h)) {
                continue;
            }
            if let Some((old, ..)) = preview.target {
                renderer.remove_image(old);
            }
            preview.target = Some((renderer.add_target(device, w, h), w, h));
            preview.dirty = true;
            changed = true;
        }
        changed
    }

    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({
            "page": self.state.page,
            "selection": self.state.selection,
            "choices": self.state.choices().iter().map(|(a, _)| a).collect::<Vec<_>>(),
            "freerideRiders": self.state.freeride_riders,
            "track": self.state.tracks.get(self.state.track).map(|t| t.id.clone()),
            "filter": self.state.filter,
            "quad": self.state.quad_id,
            "rider": self.state.rider_id,
            "tracks": self.state.tracks.len(),
            "photos": self.art.photos.len(),
            "scroll": self.state.list_scroll,
            "prepareMs": self.prepare_ms,
            "splitMs": self.split_ms,
            "models": self.previews.iter().map(|p| p.as_ref().filter(|p| p.target.is_some()).map(|p| p.key.clone())).collect::<Vec<_>>(),
            "lobby": self.state.lobby.as_ref().map(|l| serde_json::json!({
                "code": l.code, "status": l.status,
            })),
            "notice": self.state.notice,
            "loading": self.state.loading.as_ref().map(|l| serde_json::json!({
                "environment": l.environment, "status": l.status, "progress": l.progress,
            })),
            "menu": self.menu.as_ref().map(|m| serde_json::json!({
                "page": m.page, "choices": m.rows.choices,
            })),
            "hits": self.hits.iter().map(|(r, id)| serde_json::json!({
                "id": id, "x": r.x, "y": r.y, "width": r.w, "height": r.h,
            })).collect::<Vec<_>>(),
        })
    }
}

/// The page's artwork this frame.
struct Artwork {
    backdrop: Option<Img>,
    backdrop_track: bool,
    logo: Option<(Img, Img)>,
    /// id -> (thumbnail, graded hero).
    photos: HashMap<String, (Img, Option<Img>)>,
    stats: Option<front::Stats>,
    shot: (Option<Img>, Option<(Img, f32)>),
    shot_fade: f32,
}
