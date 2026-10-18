import { gameplayBindings } from './bindings.js';
import { activeGamepad, controllerSettings, controllerLabel, cycleController } from './gamepads.js';
import { standardPad, bindingGlyphs, controllerSummary } from './pad-glyphs.js';
import { parseLaunch, menuShortcut, buildRideUrl, copyRideUrl, ENVIRONMENTS, RACE_COURSES, COURSE_PREVIEWS, courseResource, loadingShotsResource } from './launch.js';
import { renderScreen, screenPages, courseGroup, COURSE_GROUPS } from './menu-screens.js';
import { PAINT_PALETTE, paintGlsl, paintable } from './render-support.js';
import { renderJukebox, stopJukebox, JUKEBOX_SEEK_STEP } from './jukebox.js';
// Rust owns menu labels, supported choices and their order. This bridge lays out
// the HD frontend screens (menu-screens.js) and ride panels, and binds input.
const base = name => name.replaceAll('\\', '/').split('/').at(-1).toLowerCase();
const unquote = value => (value ?? '').replace(/^"/, '').replace(/"$/, '');
const fields = section => Object.fromEntries([...section.fields].reverse().map(f => [f.key.toLowerCase(), f.value]));
const sections = doc => new Map(doc.sections.map(s => [s.name.toLowerCase(), fields(s)]));
const overlayPages = new Set(['settings','extras','alpha','pause','players','hud-settings','minimap-config','race-overlays','controls','controller-settings','bindings','music','jukebox']);
// Menu pages with quad/rider preview slots.
const MODEL_PAGES = ['garage','gear','main','single','create-lobby','join-lobby'];
const settingsPages = new Set(['settings','extras','alpha','hud-settings','minimap-config','race-overlays','controls','controller-settings','bindings','music','jukebox']);
// Value rows that step either way with Left/Right as well as cycling on a tap.
const stepperActions = new Set(['racetrack','racelaps','racecpu','racedifficulty','gearhelmet','gearjersey','gearpant','gearglove','gearboot','geargoggle','gearpaint']);
const cycleActions = new Set(['graphics','vibration','speed-rush','crash-camera','chase-camera','flips','train-riding','funnycrashes','diagnostics','onehanded','minimap-orientation','minimap-shape','minimap-corner','minimap-toggle','musictoggle','musicvolume','sfxvolume','soundmode','silentmode','overlay-timing','overlay-lap','overlay-score','overlay-position','overlay-info','overlay-speed','overlay-radar']);
const fontName = 'tradegothic_boldcondensed_22s';
const selectionKey = 'atv-player-selection';
const trainingPreview = 'training-map-preview';
// Original STARTEVT etips.txt entries about riding technique. Tips about
// profiles, championships, editors, mini-games or PS2 buttons are omitted.
const ridingTips = [5,9,10,11,14,16,17,18,19,23,24,25,26,27];
// Race setup offers 1-5 laps, 0, 3 or 4 CPU riders and the original Normal (1)
// or Expert (2) difficulty.
const RACE_CPU = [0,3,4];
const normalizedRace = value => ({
  course: RACE_COURSES.includes(value?.course) ? value.course : 'env01',
  laps: Number.isInteger(value?.laps) && value.laps >= 1 && value.laps <= 5 ? value.laps : 3,
  cpu: RACE_CPU.includes(value?.cpu) ? value.cpu : 3,
  difficulty: value?.difficulty === 2 ? 2 : 1,
});
const normalizedSelection = value => ({
  quadId: Number.isInteger(value?.quadId) && value.quadId >= 1 && value.quadId <= 46 ? value.quadId : 1,
  riderId: value?.riderId === 1 ? 1 : 0,
  freestyleEnvironment: ENVIRONMENTS.includes(value?.freestyleEnvironment) ? value.freestyleEnvironment : 'env01',
  // The last started race's course, laps and CPU riders, like the Freestyle course.
  race: normalizedRace(value?.race),
  // The multiplayer jersey name is remembered like the rest of the garage.
  playerName: typeof value?.playerName === 'string' && value.playerName.trim() && new TextEncoder().encode(value.playerName).length <= 24 && !/\p{Cc}/u.test(value.playerName) ? value.playerName : 'Rider',
  appearance: normalizedAppearance(value?.appearance),
});
// Rider gear (helmet, glove, boot, shirt, sleeve, pant, goggle catalog
// indices; 0 keeps the original) and the quad paint index, as shared online.
export const GEAR_SLOTS = ['helmet','glove','boot','shirt','sleeve','pant','goggle'];
export const PAINT_NAMES = ['Original','Red','Orange','Yellow','Green','Teal','Blue','Purple','Pink','White','Black'];
export function normalizedAppearance(value) {
  const gear = Array.isArray(value?.gear) && value.gear.length === 7 && value.gear.every(g => Number.isInteger(g) && g >= 0 && g <= 64) ? [...value.gear] : [0,0,0,0,0,0,0];
  return { gear, paint: Number.isInteger(value?.paint) && value.paint >= 0 && value.paint < PAINT_NAMES.length ? value.paint : 0 };
}
export const appearanceBytes = a => [...normalizedAppearance(a).gear, normalizedAppearance(a).paint];
export const appearanceFromBytes = bytes => normalizedAppearance(Array.isArray(bytes) && bytes.length === 8 ? { gear: bytes.slice(0, 7), paint: bytes[7] } : null);
export function loadPlayerSelection(read = () => localStorage.getItem(selectionKey)) {
  try { return normalizedSelection(JSON.parse(read())); } catch { return normalizedSelection(null); }
}
export function savePlayerSelection(value, write = text => localStorage.setItem(selectionKey, text)) {
  try { write(JSON.stringify(normalizedSelection(value))); } catch { /* Session-only selection when storage is unavailable. */ }
}
export function unlockedCourses(tracks, files) {
  const source = new Map();
  for (const [section, f] of tracks) {
    if (!/^(unspnat\d+|spnat\d+|amsx\d+|prosx\d+|spstunt\d+|train1|hoc1)$/.test(section)) continue;
    if (ENVIRONMENTS.includes(f.scn) && !source.has(f.scn)) source.set(f.scn,f);
  }
  return ENVIRONMENTS.filter(id=>source.has(id) && files.some(f=>f.path===courseResource(id).archive))
    .map(id=>({id,name:unquote(source.get(id).name),image:id==='training'?'':source.get(id).image}));
}

export class Frontend {
  constructor({ codec, json, content, read, start, active, audio, diagnosticsOverlay, mobileControls, multiplayer, crashSettings, minimapSettings, raceOverlays, modelAssets, menuRider, gearPreview, graphicsSettings, vibrationSettings, alphaSettings }) {
    Object.assign(this, { codec, json, content, read, start, active, audio, diagnosticsOverlay, mobileControls, multiplayer, crashSettings, minimapSettings, raceOverlays, modelAssets, menuRider, gearPreview, graphicsSettings, vibrationSettings, alphaSettings });
    this.gearCatalog = null;
    this.root = document.getElementById('frontend'); this.canvas = document.getElementById('menu-canvas');
    this.ctx = this.canvas.getContext('2d'); this.buttons = document.getElementById('menu-buttons');
    this.notice = document.getElementById('menu-notice');
    this.pausePanel = document.createElement('section'); this.pausePanel.className = 'pause-panel'; this.pausePanel.hidden = true;
    this.pausePanel.setAttribute('aria-labelledby', 'pause-title'); this.canvas.parentElement.append(this.pausePanel);
    this.loadingScreen = document.getElementById('loading-screen');
    this.installHelp = document.getElementById('app-install-help');
    this.backgroundCanvas = document.getElementById('menu-background');
    window.addEventListener('appdisplaychange', () => { this.layoutKey = ''; });
    this.transitions = []; this.transitionGhost = null;
    window.addEventListener('resize', () => this.captureTransition(null));
    matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', () => this.captureTransition(null));
    this.modelViews = new Map(); this.modelKey = ''; this.modelToken = 0; this.loadingStatus = '';
    this.assets = new Map(); this.images = new Map(); this.fonts = new Map();
    this.initialized = false; this.layoutKey = ''; this.layoutPending = false; this.loaded = new Set(); this.page = 'loading'; this.selection = 0; this.track = 0; this.busy = false; this.lastPad = new Set(); this.alive = true;
    const savedSelection = loadPlayerSelection();
    this.quadId = savedSelection.quadId; this.riderId = savedSelection.riderId; this.lastFreestyleEnvironment = savedSelection.freestyleEnvironment; this.lastRace = savedSelection.race; this.playerName = savedSelection.playerName; this.appearance = savedSelection.appearance; this.trackMode = 'single'; this.roomCode = ''; this.createCode = ''; this.garageReturn = 'single';
    this.bindingDevice = 'keyboard'; this.bindingPage = 0; this.bindingRow = 0; this.musicPage = 0;
    this.capture = null; this.menuKeys = new Set();
    this.raceSettings={...this.lastRace};this.raceActive=false;this.raceAvailable=false;this.raceCourses=[];
    this.trackFilter = 'all'; this.raceFilter = 'all'; this.trackUrls = new Map(); this.backdropUrl = ''; this.shotUrls = new Map(); this.loadingShots = []; this.shotFrame = 0;
    gameplayBindings.subscribe(() => { this.layoutKey = ''; });
    controllerSettings.subscribe(() => { this.layoutKey = ''; });
    // Controller names depend on whether the connected pad uses the standard layout.
    for (const type of ['gamepadconnected','gamepaddisconnected']) window.addEventListener(type, () => { this.layoutKey = ''; });
    window.addEventListener('keydown', e => {
      if (!this.capture) return;
      e.preventDefault(); e.stopImmediatePropagation();
      if (e.code === 'Escape') return this.cancelCapture();
      if (this.bindingDevice !== 'keyboard' || e.repeat) return;
      if (!/^[A-Za-z][A-Za-z0-9]{0,39}$/.test(e.code)) return;
      this.capture.held.add(e.code);
      if (!this.capture.atoms.some(a => a.key === e.code) && this.capture.atoms.length < 4) this.capture.atoms.push({key:e.code});
    }, true);
    window.addEventListener('keyup', e => {
      this.menuKeys.delete(e.code);
      if (!this.capture) return;
      e.preventDefault(); e.stopImmediatePropagation();
      if (this.bindingDevice !== 'keyboard') return;
      this.capture.held.delete(e.code);
      if (!this.capture.held.size && this.capture.atoms.length) this.finishCapture();
    }, true);
    window.addEventListener('blur', () => { this.padBlocked = true; this.menuKeys.clear(); this.cancelCapture(); void this.audio?.()?.setMusicPreview(false); });
    document.addEventListener('visibilitychange', () => { if (document.hidden) { this.padBlocked = true; this.menuKeys.clear(); this.cancelCapture(); } this.syncPreview(); });
    window.addEventListener('focus', () => this.syncPreview());
    window.addEventListener('keydown', e => {
      if (e.repeat || this.busy) return;
      if (this.root.hidden) return;
      if (e.target.matches?.('input') && e.code !== 'Escape') return;
      this.menuKeys.add(e.code);
      if (this.page === 'pause' && gameplayBindings.value('keyboard','pause',this.menuKeys,null) > .5) { e.preventDefault(); this.resume(); return; }
      const action = ({ ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right', Enter: 'accept', Space: 'accept', Escape: 'back', Backspace: 'back', PageUp: 'prevtab', PageDown: 'nexttab', KeyQ: 'prevtab', KeyE: 'nexttab' })[e.code];
      if (action) { e.preventDefault(); void this.action(action); }
    });
    window.addEventListener('pagehide', event => {
      this.captureTransition(null);
      void this.audio?.()?.setMusicPreview(false);
      cancelAnimationFrame(this.pollFrame); this.pollFrame = null; this.lastPad.clear();
      if (!event.persisted) this.alive = false;
    });
    window.addEventListener('pageshow', event => {
      if (event.persisted && this.alive && this.pollFrame === null) {
        this.lastPad.clear(); this.layoutKey = ''; this.pollFrame = requestAnimationFrame(this.poll);
      }
    });
    this.poll = () => {
      if (!this.alive) return;
      if (this.installHelp?.open) { this.padBlocked = true; this.lastPad.clear(); this.pollFrame = requestAnimationFrame(this.poll); return; }
      if (!document.hidden && document.hasFocus()) {
        // Menus follow the same controller as riding: Auto (the pad in use) or
        // the pad pinned in Controller Settings. Idle devices never navigate.
        const pad = activeGamepad(), pads = pad ? [pad] : [];
        if (this.capture) { this.pollCapture(pad); this.pollFrame = requestAnimationFrame(this.poll); return; }
        const held = new Set();
        for (const p of pads) {
          if (p.buttons[0]?.pressed) held.add('accept');
          if (p.buttons[9]?.pressed || gameplayBindings.value('controller','pause',null,p) > .5) held.add('pause');
          if (p.buttons[1]?.pressed || p.buttons[3]?.pressed) held.add('back');
          if (p.buttons[12]?.pressed || p.axes[1] < -.55) held.add('up');
          if (p.buttons[13]?.pressed || p.axes[1] > .55) held.add('down');
          if (p.buttons[14]?.pressed || p.axes[0] < -.55) held.add('left');
          if (p.buttons[15]?.pressed || p.axes[0] > .55) held.add('right');
          // Shoulder buttons switch the course list's event tabs.
          if (p.buttons[4]?.pressed) held.add('prevtab');
          if (p.buttons[5]?.pressed) held.add('nexttab');
        }
        if (!held.size) this.padBlocked = false;
        if (!this.root.hidden && !this.padBlocked) for (const action of held) if (!this.lastPad.has(action)) {
          if (action === 'pause' && this.page === 'garage' && this.garageReturn === 'pause' && !this.busy) void this.leaveGarage();
          else if (action === 'pause' && this.page === 'tracks' && this.trackMode === 'change' && !this.busy) { this.trackMode = 'single'; this.resume(); }
          else if (action === 'pause' && ['settings','extras','alpha','pause','players','hud-settings','minimap-config','race-overlays','controls','controller-settings','bindings','music','jukebox','multiplayer','create-lobby','join-lobby','lobby','garage'].includes(this.page) && !this.busy && this.activeSetup && !this.menuSettings()) this.resume();
          else void this.action(action === 'pause' ? 'accept' : action);
        }
        this.lastPad = held;
        if (!this.root.hidden && (!this.busy || this.page === 'loading')) void this.refresh();
      } else this.lastPad.clear();
      this.pollFrame = requestAnimationFrame(this.poll);
    };
    this.pollFrame = requestAnimationFrame(this.poll);
  }
  archive(path) {
    if (this.loaded.has(path)) return Promise.resolve();
    this.archivesPending ??= new Map();
    if (!this.archivesPending.has(path)) this.archivesPending.set(path, this.loadArchive(path).finally(() => this.archivesPending.delete(path)));
    return this.archivesPending.get(path);
  }
  async loadArchive(path) {
    const { file, files } = await this.content;
    const record = files.find(f => f.path === path);
    if (!record) throw new Error(`Original menu resource missing: ${path}`);
    const bytes = await this.read(file, record.lba * 2048, record.bytes);
    for (const [i, member] of (await this.json(2, bytes)).entries()) {
      const name = base(member.name);
      if (!this.assets.has(name)) this.assets.set(name, { bytes, i });
    }
    this.loaded.add(path);
  }
  async data(name) {
    const item = this.assets.get(base(name));
    if (!item) throw new Error(`Original UI member missing: ${name}`);
    return this.codec(3, item.bytes, item.i);
  }
  async document(name, section) {
    let bytes = await this.data(name);
    if (section) {
      // Overlays.pb has a stray un-commented authoring note outside the pause
      // section. Decode only the requested section, retaining its original bytes.
      const text = new TextDecoder('latin1').decode(bytes), headers = [...text.matchAll(/^\[([^\]\r\n]+)\][^\S\r\n]*\r?$/gm)];
      const index = headers.findIndex(m => m[1].toLowerCase() === section.toLowerCase());
      if (index < 0) throw new Error(`Original UI section missing: ${section}`);
      bytes = bytes.slice(headers[index].index, headers[index+1]?.index ?? bytes.length);
    }
    return sections(await this.json(54, bytes));
  }
  async image(name) {
    name = base(name).replace(/\.(tga|pst)$/, '');
    if (this.images.has(name)) return this.images.get(name);
    const bytes = await this.data(`${name}.pst`), info = await this.json(7, bytes), rgba = await this.codec(8, bytes);
    const image = this.bitmap(info, rgba); this.images.set(name, image); return image;
  }
  async trainingMapPreview() {
    // Training has no shipped TP preview pair. Use the approved browser capture
    // of this same venue; resource publication can follow the authored app.
    this.trainingPreviewPending ??= new Promise(resolve => {
      const image = new Image();
      image.onload = () => { this.images.set(trainingPreview, image); resolve(trainingPreview); };
      image.onerror = () => resolve('');
      image.src = `/devlog-images/${trainingPreview}.png`;
    });
    return this.trainingPreviewPending;
  }
  async imageUrl(canvas) {
    const blob = await new Promise(resolve => canvas.toBlob(resolve));
    if (!blob) throw new Error('Menu image encoding failed');
    return URL.createObjectURL(blob);
  }
  // Course preview archives register in their original order, so a member
  // name shared by two archives always resolves to the earlier one. Loading
  // stops at the archive holding `image` when one is named.
  async previewArchives(image) {
    const content = await this.content, archives = COURSE_PREVIEWS.filter(archive => content.files.some(f => f.path === archive));
    if (!image) content.prefetch?.(archives);
    for (const archive of archives) {
      if (image && this.assets.has(base(`${image}.pst`))) return;
      await this.archive(archive);
    }
  }
  // Original course photographs ship as two halves; join them once into a
  // blob URL so every HD screen can use a normal responsive <img>.
  async trackPhoto(track) {
    if (!track || this.trackUrls.has(track.id)) return this.trackUrls.get(track?.id) ?? '';
    let url = '';
    if (track.id === 'training') url = await this.trainingMapPreview() ? `/devlog-images/${trainingPreview}.png` : '';
    else if (track.image) {
      await this.previewArchives(`${track.image}b`);
      const a = await this.image(`${track.image}a`), b = await this.image(`${track.image}b`), joined = document.createElement('canvas');
      joined.width = a.width + b.width; joined.height = Math.max(a.height, b.height);
      const c = joined.getContext('2d', {willReadFrequently:true}); c.drawImage(a, 0, 0); c.drawImage(b, a.width, 0, b.width, a.height);
      url = await this.imageUrl(trimDark(joined));
    }
    this.trackUrls.set(track.id, url); return url;
  }
  // The original loader cycles six action shots per course. Their archives all
  // name members frame01..08, so decode them here rather than in the shared map.
  async courseShots(track) {
    const path = track && loadingShotsResource(track.id);
    if (!path) return [];
    if (!this.shotUrls.has(path)) this.shotUrls.set(path, (async () => {
      const {file, files} = await this.content, record = files.find(entry => entry.path === path);
      if (!record) return [];
      const bytes = await this.read(file, record.lba * 2048, record.bytes), entries = await this.json(2, bytes), urls = [];
      for (let frame = 2; frame <= 7; frame++) {
        const index = entries.findIndex(entry => base(entry.name) === `frame0${frame}.pst`);
        if (index < 0) continue;
        const image = await this.codec(3, bytes, index), info = await this.json(7, image);
        urls.push(await this.imageUrl(trimDark(this.bitmap(info, await this.codec(8, image)))));
      }
      return urls;
    })().catch(() => []));
    return this.shotUrls.get(path);
  }
  stopShots() { clearInterval(this.shotTimer); this.shotTimer = null; this.loadingShots = []; this.shotFrame = 0; }
  trackPhotoUrl(track) { return track ? this.trackUrls.get(track.id) ?? '' : ''; }
  trackOrder() {
    const order = this.tracks.map((_, index) => index);
    return this.trackFilter === 'all' ? order : order.filter(index => courseGroup(this.tracks[index].id) === this.trackFilter);
  }
  cycleTrackFilter(direction) {
    const groups = COURSE_GROUPS.map(([key]) => key).filter(key => key === 'all' || this.tracks.some(track => courseGroup(track.id) === key));
    this.setTrackFilter(groups[(groups.indexOf(this.trackFilter) + direction + groups.length) % groups.length]);
  }
  setTrackFilter(filter) {
    this.trackFilter = filter;
    const order = this.trackOrder();
    if (order.length && !order.includes(this.track)) this.track = order[0];
    this.scrollScreenSelection = true; this.layoutKey = ''; void this.refresh();
  }
  selectTrack(index) {
    if (this.busy) return;
    if (index === this.track) { void this.action('accept'); return; }
    this.track = index; this.layoutKey = ''; void this.refresh();
  }
  // Race setup lists every published race course under the same event tabs.
  raceTracks() {
    return this.raceCourses.map(id => this.tracks.find(track => track.id === id) ?? {id, name:id.toUpperCase(), image:''});
  }
  raceOrder() {
    const tracks = this.raceTracks();
    return this.raceFilter === 'all' ? tracks : tracks.filter(track => courseGroup(track.id) === this.raceFilter);
  }
  cycleRaceFilter(direction) {
    const groups = COURSE_GROUPS.map(([key]) => key).filter(key => key === 'all' || this.raceCourses.some(id => courseGroup(id) === key));
    this.setRaceFilter(groups[(groups.indexOf(this.raceFilter) + direction + groups.length) % groups.length]);
  }
  setRaceFilter(filter) {
    this.raceFilter = filter;
    const order = this.raceOrder();
    if (order.length && !order.some(track => track.id === this.raceSettings.course)) this.raceSettings.course = order[0].id;
    this.scrollScreenSelection = true; this.layoutKey = ''; void this.refresh();
  }
  selectRaceCourse(id) {
    if (this.busy || !this.raceCourses.includes(id)) return;
    this.raceSettings.course = id; this.selection = Math.max(0, this.frame.choices.indexOf('racetrack'));
    this.layoutKey = ''; void this.refresh();
  }
  garageCatalog() {
    const online = this.onlineGarage();
    return (this.frame?.catalog ?? []).filter(item => !online || item.group === 'ATV');
  }
  // Multiplayer garages (menus, or Change ATV and Rider mid-session) are ATV only.
  onlineGarage() { return ['multiplayer','create-lobby','join-lobby','lobby'].includes(this.garageReturn) || (this.garageReturn === 'pause' && Boolean(this.multiplayer?.state?.lobby)); }
  // ATV stats panel: original uiQuad.txt Engine/Traction/Suspension values,
  // the maker logo, and the authored physics top speed. None for motorcycles.
  vehicleStats() {
    const info = this.quadInfo?.get(this.quadId);
    if (!info || this.quadId >= 27) return null;
    const figures = this.vehicleFigures?.quadId === this.quadId ? this.vehicleFigures : null;
    return { ...info, logoUrl: this.statsLogo?.name === info.logo ? this.statsLogo.url : '', topSpeed: figures?.top_speed ?? null };
  }
  async refreshStatsLogo() {
    const info = this.quadInfo?.get(this.quadId);
    if (!info?.logo || this.statsLogo?.name === info.logo) return;
    try { const url = await this.imageUrl(await this.image(info.logo)); this.statsLogo = { name: info.logo, url }; this.layoutKey = ''; void this.refresh(); } catch { /* logo is optional */ }
  }
  vehicleSummary() {
    const catalog = this.frame?.catalog?.length ? this.garageCatalog() : [];
    const item = catalog.find(entry => entry.id === this.quadId), group = catalog.filter(entry => entry.group === item?.group);
    const rider = this.quadId >= 27 ? 'MX rider' : this.riderId ? 'Female rider' : 'Male rider';
    return {label:item?.label ?? `Vehicle ${this.quadId}`, group:item?.group === 'Motorcycle' ? 'Motorcycle' : 'ATV', index:group.indexOf(item) + 1, count:group.length, rider,
      meta:[item?.class, rider].filter(Boolean).join(' / ')};
  }
  bitmap(info, rgba) {
    const c = document.createElement('canvas'); c.width = info.width; c.height = info.height;
    c.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(rgba), info.width, info.height), 0, 0); return c;
  }
  async font(name) {
    name = name.toLowerCase(); if (this.fonts.has(name)) return;
    const font = await this.json(55, await this.data(`${name}.cel`));
    font.image = await this.image(name); font.glyphs = new Map(font.glyphs.map(g => [g.code, g]));
    font.height = font.glyphs.values().next().value.metrics?.[3] ?? 25; font.tints = new Map(); this.fonts.set(name, font);
  }
  async init(showTitle = true) {
    try {
      await this.archive('/STARTAPP.RES');
      const tracks = await this.document('tracks.dat');
      const {files} = await this.content;
      this.tracks = unlockedCourses(tracks,files);
      // Tracks.dat category: Pro Supercross (prosx) courses use the Pro crowd.
      this.proSupercross = new Set([...tracks].filter(([section]) => /^prosx\d+$/.test(section)).map(([, f]) => f.scn));
      this.track = Math.max(0, this.tracks.findIndex(t => t.id === this.lastFreestyleEnvironment));
      for (const name of [fontName, 'tradegothic_boldcondensed_22soff', 'tradegothic_boldcondenseditalic_-1fs_32ds']) await this.font(name);
      await this.archive('/UI/RESOURCE/PAGEA/PAGE01.RES');
      // PAGE01's opaque red-tinted scenic backdrop sits behind the HD screens.
      if (this.assets.has('bk_mainmenu.pst')) { try { this.backdropUrl = await this.imageUrl(await this.image('bk_mainmenu')); } catch { /* gradient-only backdrop */ } }
      // Authored vector branding is independent of recovered disc textures.
      // Keep the text fallback usable if the static asset cannot be decoded.
      const logo = new Image(); logo.src = 'fury-remastered.svg';
      try { await logo.decode(); this.images.set('brand-logo', logo); } catch { /* Rust text fallback */ }
      // The thumbnail arrives through a separately validated R2 package. Older
      // manifests can still open every menu while that content update publishes.
      // Each original course's race archive is offered once its R2 record exists.
      this.raceCourses=RACE_COURSES.filter(id=>files.some(f=>f.path===courseResource(id,true).archive));
      this.raceAvailable=this.raceCourses.length>0;
      if(this.raceAvailable&&!this.raceCourses.includes(this.raceSettings.course))this.raceSettings.course=this.raceCourses[0];
      if (files.some(f => f.path === '/UI/RESOURCE/QUADINFO.RES')) {
        await this.archive('/UI/RESOURCE/QUADINFO.RES'); await this.image('multi_atv01');
        // Original ATV select page stats: Engine/Traction/Suspension per IngameID.
        try {
          this.quadInfo = new Map();
          for (const [name, values] of await this.document('uiQuad.txt')) {
            const get = key => values[key] ?? values[Object.keys(values).find(k => k.toLowerCase() === key)], id = Number(get('ingameid')), number = key => Math.min(1, Math.max(0, Number(get(key))));
            if (!/^quad\d+$/.test(name) || !Number.isInteger(id)) continue;
            this.quadInfo.set(id, { name: (get('name') ?? '').replace(/^"|"$/g, ''), logo: (get('image') ?? '').replace(/"/g, '').trim(), engine: number('engine_range'), traction: number('traction_range'), suspension: number('suspension_range') });
          }
        } catch { this.quadInfo = null; }
      }
      // Original Profile Store gear catalog; older content packages omit it.
      const gearRecord = files.find(f => f.path === '/UI/RESOURCE/RIDEINFO.RES');
      if (gearRecord) {
        const bytes = await this.read((await this.content).file, gearRecord.lba * 2048, gearRecord.bytes), entries = await this.json(2, bytes);
        const index = entries.findIndex(e => e.name.toLowerCase().endsWith('uirider.txt'));
        if (index >= 0) this.gearCatalog = await this.json(107, await this.codec(3, bytes, index));
      }
      // The original catalog is available before a terrain/engine is loaded.
      // The soundtrack catalog and menu tunes stream in behind the title
      // screen instead of holding it until the whole sound bank downloads.
      const content = await this.content, audio = this.audio?.();
      content.prefetch?.(['/AUDIO/AUDIO.RES'], 'low', { head: true });
      if (audio) void (async () => {
        await audio.music.load({ ...content, read: this.read, isCurrent: () => this.alive });
        await audio.loadMenuMusic({ ...content, read: this.read, isCurrent: () => this.alive });
        this.syncPreview(); this.layoutKey = ''; void this.refresh();
      })().catch(error => audio.report(error.message));
      this.initialized = true;
      if (showTitle) await this.openLaunch(location.search);
    } catch (error) { this.fail(error); }
  }
  async openLaunch(search) {
    const shortcut = menuShortcut(search);
    if (shortcut) return this.show(shortcut);
    const setup = parseLaunch(search, this.tracks);
    if (!setup.requested) return this.show('title');
    if (setup.errors.length) { await this.show('main'); this.notice.textContent = `Cannot open ride link. ${setup.errors.join(' ')}`; return; }
    Object.assign(this, {quadId:setup.quadId,riderId:setup.riderId,playerName:setup.playerName,roomCode:setup.room,createCode:setup.action==='create'?setup.room:''});
    // Only race links change the race setup; a Freestyle link keeps the saved one.
    if (setup.activity==='race') this.raceSettings = {...setup.raceSettings};
    this.track = Math.max(0,this.tracks.findIndex(t=>t.id===setup.map));
    const page = setup.mode==='multiplayer' ? (setup.action==='create'?'create-lobby':'join-lobby') : setup.activity==='race'?'race':'tracks';
    // A ride link goes straight to its loading screen; the course photos
    // behind it load when that menu is next opened.
    await this.show(page, { previews: !setup.play });
    if (!setup.play) return;
    try {
      if (setup.mode==='multiplayer') {
        // A join link reopens its room on the link's map if the room ended.
        await this.multiplayer.action(setup.action==='create'?'host':'join',this,{map:setup.map,start:true});
      } else {
        if (setup.activity==='race' && !this.raceCourses.includes(setup.map)) throw Error('Race resources for this course are unavailable. Try Freestyle.');
        await this.launchSingle(setup.map,setup.activity==='race'?setup.raceSettings:null);
      }
    } catch (error) {
      if (setup.mode==='multiplayer') await this.multiplayer.disconnect();
      await this.show(page); this.fail(error);
    }
  }
  async launchSingle(environment, racing = null) {
    await this.multiplayer?.disconnect(); this.busy = true;
    try {
      await this.start(environment,racing);
      this.raceActive = Boolean(racing);
      this.activeSetup = {map:environment,activity:racing?'race':'freestyle',quadId:this.quadId,riderId:this.riderId,raceSettings:racing?{...racing}:undefined};
      if (!racing) this.lastFreestyleEnvironment = environment;
      else this.lastRace = normalizedRace({...racing, course:environment});
      this.saveSelection(); this.resume();
    } finally { this.busy = false; }
  }
  rideUrl() {
    const room = this.multiplayer?.state?.code;
    const racing = this.page==='race';
    const setup = room ? {mode:'multiplayer',room,map:this.multiplayer.state.environment,quadId:this.quadId,riderId:this.riderId} :
      overlayPages.has(this.page) && this.activeSetup ? this.activeSetup :
      {map:racing?this.raceSettings.course:this.tracks[this.track].id,activity:racing?'race':'freestyle',quadId:this.quadId,riderId:this.riderId,raceSettings:this.raceSettings};
    return buildRideUrl(location.origin,{...setup,playerName:this.playerName},{includePlayer:!room});
  }
  fail(error) { this.setLoading(false); this.notice.textContent = error.message; this.busy = false; this.root.dataset.error = 'true'; }
  // Settings opened from the main menu belong to the frontend, not the ride.
  menuSettings(page = this.page) { return settingsPages.has(page) && this.settingsReturn === 'main'; }
  syncPreview() {
    const rideMenu = this.isPausePanel() && !this.menuSettings();
    // The racing soundtrack plays in ride menus and the Jukebox, or once a
    // song is chosen on Audio.
    const soundtrack = rideMenu || this.page === 'jukebox' || (this.page === 'music' && this.songPreview);
    void this.audio?.()?.setMusicPreview((this.isPausePanel() || ['title','main','single','tracks','garage','multiplayer','create-lobby','join-lobby','lobby','race','loading','music'].includes(this.page)) && !this.root.hidden && !document.hidden && document.hasFocus(), !soundtrack).catch(error => this.fail(error));
  }
  beginCapture(id) {
    const row = gameplayBindings.rows(this.bindingDevice).find(row => row.id === id);
    if (!row) return;
    this.bindingRow = this.bindingPage * 4 + this.selection;
    this.capture = {id, label:row.label, atoms:[], held:new Set(), armed:false};
    gameplayBindings.cancel(); this.menuKeys.clear(); this.layoutKey = ''; void this.refresh();
  }
  cancelCapture() {
    if (!this.capture) return;
    this.capture = null; this.menuKeys.clear(); gameplayBindings.cancel(); this.layoutKey = ''; void this.refresh();
  }
  finishCapture() {
    const capture = this.capture; if (!capture?.atoms.length) return;
    const binding = capture.atoms.length === 1 ? capture.atoms[0] : {chord:capture.atoms};
    gameplayBindings.setBinding(this.bindingDevice,capture.id,binding);
    this.capture = null; this.menuKeys.clear(); this.layoutKey = ''; this.notice.textContent = gameplayBindings.storageError ?? ''; void this.refresh();
  }
  pollCapture(pad) {
    if (this.bindingDevice !== 'controller' || !this.capture) return;
    if (!pad) { this.capture.armed = false; this.capture.atoms = []; return; }
    const atoms = [];
    pad.buttons.forEach((b,i) => { if ((b.value ?? Number(b.pressed)) > .5) atoms.push({button:i}); });
    pad.axes.forEach((v,i) => { if (Math.abs(v) > .65) atoms.push({axis:i,sign:v < 0 ? -1 : 1}); });
    if (!this.capture.armed) { if (!atoms.length) this.capture.armed = true; return; }
    for (const atom of atoms) if (!this.capture.atoms.some(a => JSON.stringify(a) === JSON.stringify(atom) || ('axis' in atom && a.axis === atom.axis)) && this.capture.atoms.length < 4) this.capture.atoms.push(atom);
    if (!atoms.length && this.capture.atoms.length) this.finishCapture();
  }
  resume() { stopJukebox(this); this.stopShots(); this.animatePage(true); this.setLoading(false); this.clearModelViews(); this.cancelCapture(); this.root.hidden = true; this.active(false); void this.audio?.()?.setMusicPreview(false); }
  async show(page, { previews = true } = {}) {
    this.padBlocked = true;
    const previous = this.root.hidden ? null : this.canvas;
    this.captureTransition(previous);
    this.cancelCapture();
    // Menu pages share one quad and one rider preview: keep their views (and
    // WebGL contexts) and move them into the new page instead of recreating
    // and reloading them on every switch. Browsers, iOS Safari especially,
    // cap live WebGL contexts and evict older ones, including the game's.
    if (!(MODEL_PAGES.includes(this.page) && MODEL_PAGES.includes(page))) this.clearModelViews();
    delete this.root.dataset.error;
    // Arm soundtrack streaming before silencing gameplay voices, avoiding a
    // suspend/resume interruption on entry to the pause screen.
    if (overlayPages.has(page) && !this.menuSettings(page)) void this.audio?.()?.setMusicPreview(!document.hidden && document.hasFocus(), false);
    if (page !== 'music') this.songPreview = false;
    this.busy = true; this.root.hidden = false; this.active(true); this.notice.textContent = ''; this.buttons.replaceChildren();
    try {
      if (page === 'loading') await this.trackPhoto(this.loadingTrack);
      else if (previews && (page === 'tracks' || page === 'race' || page === 'lobby')) {
        if (!this.loaded.has('/UI/RESOURCE/TP/TP_ANAT.RES')) this.setLoading(true, 'Preparing course previews…');
        await this.previewArchives();
        if (page !== 'lobby') await Promise.all(this.tracks.map(track => this.trackPhoto(track)));
      }
      this.page = page; this.selection = 0; this.layoutKey = '';
      this.watchFreerideRiders(page === 'main');
      if (this.isPausePanel()) this.resetPauseScroll = true;
      this.syncPreview();
      // A previous worker layout may still be in flight during rapid navigation.
      while (this.layoutPending) await new Promise(resolve => requestAnimationFrame(resolve));
      await this.refresh(true);
      this.busy = false; this.setLoading(false); this.animatePage();
    } catch (error) { this.fail(error); }
  }
  async action(action) {
    if (this.busy || this.root.hidden) return;
    this.notice.textContent = '';
    if (['up','down'].includes(action) && ['title','main','single','garage','multiplayer','create-lobby','join-lobby','lobby','race','settings','extras','alpha','pause','players','hud-settings','minimap-config','race-overlays','controls','controller-settings','bindings','music','jukebox'].includes(this.page)) {
      const order = this.page === 'pause' ? this.pauseOrder : this.page === 'garage' ? this.frame.choices.filter(choice => choice !== 'quadnext') : this.frame.choices;
      const current = this.frame.choices[this.selection], index = order.indexOf(current === 'quadnext' ? 'quadprev' : current);
      this.selection = this.frame.choices.indexOf(order[(index + (action === 'up' ? -1 : 1) + order.length) % order.length]);
      this.scrollPauseSelection = this.isPausePanel(); this.scrollScreenSelection = !this.isPausePanel();
      if (this.page === 'bindings' && this.selection < 4) this.bindingRow = this.bindingPage * 4 + this.selection;
    } else if (action === 'back') {
      if (this.page === 'pause') return this.resume();
      if (this.page === 'garage') return this.leaveGarage();
      if (this.page === 'gear') return this.show('garage');
      if (this.page === 'tracks' && this.trackMode !== 'single') { const mode = this.trackMode; this.trackMode = 'single'; return this.show(mode === 'host' ? 'create-lobby' : 'pause'); }
      if (this.page === 'lobby') return this.run('leave');
      if (this.page === 'settings') return this.show(this.settingsReturn ?? 'main');
      if (this.page === 'jukebox') return this.show(this.jukeboxReturn ?? 'main');
      return this.show(({settings:this.settingsReturn ?? 'main',extras:'settings',alpha:'settings',players:'pause',bindings:this.bindingDevice === 'controller' ? 'controller-settings' : 'controls','controller-settings':'controls',music:'settings','hud-settings':'settings','minimap-config':'hud-settings','race-overlays':'hud-settings',controls:'settings',main:'title',single:'main',tracks:'single',race:'single','create-lobby':'multiplayer','join-lobby':'multiplayer',multiplayer:'main'})[this.page] ?? 'title');
    } else if (this.page === 'garage' && ['left','right'].includes(action)) {
      return this.run(action === 'left' ? 'quadprev' : 'quadnext');
    } else if (this.page === 'tracks' && ['left','right','up','down'].includes(action)) {
      // Every direction steps through the courses visible under the event filter.
      const order = this.trackOrder(), position = Math.max(0, order.indexOf(this.track)) + (['left','up'].includes(action) ? -1 : 1);
      this.track = order[(position + order.length) % order.length]; this.scrollScreenSelection = true;
    } else if (this.page === 'tracks' && ['prevtab','nexttab'].includes(action)) {
      return this.cycleTrackFilter(action === 'prevtab' ? -1 : 1);
    } else if (this.page === 'race' && ['prevtab','nexttab'].includes(action)) {
      return this.cycleRaceFilter(action === 'prevtab' ? -1 : 1);
    } else if (['race','gear'].includes(this.page) && ['left','right'].includes(action)) {
      // Only value rows step; Left/Right on Start Race, Copy link or Back does nothing.
      const current = this.frame.choices[this.selection];
      if (stepperActions.has(current)) return this.run(current, action === 'left' ? -1 : 1, true);
    } else if (['left','right'].includes(action) && (cycleActions.has(this.frame.choices[this.selection]) || this.frame.choices[this.selection]?.startsWith('alpha:')) && this.isPausePanel()) {
      return this.run(this.frame.choices[this.selection], action === 'left' ? -1 : 1, true);
    } else if (this.page === 'minimap-config' && this.frame.choices[this.selection] === 'minimap-size' && ['left','right'].includes(action)) {
      this.minimapSettings?.setSize(this.minimapSettings.size + (action === 'left' ? -8 : 8));
    } else if (this.page === 'minimap-config' && this.frame.choices[this.selection] === 'minimap-zoom' && ['left','right'].includes(action)) {
      this.minimapSettings?.setZoom(this.minimapSettings.zoom + (action === 'left' ? -25 : 25));
    } else if (this.page === 'controller-settings' && ['left','right'].includes(action) && (['padselect','padpreset'].includes(this.frame.choices[this.selection]) || this.frame.choices[this.selection]?.startsWith('pad-'))) {
      const current = this.frame.choices[this.selection], sign = action === 'left' ? -1 : 1;
      if (current === 'padselect') cycleController(sign);
      else if (current === 'padpreset') gameplayBindings.cycleControllerPreset(sign);
      else controllerSettings.step(current.slice(4), sign);
    } else if (this.page === 'controls' && this.frame.choices[this.selection] === 'steering-sensitivity' && ['left','right'].includes(action)) {
      this.mobileControls?.setSteeringSensitivity(this.mobileControls.steeringSensitivity + (action === 'left' ? -5 : 5));
    } else if (this.page === 'jukebox' && ['left','right'].includes(action)) {
      // Left/Right scrub the seek bar, step the volume, or change track.
      const current = this.frame.choices[this.selection], music = this.audio?.()?.music, sign = action === 'left' ? -1 : 1;
      if (current === 'jukeseek') music?.seek(music.snapshot().positionSeconds + sign * JUKEBOX_SEEK_STEP);
      else if (current === 'jukevolume') music?.setVolume(Math.round(music.volume * 20 + sign) / 20);
      else return this.run(sign < 0 ? 'jukeprev' : 'jukenext');
    } else if (['bindings','music'].includes(this.page) && ['left','right'].includes(action)) {
      return this.run(`${this.page === 'music' ? 'music' : 'bind'}${action === 'left' ? 'prev' : 'next'}`);
    } else if (action === 'accept') {
      if (this.page === 'title') return this.run(this.frame.choices[this.selection]);
      // Accepting the course list starts the race, as Ride does in Freestyle.
      if (this.page === 'race' && this.frame.choices[this.selection] === 'racetrack') return this.run('racestart');
      if (this.page === 'tracks') {
        const environment = this.tracks[this.track].id, mode = this.trackMode;
        this.busy = true;
        try {
          if (mode === 'host') { this.trackMode = 'single'; this.saveSelection(); await this.multiplayer.action('host', this, {map:environment}); }
          else if (mode === 'change') { this.trackMode = 'single'; await this.multiplayer.setMap(environment); this.busy = false; this.resume(); }
          else await this.launchSingle(environment);
        }
        catch (error) { if (mode !== 'single') { this.trackMode = mode; } await this.show(mode === 'host' ? 'create-lobby' : mode === 'change' ? 'pause' : 'tracks'); this.fail(error); }
        this.busy = false; return;
      }
      return this.run(this.frame.choices[this.selection]);
    }
    await this.refresh();
  }
  // `stepped` marks Left/Right (or an arrow button) rather than a tap that cycles.
  async run(action,direction=1,stepped=false) {
    if (this.busy) return;
    try {
      if (action === 'copyride') return await copyRideUrl(this.rideUrl(),this.notice);
      if (action === 'settings') { this.settingsReturn = this.page === 'pause' ? 'pause' : 'main'; return this.show('settings'); }
      if (action === 'jukebox') { this.jukeboxReturn = this.settingsReturn = this.page === 'pause' ? 'pause' : 'main'; return this.show('jukebox'); }
      if (action === 'garage') { this.garageReturn = this.page; return this.show('garage'); }
      if (action === 'changeride') { this.garageReturn = 'pause'; this.rideBeforeGarage = [this.quadId, this.riderId, appearanceBytes(this.appearance).join('.')]; return this.show('garage'); }
      if (action === 'hosttrack') {
        // Hosting picks a track first; in a session the host moves everyone.
        if (this.page === 'create-lobby' && this.quadId >= 27) throw new Error('Choose an ATV for multiplayer. Motorcycles are available in Single Player.');
        this.trackMode = this.page === 'create-lobby' ? 'host' : 'change';
        const current = this.tracks.findIndex(t => t.id === this.multiplayer?.state?.environment);
        if (this.trackMode === 'change' && current >= 0) this.track = current;
        return this.show('tracks');
      }
      if (action.startsWith('vehicle:')) { const id = Number(action.slice(8)); if (this.garageCatalog().some(item => item.id === id)) { this.quadId = id; this.scrollScreenSelection = true; } }
      else if (action === 'quadprev' || action === 'quadnext') { const count = this.onlineGarage() ? 26 : 46; this.quadId = ((this.quadId - 1 + (action === 'quadnext' ? 1 : count - 1)) % count) + 1; }
      else if (action === 'riderchange' && this.quadId < 27) this.riderId = 1 - this.riderId;
      else if (action.startsWith('spawn:')) { this.busy=true; try { await this.multiplayer.action(action,this); } finally {this.busy=false;} return; }
      else if (action === 'freeride') {
        // No setup: the remembered name and garage ATV ride straight in.
        // Motorcycles stay in Single Player, so they fall back to the stock ATV.
        if (this.quadId >= 27) this.quadId = 1;
        // Many riders keep the default name; number it so strangers can tell them apart.
        if (['','Rider'].includes(this.playerName.trim())) this.playerName = `Rider ${1000 + Math.floor(Math.random() * 9000)}`;
        this.busy = true;
        try { await this.multiplayer.action('freeride', this); }
        catch (error) { await this.multiplayer.disconnect(); await this.show('main'); throw error; }
        finally { this.busy = false; }
        return;
      }
      else if (['host','join','share','leave'].includes(action)) { if (['host','join'].includes(action) && this.quadId>=27) throw new Error('Choose an ATV for multiplayer. Motorcycles are available in Single Player.'); if (['host','join'].includes(action)) this.saveSelection(); return await this.multiplayer.action(action, this); }
      else if (['main','single','tracks','settings','extras','alpha','pause','players','hud-settings','minimap-config','race-overlays','controls','music','multiplayer','create-lobby','join-lobby','lobby','race','gear'].includes(action)) return await this.show(action);
      if (action.startsWith('gear') && action !== 'gear') { this.stepGear(action.slice(4), direction); this.saveSelection(); this.layoutKey = ''; await this.refresh(); return; }
      if (action === 'racestart') {
        try { await this.launchSingle(this.raceSettings.course, {...this.raceSettings}); }
        catch(error) { await this.show('race');this.fail(error); } finally { this.busy = false; }
        return;
      }
      if (['racetrack','racelaps','racecpu','racedifficulty'].includes(action)) {
        // Stepping the course walks the list shown under the current event tab.
        if (action === 'racetrack' && this.raceCourses.length) { const courses = this.raceOrder().map(track => track.id), at = Math.max(0, courses.indexOf(this.raceSettings.course)); this.raceSettings.course = courses[(at + direction + courses.length) % courses.length]; this.scrollScreenSelection = true; }
        if (action === 'racelaps') this.raceSettings.laps = (this.raceSettings.laps - 1 + direction + 5) % 5 + 1;
        if (action === 'racecpu') this.raceSettings.cpu = RACE_CPU[(RACE_CPU.indexOf(this.raceSettings.cpu) + direction + RACE_CPU.length) % RACE_CPU.length];
        if (action === 'racedifficulty') this.raceSettings.difficulty = this.raceSettings.difficulty === 2 ? 1 : 2;
        this.layoutKey = ''; await this.refresh(); return;
      }
      if (action === 'keyboard' || action === 'padbindings') { this.bindingDevice = action === 'keyboard' ? 'keyboard' : 'controller'; this.bindingPage = 0; this.bindingRow = 0; return this.show('bindings'); }
      if (action === 'controller') return this.show('controller-settings');
      if (action.startsWith('bind:')) { const id = action.slice(5); this.selection = gameplayBindings.rows(this.bindingDevice).findIndex(row => row.id === id) % 4; return this.beginCapture(id); }
      if (action === 'bindcancel') return this.cancelCapture();
      if (['bindprev','bindnext','musicprev','musicnext'].includes(action)) {
        const music = action.startsWith('music'), rows = music ? (this.audio?.()?.music?.tracks ?? []) : gameplayBindings.rows(this.bindingDevice), pages = Math.max(1,Math.ceil(rows.length / 4)), prop = music ? 'musicPage' : 'bindingPage';
        this[prop] = (this[prop] + (action.endsWith('prev') ? -1 : 1) + pages) % pages; this.selection = 0; this.scrollPauseSelection = true;
        if (!music) this.bindingRow = this.bindingPage * 4;
      } else if (action === 'bindclear') { const row = gameplayBindings.rows(this.bindingDevice)[this.bindingRow]; if (row) gameplayBindings.setBinding(this.bindingDevice,row.id,[]); }
      else if (action === 'bindreset') gameplayBindings.reset(this.bindingDevice);
      else if (action.startsWith('song:')) { const music = this.audio?.()?.music; music?.setEnabled(true); music?.setPaused(false); music?.select(Number(action.slice(5))); this.songPreview = true; this.syncPreview(); }
      // Jukebox Pause is transient; Play also turns the saved Music setting on.
      else if (action === 'jukeplay') { const music = this.audio?.()?.music; if (music) { const playing = music.enabled && !music.paused; if (!playing) music.setEnabled(true); music.setPaused(playing); } }
      else if (action === 'jukeprev' || action === 'jukenext') { const music = this.audio?.()?.music; music?.setEnabled(true); music?.setPaused(false); if (action === 'jukeprev') music?.previous(); else music?.next(); }
      else if (action === 'jukeseek' || action === 'jukevolume') { this.pausePanel.querySelector(`input[data-setting="${action}"]`)?.focus(); return; }
      else if (action === 'musictoggle') { const music = this.audio?.()?.music; if (music) music.setEnabled(!music.snapshot().enabled); this.songPreview = true; this.syncPreview(); }
      else if (action === 'musicvolume') {
        const music = this.audio?.()?.music, step = Math.round((music?.snapshot().volume ?? 0) * 4) + direction;
        // A tap wraps 100% back to off; Left/Right stop at either end.
        if (music) music.setVolume((stepped ? Math.min(4, Math.max(0, step)) : (step + 5) % 5) / 4);
      }
      else if (action === 'soundmode') { const audio = this.audio?.(); audio?.setSoundMode?.(audio.settings?.mode === 'original' ? 'enhanced' : 'original'); }
      else if (action === 'silentmode') { const audio = this.audio?.(); audio?.setSilentMode?.(audio.settings?.silent === 'mute' ? 'play' : 'mute'); }
      else if (action === 'sfxvolume') {
        const audio = this.audio?.(), step = Math.round((audio?.settings?.sfx ?? 1) * 4) + direction;
        if (audio?.setSfxVolume) audio.setSfxVolume((stepped ? Math.min(4, Math.max(0, step)) : (step + 5) % 5) / 4);
      }
      else if (action === 'padselect') cycleController(direction);
      else if (action === 'padpreset') gameplayBindings.cycleControllerPreset(direction);
      else if (action === 'paddefaults') controllerSettings.reset();
      else if (action.startsWith('pad-')) { this.pausePanel.querySelector(`input[data-setting="${action}"]`)?.focus(); return; }
      else if (action === 'steering-sensitivity') { this.pausePanel.querySelector('input[data-setting=steering-sensitivity]')?.focus(); return; }
      else if (action === 'minimap-zoom') { this.pausePanel.querySelector('input[data-setting="minimap-zoom"]')?.focus(); return; }
      else if (action === 'minimap-size') { this.pausePanel.querySelector('input[data-setting="minimap-size"]')?.focus(); return; }
      else if (action === 'minimap-orientation') this.minimapSettings?.toggleOrientation();
      else if (action === 'minimap-shape') this.minimapSettings?.toggleShape();
      else if (action === 'minimap-corner') this.minimapSettings?.toggleCorner();
      else if (action === 'minimap-toggle') this.minimapSettings?.toggle();
      else if (action === 'overlay-position') this.raceOverlays?.cyclePosition();
      else if (action.startsWith('overlay-')) this.raceOverlays?.toggle(action.slice(8));
      else if (action === 'onehanded') this.mobileControls?.toggleOneHanded();
      else if (action === 'speed-rush') this.crashSettings?.toggleSpeed(direction);
      else if (action === 'graphics') this.graphicsSettings?.cycle(direction);
      else if (action === 'vibration') this.vibrationSettings?.toggle();
      else if (action === 'crash-camera') this.crashSettings?.toggleCamera();
      else if (action === 'chase-camera') this.crashSettings?.toggleChase();
      else if (action === 'flips') this.crashSettings?.toggleFlips();
      else if (action.startsWith('alpha:')) this.alphaSettings?.toggle(action.slice(6));
      else if (action === 'train-riding') this.crashSettings?.toggleTrain();
      else if (action === 'funnycrashes') this.crashSettings?.toggle(direction);
      else if (action === 'diagnostics') this.diagnosticsOverlay?.cycle(direction);
      else if (action === 'resume') return this.resume();
      else if (action === 'reset') { await document.getElementById('reset').onclick({restart:true}); return this.resume(); }
      else if (action === 'fullscreen') {
        if (document.fullscreenElement) await document.exitFullscreen(); else await document.documentElement.requestFullscreen();
      } else if (action === 'devlog') location.href = 'devlog.html';
      else if (!['quadprev','quadnext','riderchange'].includes(action) && !action.startsWith('vehicle:')) return await this.action(action);
      if (['quadprev','quadnext','riderchange'].includes(action) || action.startsWith('vehicle:')) { this.saveSelection(); this.scrollScreenSelection = true; }
      this.layoutKey = ''; await this.refresh();
    } catch (error) { this.fail(error); }
  }
  // Leaving the garage mid-session applies a changed ATV/rider in place.
  async leaveGarage() {
    const previous = this.rideBeforeGarage; this.rideBeforeGarage = null;
    if (this.garageReturn !== 'pause') return this.show(this.garageReturn);
    if (!this.multiplayer?.state?.lobby || !previous || (previous[0] === this.quadId && previous[1] === this.riderId && previous[2] === appearanceBytes(this.appearance).join('.'))) return this.show('pause');
    this.saveSelection(); this.busy = true;
    try { await this.multiplayer.changeRide(this); }
    catch (error) { await this.show('pause'); this.fail(error); }
    finally { this.busy = false; }
  }
  saveSelection() {
    savePlayerSelection({quadId:this.quadId, riderId:this.riderId, freestyleEnvironment:this.lastFreestyleEnvironment, race:this.lastRace, playerName:this.playerName, appearance:this.appearance});
  }
  // Cycle one gear row through Original and the catalog items. The jersey sets
  // the shirt and its matching sleeves together, as the original store does.
  stepGear(row, direction = 1) {
    const a = normalizedAppearance(this.appearance);
    if (row === 'paint') a.paint = (a.paint + direction + PAINT_NAMES.length) % PAINT_NAMES.length;
    else {
      const slot = row === 'jersey' ? 'shirt' : row, at = GEAR_SLOTS.indexOf(slot), items = this.gearCatalog?.[at]?.items ?? [];
      if (at < 0 || !items.length) return;
      a.gear[at] = (a.gear[at] + direction + items.length + 1) % (items.length + 1);
      if (row === 'jersey') a.gear[GEAR_SLOTS.indexOf('sleeve')] = Math.min(a.gear[at], this.gearCatalog[GEAR_SLOTS.indexOf('sleeve')]?.items.length ?? 0);
    }
    this.appearance = a;
  }
  padStandard() { return standardPad(activeGamepad()); }
  gearRows() {
    // The original catalog names pants/sleeves generically ("pants 10"); their
    // index matches the jersey line of the same brand.
    const a = normalizedAppearance(this.appearance), name = slot => { const at = GEAR_SLOTS.indexOf(slot), index = a.gear[at]; const item = index ? this.gearCatalog?.[at]?.items[index - 1]?.name : null; return !item ? 'Original' : /^(pants|sleeves) \d+$/i.test(item) ? this.gearCatalog?.[GEAR_SLOTS.indexOf('shirt')]?.items[index - 1]?.name ?? item : item; };
    return [name('helmet'), name('shirt'), name('pant'), name('glove'), name('boot'), name('goggle'), PAINT_NAMES[a.paint]];
  }
  async refresh(force = false) {
    if (!this.initialized || this.root.hidden || (this.busy && this.page !== 'loading' && !force) || this.layoutPending) return;
    const stage = this.canvas.parentElement, track = this.page === 'loading' ? this.loadingTrack : this.page === 'lobby' ? this.tracks.find(t=>t.id===this.multiplayer?.state?.environment) : this.page === 'race' ? this.tracks.find(t=>t.id===this.raceSettings.course) : this.tracks[this.track];
    const music = this.audio?.()?.music, musicState = music?.snapshot();
    const lobby = this.multiplayer?.state?.lobby;
    const request = {lobby_players:(lobby?.players??[]).map(p=>({id:String(p.id),label:`${p.name}${p.id===this.multiplayer.state.peer?' (You)':''}`,value:p.id===this.multiplayer.state.peer?'self':(this.multiplayer.remote.find(r=>r.peer===p.id && !r.stale)?'ready':'waiting')})),race_laps:this.raceSettings.laps,race_cpu:this.raceSettings.cpu,race_difficulty:this.raceSettings.difficulty,race_available:this.raceAvailable,race_course:this.raceSettings.course,gear_rows:this.page === 'gear' ? this.gearRows() : [],race_active:this.raceActive,quad_id:this.quadId,rider_id:this.riderId,player_name:this.playerName,room_code:this.multiplayer?.state?.code ?? (this.page==='create-lobby'?this.createCode:this.roomCode),lobby_status:lobby ? (lobby.public ? `Arlington Free Ride / ${lobby.players.length}/20 riders` : `${lobby.players.length}/4 riders / ${lobby.players.map(p => p.name).join(', ')}`) : '',lobby_started:Boolean(lobby?.started),lobby_host:Boolean(lobby && this.multiplayer.state.peer === lobby.host),lobby_public:Boolean(lobby?.public),page:this.page,loading_status:this.loadingStatus,width:stage.clientWidth,height:stage.clientHeight,selection:this.selection,track_name:track?.name ?? '',track_image:(this.page === 'lobby' && track?.id === 'training' && this.images.has(trainingPreview) ? trainingPreview : track?.image) ?? '',fullscreen:Boolean(document.fullscreenEnabled),
      minimap_orientation:this.minimapSettings?.orientation ?? 'north-up',minimap_shape:this.minimapSettings?.shape ?? 'circle',minimap_corner:this.minimapSettings?.corner ?? 'top-right',minimap_enabled:this.minimapSettings?.enabled ?? true,minimap_size:this.minimapSettings?.size ?? 160,minimap_zoom:this.minimapSettings?.zoom ?? 50,race_overlays_off:[...(this.raceOverlays?.off ?? [])],race_position:this.raceOverlays?.position ?? 'name',funny_crashes:this.crashSettings?.enabled ?? false,crash_mode:this.crashSettings?.mode ?? 'original',original_camera:this.crashSettings?.camera === 'original',chase_assist:this.crashSettings?.chase === 'assisted',flips:this.crashSettings?.flipsEnabled ?? false,alpha_rows:this.page === 'alpha' ? (this.alphaSettings?.rows() ?? []) : [],realistic_train:this.crashSettings?.realisticTrain ?? false,speed_rush:this.crashSettings?.speed ?? 'off',graphics_preset:this.graphicsSettings?.preset ?? 'auto',graphics_resolved:this.graphicsSettings?.resolved ?? '',vibration:this.vibrationSettings?.enabled ?? true,controller_label:this.page === 'controller-settings' ? controllerLabel() : '',controller_preset:this.page === 'controller-settings' ? gameplayBindings.controllerPresetLabel() : '',controller_steering:controllerSettings.steering,controller_lean:controllerSettings.lean,controller_triggers:controllerSettings.triggers,controller_camera:controllerSettings.camera,controller_deadzone:controllerSettings.deadzone,mobile_steering_sensitivity:this.mobileControls?.steeringSensitivity ?? 85,one_handed_mobile:this.mobileControls?.oneHanded ?? true,diagnostics_mode:this.diagnosticsOverlay?.mode ?? 'off',control_rows:this.page === 'bindings' ? gameplayBindings.rows(this.bindingDevice,this.padStandard()).map(row => ({id:row.id,label:row.label,value:row.labelBinding})) : [],
      binding_device:this.bindingDevice,binding_page:this.bindingPage,capture_label:this.capture?.label ?? '',
      music_ready:Boolean(music?.tracks?.length),music_rows:this.page === 'jukebox' ? music.tracks.map(row => ({id:String(row.index),label:row.title,value:row.artist})) : this.page === 'music' ? music.tracks.map(row => ({id:String(row.index),label:row.title,value:`${row.index === musicState.playingIndex && musicState.enabled ? 'Playing / ' : row.index === musicState.selectedIndex ? 'Selected / ' : ''}${row.artist}`})) : [],
      music_page:this.musicPage,music_status:musicState ? `${musicState.enabled ? 'On' : 'Off'} / Volume ${Math.round(musicState.volume * 100)}% / ${musicState.title || 'Choose a song'}` : '',music_enabled:(musicState?.enabled ?? true) && !(this.page === 'jukebox' && musicState?.paused),music_volume:Math.round((musicState?.volume ?? .7)*100),sfx_volume:Math.round((this.audio?.()?.settings?.sfx ?? 1)*100),sound_mode:this.audio?.()?.settings?.mode ?? 'enhanced',silent_mode:this.audio?.()?.audioSession ? (this.audio().settings?.silent ?? 'play') : '',menu_logo:this.images.has('brand-logo'),menu_atv:this.images.has('multi_atv01')};
    let key = JSON.stringify(request);
    if (this.layoutKey === key) return;
    this.layoutPending = true;
    try {
      if (this.page === 'lobby' && track?.id === 'training') {
        request.track_image = await this.trainingMapPreview(); key = JSON.stringify(request);
      } else if (this.page === 'lobby' && track?.image) {
        await this.image(`${track.image}a`); await this.image(`${track.image}b`);
      }
      const frame = await this.json(62,new TextEncoder().encode(key));
      // Page/viewport can change while the worker handles the request. The next
      // frame requests fresh data, rather than applying a stale hit map.
      if ((this.minimapSettings?.zoom ?? 50) !== request.minimap_zoom || (this.minimapSettings?.orientation ?? 'north-up') !== request.minimap_orientation || (this.minimapSettings?.shape ?? 'circle') !== request.minimap_shape || (this.minimapSettings?.corner ?? 'top-right') !== request.minimap_corner || (this.minimapSettings?.enabled ?? true) !== request.minimap_enabled || (this.minimapSettings?.size ?? 160) !== request.minimap_size || this.raceSettings.laps!==request.race_laps || this.raceSettings.course!==request.race_course || JSON.stringify(this.page === 'gear' ? this.gearRows() : [])!==JSON.stringify(request.gear_rows) || this.raceSettings.cpu!==request.race_cpu || this.raceSettings.difficulty!==request.race_difficulty || (this.mobileControls?.steeringSensitivity ?? 85) !== request.mobile_steering_sensitivity || (this.capture?.label ?? '') !== request.capture_label || this.bindingPage !== request.binding_page || this.musicPage !== request.music_page || this.page !== request.page || this.selection !== request.selection || this.quadId !== request.quad_id || this.riderId !== request.rider_id || stage.clientWidth !== request.width || stage.clientHeight !== request.height || this.root.hidden) return;
      if (this.page === 'lobby' && this.multiplayer?.state?.environment !== track?.id) return;
      this.presentation = request; this.presentationTrack = track; this.frame = frame; this.layoutKey = key; this.draw(); this.rebuildButtons(); this.updateModelViews();
    } catch (error) { this.fail(error); }
    finally { this.layoutPending = false; }
  }
  async loadTips() {
    this.tipsPending ??= (async () => {
      await this.archive('/STARTEVT.RES');
      const doc = await this.document('etips.txt');
      return ridingTips.map(id => doc.get(`tip${id}`)).filter(Boolean).map(fields => Object.keys(fields).filter(key => /^line\d+_1$/.test(key))
        .sort((a, b) => parseInt(a.slice(4)) - parseInt(b.slice(4))).map(key => unquote(fields[key])));
    })().catch(() => []);
    return this.tipsPending;
  }
  // The main menu shows how many people are riding Arlington Free Ride.
  watchFreerideRiders(on) {
    clearInterval(this.freerideTimer); this.freerideTimer = null;
    if (!on) return;
    const poll = async () => {
      if (this.page !== 'main' || this.root.hidden || globalThis.document?.hidden) return;
      try {
        const response = await fetch('/multiplayer/freeride/riders');
        const riders = response.ok ? (await response.json()).riders : null;
        this.freerideRiders = Number.isInteger(riders) && riders >= 0 ? riders : null;
      } catch { this.freerideRiders = null; }
      if (this.page === 'main') { this.layoutKey = ''; void this.refresh(); }
    };
    void poll();
    this.freerideTimer = setInterval(poll, 15000); this.freerideTimer?.unref?.();
  }
  async beginLoading(environment) {
    if (!this.initialized) this.setLoading(true, 'Preparing course…');
    this.loadingTrack = this.tracks.find(track => track.id === environment) ?? this.tracks[this.track];
    this.loadingStatus = 'Preparing course…'; this.loadingProgress = 0; this.loadingTip = null; this.stopShots();
    void this.courseShots(this.loadingTrack).then(urls => {
      if (this.page !== 'loading' || !urls.length) return;
      // Original icon_info.ini: frames 02-07 loop over 15 seconds.
      this.loadingShots = urls; this.shotFrame = 0; this.layoutKey = ''; void this.refresh();
      this.shotTimer = setInterval(() => { if (this.page !== 'loading') return this.stopShots(); this.shotFrame++; this.layoutKey = ''; void this.refresh(); }, 2500);
    });
    void this.loadTips().then(tips => { if (this.page !== 'loading' || !tips.length) return; this.loadingTip = tips[Math.floor(Math.random() * tips.length)]; this.layoutKey = ''; void this.refresh(); });
    // Free Ride opens the loading screen before claiming a slot; the ride
    // itself then continues on that screen instead of opening it again.
    if (this.page === 'loading' && !this.root.hidden) { this.layoutKey = ''; await this.refresh(); }
    else await this.show('loading');
    this.busy = true;
    // Yield a frame so the loading UI is painted before decoding/upload work.
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  }
  loadingStage(text) {
    if (this.page !== 'loading' || this.root.hidden) return;
    // Keep decoder details in diagnostics; the player sees the actual loading phase.
    this.loadingStatus = /engine|sound/i.test(text) ? 'Tuning sound…' : /ATV/i.test(text) ? 'Preparing your ride…' : /rider/i.test(text) ? 'Getting your rider ready…' : /texture|sky|model|vegetation/i.test(text) ? 'Building the scenery…' : /collision/i.test(text) ? 'Preparing the trail…' : /incomplete|Static terrain/i.test(text) ? 'Ready to ride' : 'Opening course…';
    // The bar advances with the real loading stages and never moves backwards.
    const stage = {'Opening course…':.12,'Preparing the trail…':.3,'Building the scenery…':.5,'Preparing your ride…':.7,'Getting your rider ready…':.82,'Tuning sound…':.92,'Ready to ride':1}[this.loadingStatus];
    this.loadingProgress = Math.max(this.loadingProgress ?? 0, stage ?? 0);
    this.layoutKey = '';
    this.courseMeter?.setAttribute('aria-label', this.loadingStatus); void this.refresh();
  }
  setLoading(visible, detail = 'Preparing menus…', title = 'Now Loading...') {
    if (!this.loadingScreen) return;
    this.loadingScreen.hidden = !visible;
    this.loadingScreen.setAttribute('aria-busy', String(visible));
    document.getElementById('loading-title').textContent = title;
    document.getElementById('loading-detail').textContent = detail;
  }
  captureTransition(canvas = this.canvas) {
    for (const animation of this.transitions) animation.cancel();
    this.transitions = []; releaseCanvas(this.transitionGhost); this.transitionGhost = null;
    if (this.isPausePanel() || !canvas || !this.frame || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const ghost = document.createElement('canvas'), rect = canvas.getBoundingClientRect();
    ghost.width = canvas.width; ghost.height = canvas.height; ghost.getContext('2d').drawImage(canvas, 0, 0);
    ghost.className = 'menu-transition'; ghost.setAttribute('aria-hidden', 'true');
    Object.assign(ghost.style, {left:`${rect.left}px`,top:`${rect.top}px`,width:`${rect.width}px`,height:`${rect.height}px`});
    document.body.append(ghost); this.transitionGhost = ghost;
  }
  animatePage(leaving = false) {
    if (leaving) this.captureTransition();
    const ghost = this.transitionGhost;
    if (ghost) {
      const exit = ghost.animate([{opacity:1,transform:'translateX(0)'},{opacity:0,transform:'translateX(-28px)'}], {duration:180,easing:'ease-in',fill:'forwards'});
      this.transitions.push(exit); exit.finished.then(() => releaseCanvas(ghost), () => releaseCanvas(ghost));
      this.transitionGhost = null;
    }
    if (leaving || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const stage = this.canvas.parentElement;
    const entry = stage.animate([{opacity:0,transform:'translateX(36px)'},{opacity:1,transform:'translateX(0)'}], {duration:240,easing:'cubic-bezier(.16,1,.3,1)'});
    this.transitions.push(entry);
  }
  clearModelViews() {
    cancelAnimationFrame(this.modelAnimation);
    this.modelToken++; this.modelKey = '';
    for (const view of this.modelViews.values()) view.destroy();
    this.modelViews.clear();
  }
  updateModelViews() {
    if (!MODEL_PAGES.includes(this.page)) return;
    // HD screens provide named slots; the views move with responsive layout.
    const slots = [...this.buttons.querySelectorAll('[data-model-slot]')];
    if (!slots.length) { this.clearModelViews(); return; }
    let created = false;
    for (const slot of slots) {
      let view = this.modelViews.get(slot.dataset.modelSlot);
      if (!view) { view = new ModelView(slot, slot.dataset.modelSlot); this.modelViews.set(slot.dataset.modelSlot, view); created = true; }
      else if (view.element.parentElement !== slot) { slot.append(view.element); view.render(); }
    }
    const appearance = normalizedAppearance(this.appearance), key = `${this.quadId}:${this.riderId}:${appearanceBytes(appearance).join('.')}`;
    // A newly added view (e.g. the rider on entering the Garage) still needs the models.
    if (key === this.modelKey && !created) return;
    cancelAnimationFrame(this.modelAnimation);
    this.modelKey = key; const token = ++this.modelToken;
    for (const view of this.modelViews.values()) {
      view.clear('Loading model...');
      delete view.canvas.dataset.clip; delete view.canvas.dataset.animationTime;
    }
    void this.modelAssets(this.quadId, this.riderId).then(async source => {
      const gear = this.quadId < 27 ? await this.gearPreview?.(appearance, source.riderMesh) : null;
      if (token !== this.modelToken || !MODEL_PAGES.includes(this.page) || this.root.hidden) return;
      this.modelViews.get('quad')?.set(source.mesh, source.textures, undefined, { paint: this.quadId < 27 ? appearance.paint : 0 });
      this.vehicleFigures = { quadId: this.quadId, ...source.stats }; void this.refreshStatsLogo();
      this.modelViews.get('rider')?.set(source.riderMesh, gear?.size ? new Map([...source.riderTextures, ...gear]) : source.riderTextures, source.riderPose?.world_matrices);
      if (['garage','gear'].includes(this.page)) void this.startMenuRider(token, key);
      for (const view of this.modelViews.values()) view.canvas.dataset.selection = key;
    }).catch(error => {
      if (token !== this.modelToken) return;
      for (const view of this.modelViews.values()) view.clear('Model unavailable');
      this.notice.textContent = `Models: ${error.message}`;
    });
  }
  async startMenuRider(token, key) {
    try {
      const original = await this.menuRider(this.riderId, () => token === this.modelToken, this.quadId);
      const gear = original && this.quadId < 27 ? await this.gearPreview?.(normalizedAppearance(this.appearance), original.mesh) : null;
      if (!original || token !== this.modelToken || !['garage','gear'].includes(this.page)) return;
      const view = this.modelViews.get('rider');
      view.yaw = original.previewYaw ?? -.6;
      view.set(original.mesh,gear?.size ? new Map([...original.textures, ...gear]) : original.textures,original.pose.world_matrices);
      view.canvas.dataset.selection = key; view.canvas.dataset.clip = original.clip;
      const start = performance.now(); let busy = false, last = 0;
      const tick = async time => {
        if (token !== this.modelToken) return;
        this.modelAnimation = requestAnimationFrame(tick);
        // A kept view is briefly detached while its page is rebuilt.
        if (!view.element.isConnected || document.hidden || !document.hasFocus() || busy || time-last < 33) return;
        busy = true; last = time;
        try {
          const data = new Float32Array([Math.max(0,(time-start)/1000)]);
          const result = await this.json(93,new Uint8Array(data.buffer));
          if (token !== this.modelToken) return;
          view.pose = result.pose.world_matrices; view.canvas.dataset.clip = result.clip;
          view.canvas.dataset.animationTime = String(data[0]); view.render();
        } catch (error) { if(token === this.modelToken) this.notice.textContent = `Rider animation: ${error.message}`; }
        finally { busy = false; }
      };
      cancelAnimationFrame(this.modelAnimation); this.modelAnimation = requestAnimationFrame(tick);
    } catch (error) { if(token === this.modelToken) this.notice.textContent = `Rider animation: ${error.message}`; }
  }
  rebuildButtons() {
    this.pausePanel.hidden = !this.isPausePanel();
    this.root.dataset.pausePanel = String(this.isPausePanel());
    this.root.dataset.context = this.menuSettings() ? 'menu' : '';
    if (this.backdropUrl) this.root.style.setProperty('--ms-backdrop', `url("${this.backdropUrl}")`);
    this.root.dataset.page = this.page;
    if (this.page !== 'jukebox') stopJukebox(this);
    if (this.isPausePanel()) { if (this.page === 'jukebox') renderJukebox(this); else this.rebuildPausePanel(); return; }
    this.courseMeter?.remove(); this.courseMeter = null;
    if (screenPages.has(this.page)) {
      renderScreen(this);
      Object.assign(this.root.dataset, {selection:String(this.selection), track:this.tracks[this.track]?.id ?? '', raceCourse:this.raceSettings.course, renderer:'rust', musicPage:String(this.musicPage), bindingPage:String(this.bindingPage)});
      return;
    }
    delete this.buttons.dataset.structure;
    const focusedAction = this.buttons.contains(document.activeElement) ? document.activeElement.dataset.action : null;
    // Preserve native range nodes during input/redraw so dragging and keyboard
    // focus survive every Rust label update.
    for (const element of [...this.buttons.children]) if (element.tagName !== 'INPUT' || ![...this.frame.sliders, ...(this.frame.inputs ?? [])].some(item => item.action === element.dataset.setting)) element.remove();
    for (const hit of this.frame.hits) {
      const b = document.createElement('button'), [x,y,w,h] = hit.rect;
      b.dataset.action = hit.action; b.type = 'button'; b.textContent = hit.label; b.setAttribute('aria-label',hit.label);
      b.style.left = `${x/this.frame.width*100}%`; b.style.top = `${y/this.frame.height*100}%`;
      b.style.width = `${w/this.frame.width*100}%`; b.style.height = `${h/this.frame.height*100}%`;
      b.setAttribute('aria-current',String(hit.selected));
      b.onclick = () => { const index = this.frame.choices.indexOf(hit.action); if (index >= 0) this.selection = index; void this.run(hit.action); };
      this.buttons.append(b);
    }
    for (const slider of this.frame.sliders) {
      let input = this.buttons.querySelector(`input[data-setting="${slider.action}"]`);
      if (!input) {
        input = document.createElement('input'); input.type = 'range'; input.className = 'atv-range'; input.dataset.setting = slider.action;
        input.setAttribute('aria-label', slider.label);
        input.addEventListener('input', () => {
          if (input.dataset.setting === 'minimap-size') this.minimapSettings?.setSize(Number(input.value));
          else if (input.dataset.setting === 'minimap-zoom') this.minimapSettings?.setZoom(Number(input.value));
          else this.mobileControls?.setSteeringSensitivity(Number(input.value));
          input.setAttribute('aria-valuetext', `${input.value}${input.dataset.setting === 'minimap-size' ? ' pixels' : '%'}`);
          input.style.setProperty('--atv-range-fill', `${(Number(input.value) - Number(input.min)) / (Number(input.max) - Number(input.min)) * 100}%`);
          this.layoutKey = ''; void this.refresh();
        });
        this.buttons.append(input);
      }
      const [x,y,w,h] = slider.rect;
      Object.assign(input.style, { position: 'absolute', left: `${x/this.frame.width*100}%`, top: `${y/this.frame.height*100}%`, width: `${w/this.frame.width*100}%`, height: `${h/this.frame.height*100}%` });
      input.min = slider.min; input.max = slider.max; input.step = slider.step; input.value = slider.value;
      input.setAttribute('aria-valuetext', `${slider.value}${slider.action === 'minimap-size' ? ' pixels' : '%'}`);
      input.style.setProperty('--atv-range-fill', `${(slider.value-slider.min)/(slider.max-slider.min)*100}%`);
      input.dataset.selected = String(slider.selected);
    }
    for (const field of this.frame.inputs ?? []) {
      let input = this.buttons.querySelector(`input[data-setting="${field.action}"]`);
      if (!input) {
        input = document.createElement('input'); input.type = 'text'; input.className = 'atv-text-input'; input.dataset.setting = field.action;
        input.autocomplete = 'off'; input.spellcheck = false; input.value = field.value;
        input.addEventListener('input', () => {
          if (field.action === 'player-name') this.playerName = input.value;
          else if (field.action === 'create-code') this.createCode = input.value.toUpperCase().trim();
          else this.roomCode = input.value.toUpperCase().trim();
          this.layoutKey = '';
        });
        this.buttons.append(input);
      }
      const [x,y,w,h] = field.rect;
      Object.assign(input.style, {position:'absolute',left:`${x/this.frame.width*100}%`,top:`${y/this.frame.height*100}%`,width:`${w/this.frame.width*100}%`,height:`${h/this.frame.height*100}%`});
      input.maxLength = field.max_length; input.setAttribute('aria-label', field.label);
    }
    if (focusedAction) [...this.buttons.children].find(element => element.dataset.action === focusedAction)?.focus({preventScroll:true});
    this.courseMeter?.remove(); this.courseMeter = null;
    if (this.page === 'loading') {
      const preview = this.frame.draw.find(command => command.kind === 'preview');
      if (preview) {
        const [x,y,w,h] = preview.rect, meter = document.createElement('div');
        meter.className = 'loading-meter course-load-meter'; meter.setAttribute('role', 'status');
        meter.setAttribute('aria-label', this.loadingStatus);
        Object.assign(meter.style, {left:`${x/this.frame.width*100}%`,top:`${(y+h+100)/this.frame.height*100}%`,width:`${w/this.frame.width*100}%`});
        this.canvas.parentElement.append(meter); this.courseMeter = meter;
      }
    }
    this.root.dataset.page = this.page; this.root.dataset.selection = String(this.selection);
    this.root.dataset.track = this.tracks[this.track]?.id ?? '';
    this.root.dataset.renderer = 'rust';
    this.root.dataset.musicPage = String(this.musicPage);
    this.root.dataset.bindingPage = String(this.bindingPage);
  }
  isPausePanel() {
    return overlayPages.has(this.page) || (this.page === 'lobby' && Boolean(this.multiplayer?.state?.lobby?.started));
  }
  rebuildPausePanel() {
    // Rust owns supported actions, labels and values. DOM owns responsive layout.
    // Keep the same nodes for state updates so slider drags and focus survive.
    const state = this.presentation;
    const listPage = ['bindings','music'].includes(this.page);
    const players = state.lobby_players;
    const padPage = this.page === 'bindings' && this.bindingDevice === 'controller', standard = padPage && this.padStandard();
    const structure = JSON.stringify([this.page, this.frame.choices, Boolean(this.capture), this.bindingDevice, standard, ['players','lobby'].includes(this.page) ? players : []]);
    const changed = structure !== this.panelStructure;
    const element = (tag, className, text) => {
      const node = document.createElement(tag); node.className = className;
      if (text !== undefined) node.textContent = text;
      return node;
    };
    const selectedAction = this.frame.choices[this.selection];
    if (changed) {
      this.panelStructure = structure;
      this.buttons.replaceChildren(); this.courseMeter?.remove(); this.courseMeter = null;
      const header = element('header', 'pause-header');
      const title = element('h1',''); title.id = 'pause-title';
      header.append(title, element('p','pause-status'));
      const body = element('div', this.page === 'pause' ? 'pause-groups' : 'pause-content');
      const section = (className = '') => {
        const group = element('section', `pause-group ${className}`);
        body.append(group); return group;
      };
      const focus = (node, action) => {
        node.onfocus = () => {
          const index = this.frame.choices.indexOf(action);
          if (this.restoringPauseFocus || index < 0 || this.selection === index) return;
          this.selection = index;
          if (this.page === 'bindings' && action.startsWith('bind:')) this.bindingRow = this.bindingPage * 4 + index;
          this.layoutKey = '';
        };
      };
      const addButton = (parent, action) => {
        const hit = this.frame.hits.find(hit => hit.action === action); if (!hit) return;
        const button = element('button',''); button.type = 'button'; button.dataset.action = action;
        const copy = element('span','pause-label'); copy.append(element('span','pause-label-text'), element('small','pause-detail'));
        button.append(copy, element('span','pause-value'));
        button.onclick = () => { this.selection = this.frame.choices.indexOf(action); void this.run(action); };
        // Page and Back controls carry a direction glyph so they read as navigation.
        const glyph = /prev$/.test(action) || action === 'back' ? 'left' : /next$/.test(action) ? 'right' : '';
        if (glyph) { const arrow = element('span',`pause-icon pause-icon-${glyph}`); arrow.setAttribute('aria-hidden','true'); if (glyph === 'left') button.prepend(arrow); else button.append(arrow); }
        focus(button, action); parent.append(button);
      };
      const addSlider = (parent, slider) => {
        const label = element('label','pause-slider');
        const heading = element('span','pause-slider-heading');
        heading.append(element('span','',slider.label), element('output','pause-value'));
        const input = element('input','atv-range'); input.type = 'range'; input.dataset.setting = slider.action;
        input.min = slider.min; input.max = slider.max; input.step = slider.step;
        input.setAttribute('aria-label',slider.label); focus(input,slider.action);
        input.oninput = () => {
          const value = Number(input.value);
          if (slider.action === 'minimap-size') this.minimapSettings?.setSize(value);
          else if (slider.action === 'minimap-zoom') this.minimapSettings?.setZoom(value);
          else if (slider.action.startsWith('pad-')) controllerSettings.set(slider.action.slice(4), value);
          else this.mobileControls?.setSteeringSensitivity(value);
          this.layoutKey = ''; void this.refresh();
        };
        label.append(heading,input); parent.append(label);
      };
      if (this.page === 'pause') {
        const groups = [['ride',['resume','reset','players','changeride','hosttrack']],['preferences',['settings','jukebox']],['session',['copyride','devlog','main','leave']]];
        this.pauseOrder = [];
        for (const [name,actions] of groups) {
          const group = section(`pause-${name}`);
          for (const action of actions) if (this.frame.choices.includes(action)) { this.pauseOrder.push(action); addButton(group,action); }
        }
      } else {
        if (this.page === 'controls') {
          const tips = section('pause-tips');
          const lines = this.mobileControls?.oneHanded ? ['Hold anywhere to accelerate and brace for a jump.','Drag sideways to steer, and up or down to lean. Lift to coast and release preload.','Use a second finger to brake / reverse on the ground, or swipe / circle for tricks in the air.'] : ['Left thumb: steer and lean.','Right thumb: hold to accelerate and brace; drag back to brake / reverse. Lift to release preload.','In the air, swipe or circle with your right thumb for tricks.'];
          for (const line of lines) tips.append(element('p','',line));
        }
        if (this.page === 'players') {
          const roster = section();
          for (const player of players) if (player.value !== 'ready') {
            const row = element('div','pause-player'); row.append(element('span','',player.label),element('span','pause-value',player.value === 'self' ? 'You' : 'Connecting')); roster.append(row);
          }
        }
        if (this.page === 'lobby') {
          const details = section(); details.append(element('p','pause-room',state.room_code),element('p','pause-status',state.track_name),element('p','pause-status',state.lobby_status));
        }
        if (this.capture) {
          const capture = section('pause-capture');
          capture.append(element('p','pause-capture-label',this.capture.label),element('p','','Press a key, controller button or move an axis. Hold several together for a combination. Release to save; Escape cancels.'));
        }
        const settings = section();
        const footer = element('footer','pause-footer');
        const paging = element('div','pause-pagination');
        for (const action of this.frame.choices) {
          const slider = this.frame.sliders.find(item => item.action === action);
          if (slider) { addSlider(settings,slider); continue; }
          if (['bindprev','bindnext','musicprev','musicnext'].includes(action)) addButton(paging,action);
          else if (['back','pause','bindcancel','bindclear','bindreset','paddefaults','musictoggle','musicvolume','sfxvolume'].includes(action)) addButton(footer,action);
          else addButton(settings,action);
        }
        // Previous / Page n of m / Next on one row.
        if (listPage && !this.capture) { paging.firstChild?.after(element('span','pause-page-count')); body.append(paging); }
        if (footer.childElementCount) body.append(footer);
      }
      // Controller bindings add the original Controller Settings summary panel.
      this.padSummary = padPage && !this.capture ? element('div','pad-summary-slot') : null; this.padSummaryKey = '';
      this.pausePanel.replaceChildren(header,body,...(this.padSummary ? [this.padSummary] : []));
    }
    this.pausePanel.dataset.screen = this.page;
    this.pausePanel.querySelector('h1').textContent = ({settings:'Settings',extras:'Extras',alpha:'Alpha',pause:'Pause',controls:'Controls','controller-settings':'Controller Settings','hud-settings':'Display','minimap-config':'Minimap',bindings:this.bindingDevice === 'controller' ? 'Controller' : 'Keyboard',music:'Audio',players:'Players',lobby:'Lobby'})[this.page];
    // Only live state appears under the title (the soundtrack's on/volume/song).
    this.pausePanel.querySelector('.pause-header .pause-status').textContent = this.page === 'music' ? state.music_status : '';
    const padRows = padPage ? gameplayBindings.rows('controller', standard) : null;
    if (this.padSummary) {
      const key = JSON.stringify([standard, padRows.map(row => row.binding)]);
      if (key !== this.padSummaryKey) { this.padSummaryKey = key; this.padSummary.replaceChildren(controllerSummary(padRows, standard)); }
    }
    for (const button of this.pausePanel.querySelectorAll('button')) {
      const action = button.dataset.action, hit = this.frame.hits.find(hit => hit.action === action);
      if (!hit) continue;
      const [label,...values] = hit.label.split(': '), value = values.join(': ');
      const detail = listPage && /^(bind|song):/.test(action);
      button.setAttribute('aria-label',hit.label); button.setAttribute('aria-current',String(action === selectedAction));
      button.querySelector('.pause-label-text').textContent = label;
      const padRow = padRows && action.startsWith('bind:') ? padRows.find(row => row.id === action.slice(5)) : null;
      if (padRow) {
        // Glyphs for the controller; the text value stays in the aria-label.
        const detailNode = button.querySelector('.pause-detail'), key = JSON.stringify([standard, padRow.binding]);
        if (detailNode.dataset.glyphs !== key) { detailNode.dataset.glyphs = key; detailNode.replaceChildren(bindingGlyphs(padRow.binding, standard)); }
      } else button.querySelector('.pause-detail').textContent = detail ? value : '';
      const badge = button.querySelector('.pause-value'); badge.textContent = detail ? '' : value; badge.hidden = detail || !value;
      if (action.startsWith('song:')) button.dataset.playing = String(Number(action.slice(5)) === this.audio?.()?.music?.snapshot().playingIndex && state.music_enabled);
    }
    for (const slider of this.frame.sliders) {
      const input = this.pausePanel.querySelector(`input[data-setting="${slider.action}"]`); if (!input) continue;
      input.value = slider.value; input.dataset.selected = String(slider.action === selectedAction);
      input.setAttribute('aria-valuetext',`${slider.value}${slider.action === 'minimap-size' ? ' pixels' : '%'}`);
      input.style.setProperty('--atv-range-fill',`${(slider.value-slider.min)/(slider.max-slider.min)*100}%`);
      input.parentElement.querySelector('output').textContent = `${slider.value}${slider.action === 'minimap-size' ? ' px' : '%'}`;
    }
    const count = this.pausePanel.querySelector('.pause-page-count');
    if (count) count.textContent = `Page ${(this.page === 'music' ? this.musicPage : this.bindingPage)+1} of ${Math.max(1,Math.ceil((this.page === 'music' ? state.music_rows : state.control_rows).length/4))}`;
    this.restoringPauseFocus = true;
    if (this.resetPauseScroll || changed) this.pausePanel.scrollTop = 0;
    if (this.scrollPauseSelection || this.resetPauseScroll || changed) {
      const selected = this.pausePanel.querySelector(`[data-action="${selectedAction}"], input[data-setting="${selectedAction}"]`);
      selected?.focus({preventScroll:true});
      if (this.scrollPauseSelection) selected?.scrollIntoView({block:'nearest'});
    }
    this.resetPauseScroll = false; this.scrollPauseSelection = false; this.restoringPauseFocus = false;
    this.root.dataset.selection = String(this.selection); this.root.dataset.renderer = 'rust';
    this.root.dataset.musicPage = String(this.musicPage); this.root.dataset.bindingPage = String(this.bindingPage);
  }
  text(text, x, y, name = fontName, color = '200,200,200', alpha = 255, justify = '') {
    const font = this.fonts.get(name.toLowerCase()); if (!font) return;
    let atlas = font.tints.get(color);
    if (!atlas) {
      atlas = document.createElement('canvas'); atlas.width = font.image.width; atlas.height = font.image.height;
      const c = atlas.getContext('2d'); c.drawImage(font.image, 0, 0); const pixels = c.getImageData(0, 0, atlas.width, atlas.height), tint = color.split(',').map(Number);
      for (let i = 0; i < pixels.data.length; i += 4) for (let channel = 0; channel < 3; channel++) pixels.data[i + channel] = pixels.data[i + channel] * tint[channel] / 255;
      c.putImageData(pixels, 0, 0); font.tints.set(color, atlas);
    }
    const glyphs = [...text].map(c => font.glyphs.get(c.charCodeAt(0))).filter(Boolean);
    const advance = g => g.metrics ? g.metrics[0] + g.metrics[2] : (g.uv[2] - g.uv[0]) * atlas.width;
    const width = glyphs.reduce((a, g) => a + advance(g), 0);
    if (justify.toLowerCase().includes('center')) x -= width / 2;
    if (justify.toLowerCase() === 'right') x -= width;
    this.ctx.globalAlpha = alpha / 255;
    for (const g of glyphs) {
      const [l,t,r,b] = g.uv, m = g.metrics, sx = Math.round(l * (atlas.width - 1)), sy = Math.round(t * (atlas.height - 1));
      const w = m?.[1] ?? (r-l)*atlas.width, h = m?.[3] ?? (b-t)*atlas.height;
      this.ctx.drawImage(atlas, sx, sy, w, h, x, y, w, h); x += advance(g);
    }
    this.ctx.globalAlpha = 1;
  }
  draw() {
    const frame = this.frame; if (!frame) return;
    if (this.isPausePanel()) { this.ctx.clearRect(0,0,this.canvas.width,this.canvas.height); if(this.backgroundCanvas)this.backgroundCanvas.hidden=true; return; }
    const c = this.ctx, stage = this.canvas.parentElement, dpr = Math.min(devicePixelRatio || 1,3, Math.sqrt(8_000_000 / Math.max(1,stage.clientWidth * stage.clientHeight)));
    this.canvas.width = Math.round(stage.clientWidth*dpr); this.canvas.height = Math.round(stage.clientHeight*dpr);
    if (screenPages.has(this.page)) { if (this.backgroundCanvas) this.backgroundCanvas.hidden = true; return; }
    c.setTransform(this.canvas.width/frame.width,0,0,this.canvas.height/frame.height,0,0);
    const fullBleed = document.documentElement.classList.contains('app-standalone') && !['pause','hud-settings','minimap-config','controls','controller-settings','bindings','music'].includes(this.page) && this.backgroundCanvas;
    let decoration = true;
    if (this.backgroundCanvas) {
      this.backgroundCanvas.hidden = !fullBleed;
      if (fullBleed) {
        this.backgroundCanvas.width = Math.round(this.root.clientWidth*dpr); this.backgroundCanvas.height = Math.round(this.root.clientHeight*dpr);
        this.backgroundCanvas.getContext('2d').setTransform(this.backgroundCanvas.width/frame.width,0,0,this.backgroundCanvas.height/frame.height,0,0);
      }
    }
    for (const command of frame.draw) {
      if (['text','artwork','preview','model_preview'].includes(command.kind)) decoration = false;
      if (command.kind === 'model_preview') continue;
      if (command.kind === 'text') {
        const heading=command.font.includes('italic');
        const size=(heading ? 32 : 24)*command.scale;
        c.save(); c.font=`${heading ? 'italic ' : ''}800 ${size}px "Arial Narrow", "Helvetica Neue", Arial, sans-serif`;
        c.textBaseline='top'; c.fillStyle=`rgb(${command.color})`;
        c.shadowColor='#0009'; c.shadowBlur=2; c.shadowOffsetY=1;
        c.fillText(command.text,...command.at,command.max_width); c.restore();
      } else if (command.kind === 'artwork') {
        const image = this.images.get(command.image); if (image) c.drawImage(image,...command.rect);
      } else if (command.kind === 'preview') {
        const screenshot = this.images.get(command.image);
        if (screenshot) { c.drawImage(screenshot,...command.rect); continue; }
        const a=this.images.get(`${command.image}a`), b=this.images.get(`${command.image}b`);
        if (!a || !b) continue;
        const [x,y,w,h]=command.rect, total=a.width+b.width;
        c.drawImage(a,x,y,w*a.width/total,h); c.drawImage(b,x+w*a.width/total,y,w*b.width/total,h);
      } else {
        const paintContext = fullBleed && decoration ? this.backgroundCanvas.getContext('2d') : c;
        const path = command.kind === 'path' ? new Path2D(command.d) : new Path2D();
        if (command.kind === 'rect') path.roundRect(...command.rect,command.radius);
        const y = command.rect?.[1] ?? 0, height = command.rect?.[3] ?? frame.height;
        const paint=paintContext.createLinearGradient(0,y,0,y+height); paint.addColorStop(0,command.color); paint.addColorStop(1,command.bottom);
        paintContext.fillStyle=paint;paintContext.fill(path);
        if (command.stroke) {paintContext.strokeStyle=command.stroke;paintContext.lineWidth=1.5;paintContext.stroke(path);}
      }
    }
  }
}

// Isolated inspection views use decoded original bind-pose meshes/materials.
// They never initialize or step the gameplay animator/physics worker state.
// Drop a discarded canvas's backing store now rather than at garbage
// collection; iOS Safari limits total canvas memory.
function releaseCanvas(canvas) {
  if (!canvas) return;
  canvas.remove(); canvas.width = canvas.height = 0;
}

class ModelView {
  constructor(stage, kind) {
    this.element = document.createElement('div'); this.element.className = 'model-view';
    this.canvas = document.createElement('canvas'); this.canvas.setAttribute('aria-label', `${kind === 'quad' ? 'ATV' : 'Rider'} model; drag to rotate`);
    this.canvas.tabIndex = 0; this.message = document.createElement('span'); this.message.setAttribute('role', 'status');
    this.element.append(this.canvas, this.message); stage.append(this.element);
    this.resize = new ResizeObserver(() => this.render()); this.resize.observe(this.element);
    this.gl = this.canvas.getContext('webgl2', { alpha: true, antialias: true });
    this.yaw = -.6; this.groups = []; this.textures = []; this.kind = kind;
    this.canvas.addEventListener('pointerdown', event => { this.pointer = {id:event.pointerId, x:event.clientX}; this.canvas.setPointerCapture(event.pointerId); });
    this.canvas.addEventListener('pointermove', event => { if (this.pointer?.id !== event.pointerId) return; this.yaw += (event.clientX - this.pointer.x) * .015; this.pointer.x = event.clientX; this.render(); });
    for (const type of ['pointerup','pointercancel','lostpointercapture']) this.canvas.addEventListener(type, () => { this.pointer = null; });
    this.canvas.addEventListener('keydown', event => { if (!['ArrowLeft','ArrowRight'].includes(event.code)) return; event.preventDefault(); event.stopPropagation(); this.yaw += event.code === 'ArrowLeft' ? -.25 : .25; this.render(); });
    this.canvas.addEventListener('webglcontextlost', event => { event.preventDefault(); this.lost = true; delete this.canvas.dataset.ready; this.message.textContent = 'Restoring model…'; this.element.classList.add('is-loading'); });
    this.canvas.addEventListener('webglcontextrestored', () => { this.lost = false; this.program = null; this.groups = []; this.textures = []; if (this.source) this.set(...this.source); });
  }
  clear(message = '') {
    const gl = this.gl;
    if (gl && !this.lost) {
      for (const group of this.groups) { gl.deleteVertexArray(group.vao); gl.deleteBuffer(group.buffer); }
      for (const texture of this.textures) gl.deleteTexture(texture);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    }
    this.groups = []; this.textures = []; this.source = null; this.message.textContent = message; this.element.classList.toggle('is-loading', /Loading|Restoring/.test(message)); delete this.canvas.dataset.ready;
  }
  set(mesh, materials, pose = mesh.nodes.map(node => node.world_matrix), { paint = 0 } = {}) {
    pose ??= mesh.nodes.map(node => node.world_matrix);
    this.clear(); this.pose = pose; this.source = [mesh, materials, pose, { paint }]; this.paint = PAINT_PALETTE[paint] ?? PAINT_PALETTE[0]; const gl = this.gl;
    if (!gl || this.lost) { this.message.textContent = '3D preview unavailable'; return; }
    if (!this.program) {
      const shader = (type, text) => { const s=gl.createShader(type); gl.shaderSource(s,text); gl.compileShader(s); if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s)); return s; };
      const vertex=shader(gl.VERTEX_SHADER, `#version 300 es
        layout(location=0) in vec3 p; layout(location=1) in vec3 n; layout(location=2) in vec2 uv; layout(location=3) in float node; uniform mat4 bones[32];
        uniform vec3 center; uniform vec2 extent; uniform float yaw; out vec2 tex; out float light;
        void main(){float c=cos(yaw),s=sin(yaw);mat3 r=mat3(c,0.,-s,0.,1.,0.,s,0.,c);vec3 v=r*((bones[int(node)]*vec4(p,1.)).xyz-center);
        float pitch=.25;float y=v.y*cos(pitch)-v.z*sin(pitch);float z=v.y*sin(pitch)+v.z*cos(pitch);
        gl_Position=vec4(-v.x/extent.x,y/extent.y,-z/40.,1.);tex=uv;vec3 nn=normalize(r*mat3(bones[int(node)])*n);light=.36+.7*max(dot(nn,normalize(vec3(.4,.8,.6))),0.)+.16*max(dot(nn,normalize(vec3(-.7,.1,-.7))),0.);}`);
      const fragment=shader(gl.FRAGMENT_SHADER, `#version 300 es
        precision highp float; in vec2 tex; in float light; uniform sampler2D image; uniform bool cutout; uniform vec2 paint; out vec4 color;
        ${paintGlsl}
        void main(){vec4 t=texture(image,tex);if(cutout&&t.a<.15)discard;vec3 c=applyPaint(t.rgb,paint)*light;float l=dot(c,vec3(.299,.587,.114));
        // Studio grade: richer colour and a little contrast so pale suits read on the silver stage.
        c=clamp(mix(vec3(l),c,1.18),0.,1.);c=clamp((c-.5)*1.1+.47,0.,1.);color=vec4(c,1.);}`);
      this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);
      gl.deleteShader(vertex);gl.deleteShader(fragment);if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(this.program));
      this.uniforms=Object.fromEntries(['center','extent','yaw','image','cutout','paint','bones[0]'].map(n=>[n,gl.getUniformLocation(this.program,n)]));
    }
    const gpu=new Map();
    const texture = decoded => {
      const t=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,t);gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,decoded.width,decoded.height,0,gl.RGBA,gl.UNSIGNED_BYTE,decoded.rgba);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.REPEAT);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.REPEAT);this.textures.push(t);return t;
    };
    for(const [name,decoded] of materials) if(decoded.rgba) gpu.set(name,texture(decoded));
    const fallback=texture({width:1,height:1,rgba:new Uint8Array([185,185,185,255])});
    const min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];
    for(const group of mesh.lods[0].groups) {
      const values=new Float32Array(group.indices.length*9);
      group.indices.forEach((index,i)=>{const v=group.vertices[index];values.set([...v.local_position,...localNormal(mesh.nodes[v.node].world_matrix,v.normal),...v.uv,v.node],i*9);const matrix=pose[v.node]; const position=[0,1,2].map(a=>matrix[a]*v.local_position[0]+matrix[a+4]*v.local_position[1]+matrix[a+8]*v.local_position[2]+matrix[a+12]);position.forEach((p,a)=>{min[a]=Math.min(min[a],p);max[a]=Math.max(max[a],p);});});
      const vao=gl.createVertexArray(),buffer=gl.createBuffer();gl.bindVertexArray(vao);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferData(gl.ARRAY_BUFFER,values,gl.STATIC_DRAW);
      for(const [i,size,offset] of [[0,3,0],[1,3,12],[2,2,24],[3,1,32]]){gl.enableVertexAttribArray(i);gl.vertexAttribPointer(i,size,gl.FLOAT,false,36,offset);}
      // Original UI rider textures store a specular/reflection mask in alpha;
      // only materials without that flag use alpha as a cut-out.
      const decoded=materials.get(mesh.materials[group.material]);
      this.groups.push({vao,buffer,count:group.indices.length,texture:gpu.get(mesh.materials[group.material])??fallback,cutout:!decoded?.opaque,paintable:paintable(mesh.materials[group.material])});
    }
    this.center=min.map((v,i)=>(v+max[i])/2); this.dimensions=min.map((v,i)=>max[i]-v);
    this.canvas.dataset.ready='true';this.canvas.dataset.triangles=String(this.groups.reduce((n,g)=>n+g.count/3,0));this.render();
  }
  render() {
    const gl=this.gl;if(!gl||this.lost||!this.groups.length||!this.element.isConnected)return;
    const box=this.element.getBoundingClientRect(),dpr=Math.min(devicePixelRatio||1,2);
    this.canvas.width=Math.max(1,Math.round(box.width*dpr));this.canvas.height=Math.max(1,Math.round(box.height*dpr));
    gl.viewport(0,0,this.canvas.width,this.canvas.height);gl.clearColor(0,0,0,0);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);gl.enable(gl.DEPTH_TEST);gl.useProgram(this.program);
    // Fit height and horizontal turn radius separately so tall phone cards
    // show the rider at useful scale while retaining the full model when rotated.
    const horizontal=Math.hypot(this.dimensions[0],this.dimensions[2])/2,aspect=this.canvas.width/this.canvas.height;
    const vertical=(this.dimensions[1]*Math.cos(.25)/2+horizontal*Math.sin(.25));
    const extent=Math.max(vertical,horizontal/aspect)*1.12;
    gl.uniformMatrix4fv(this.uniforms['bones[0]'],false,new Float32Array(this.pose.flat()));
    gl.uniform3fv(this.uniforms.center,this.center);gl.uniform2f(this.uniforms.extent,extent*aspect,extent);gl.uniform1f(this.uniforms.yaw,-this.yaw);gl.uniform1i(this.uniforms.image,0);gl.activeTexture(gl.TEXTURE0);
    for(const group of this.groups){gl.bindVertexArray(group.vao);gl.bindTexture(gl.TEXTURE_2D,group.texture);gl.uniform1i(this.uniforms.cutout,group.cutout);gl.uniform2fv(this.uniforms.paint,group.paintable?this.paint:PAINT_PALETTE[0]);gl.drawArrays(gl.TRIANGLES,0,group.count);}
    this.canvas.dataset.glError=String(gl.getError());
  }
  destroy(){this.resize.disconnect();this.clear();if(this.program&&!this.lost)this.gl.deleteProgram(this.program);this.gl?.getExtension('WEBGL_lose_context')?.loseContext();releaseCanvas(this.canvas);this.element.remove();}
}

// Course photographs carry black/transparent mats from their 640x448 page
// placement, including a diagonal wedge where the original red panel curve
// covered the photo. Trim dark edge rows/columns and the wedge's widest extent.
function trimDark(canvas) {
  const {width, height} = canvas, data = canvas.getContext('2d').getImageData(0, 0, width, height).data;
  const lit = o => data[o + 3] > 16 && data[o] + data[o + 1] + data[o + 2] > 54;
  const dark = (x0, y0, dx, dy, count) => {
    let count2 = 0;
    for (let i = 0; i < count; i++) if (lit(((y0 + dy * i) * width + x0 + dx * i) * 4)) count2++;
    return count2 < count * .04;
  };
  // First lit pixel of each row; the 90th percentile ignores stray dark pixels.
  const starts = [];
  for (let y = 0; y < height; y++) { let x = 0; while (x < width / 4 && !lit((y * width + x) * 4)) x++; starts.push(x); }
  starts.sort((a, b) => a - b);
  let left = Math.min(Math.floor(width / 4), starts[Math.floor(height * .9)]), right = width, top = 0, bottom = height;
  while (left < width / 4 && dark(left, 0, 0, 1, height)) left++;
  while (right > width * .75 && dark(right - 1, 0, 0, 1, height)) right--;
  while (top < height / 4 && dark(left, top, 1, 0, right - left)) top++;
  while (bottom > height * .6 && dark(left, bottom - 1, 1, 0, right - left)) bottom--;
  if (left === 0 && top === 0 && right === width && bottom === height) return canvas;
  const trimmed = document.createElement('canvas'); trimmed.width = right - left; trimmed.height = bottom - top;
  trimmed.getContext('2d').drawImage(canvas, left, top, trimmed.width, trimmed.height, 0, 0, trimmed.width, trimmed.height);
  return trimmed;
}

function localNormal(m, n) {
  const a=m.slice(0,3),b=m.slice(4,7),c=m.slice(8,11);
  const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]],dot=(a,b)=>a.reduce((sum,v,i)=>sum+v*b[i],0);
  const determinant=dot(a,cross(b,c));
  if(Math.abs(determinant)<1e-10)throw new Error('Menu bind matrix is singular');
  return [cross(b,c),cross(c,a),cross(a,b)].map(row=>dot(row,n)/determinant);
}
