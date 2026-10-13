import { gameplayBindings } from './bindings.js';
// Rust owns menu labels, supported choices and canvas hit regions. This bridge
// draws the original frontend and crisp browser overlays, and binds input.
const base = name => name.replaceAll('\\', '/').split('/').at(-1).toLowerCase();
const unquote = value => (value ?? '').replace(/^"/, '').replace(/"$/, '');
const fields = section => Object.fromEntries([...section.fields].reverse().map(f => [f.key.toLowerCase(), f.value]));
const sections = doc => new Map(doc.sections.map(s => [s.name.toLowerCase(), fields(s)]));
const overlayPages = new Set(['pause','players','hud-settings','minimap-config','controls','bindings','music']);
const fontName = 'tradegothic_boldcondensed_22s';
const selectionKey = 'atv-player-selection';
const normalizedSelection = value => ({
  quadId: Number.isInteger(value?.quadId) && value.quadId >= 1 && value.quadId <= 46 ? value.quadId : 1,
  riderId: value?.riderId === 1 ? 1 : 0,
  freestyleEnvironment: typeof value?.freestyleEnvironment === 'string' && /^(env0[1-8]|free05)$/.test(value.freestyleEnvironment) ? value.freestyleEnvironment : 'env01',
});
export function loadPlayerSelection(read = () => localStorage.getItem(selectionKey)) {
  try { return normalizedSelection(JSON.parse(read())); } catch { return normalizedSelection(null); }
}
export function savePlayerSelection(value, write = text => localStorage.setItem(selectionKey, text)) {
  try { write(JSON.stringify(normalizedSelection(value))); } catch { /* Session-only selection when storage is unavailable. */ }
}

export class Frontend {
  constructor({ codec, json, content, read, start, active, audio, diagnosticsOverlay, mobileControls, multiplayer, crashSettings, minimapSettings, modelAssets, menuRider }) {
    Object.assign(this, { codec, json, content, read, start, active, audio, diagnosticsOverlay, mobileControls, multiplayer, crashSettings, minimapSettings, modelAssets, menuRider });
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
    this.quadId = savedSelection.quadId; this.riderId = savedSelection.riderId; this.lastFreestyleEnvironment = savedSelection.freestyleEnvironment; this.playerName = 'Rider'; this.roomCode = new URL(location.href).searchParams.get('room') ?? ''; this.createCode = ''; this.garageReturn = 'single';
    this.bindingDevice = 'keyboard'; this.bindingPage = 0; this.bindingRow = 0; this.musicPage = 0;
    this.capture = null; this.menuKeys = new Set();
    this.raceSettings={laps:3,cpu:3,difficulty:1};this.raceActive=false;this.raceAvailable=false;
    gameplayBindings.subscribe(() => { this.layoutKey = ''; });
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
      const action = ({ ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right', Enter: 'accept', Space: 'accept', Escape: 'back', Backspace: 'back' })[e.code];
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
        let pads = []; try { pads = [...navigator.getGamepads()].filter(Boolean); } catch { /* browser without gamepad permission */ }
        if (this.capture) { this.pollCapture(pads.find(p => p.connected !== false)); this.pollFrame = requestAnimationFrame(this.poll); return; }
        const held = new Set();
        for (const p of pads) {
          if (p.buttons[0]?.pressed) held.add('accept');
          if (p.buttons[9]?.pressed || gameplayBindings.value('controller','pause',null,p) > .5) held.add('pause');
          if (p.buttons[1]?.pressed || p.buttons[3]?.pressed) held.add('back');
          if (p.buttons[12]?.pressed || p.axes[1] < -.55) held.add('up');
          if (p.buttons[13]?.pressed || p.axes[1] > .55) held.add('down');
          if (p.buttons[14]?.pressed || p.axes[0] < -.55) held.add('left');
          if (p.buttons[15]?.pressed || p.axes[0] > .55) held.add('right');
        }
        if (!held.size) this.padBlocked = false;
        if (!this.root.hidden && !this.padBlocked) for (const action of held) if (!this.lastPad.has(action)) {
          if (action === 'pause' && ['pause','players','hud-settings','minimap-config','controls','bindings','music','multiplayer','create-lobby','join-lobby','lobby','garage'].includes(this.page) && !this.busy) this.resume();
          else void this.action(action === 'pause' ? 'accept' : action);
        }
        this.lastPad = held;
        if (!this.root.hidden && (!this.busy || this.page === 'loading')) void this.refresh();
      } else this.lastPad.clear();
      this.pollFrame = requestAnimationFrame(this.poll);
    };
    this.pollFrame = requestAnimationFrame(this.poll);
  }
  async archive(path) {
    if (this.loaded.has(path)) return;
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
      this.tracks = [...tracks.entries()].filter(([s, f]) => (/^(unspnat\d+|spstunt6)$/.test(s)) && ['env01','env02','env03','env04','env05','env06','env07','env08','free05'].includes(f.scn))
        .map(([, f]) => ({ id: f.scn, name: unquote(f.name), image: f.image }));
      this.track = Math.max(0, this.tracks.findIndex(t => t.id === this.lastFreestyleEnvironment));
      for (const name of [fontName, 'tradegothic_boldcondensed_22soff', 'tradegothic_boldcondenseditalic_-1fs_32ds']) await this.font(name);
      await this.archive('/UI/RESOURCE/PAGEA/PAGE01.RES');
      // Authored vector branding is independent of recovered disc textures.
      // Keep the text fallback usable if the static asset cannot be decoded.
      const logo = new Image(); logo.src = 'fury-remastered.svg';
      try { await logo.decode(); this.images.set('brand-logo', logo); } catch { /* Rust text fallback */ }
      // The thumbnail arrives through a separately validated R2 package. Older
      // manifests can still open every menu while that content update publishes.
      const {files} = await this.content;
      this.raceAvailable=files.some(f=>f.path==='/TRACKS/TRACKSA/ENV01/E01_NAT.RES');
      if (files.some(f => f.path === '/UI/RESOURCE/QUADINFO.RES')) { await this.archive('/UI/RESOURCE/QUADINFO.RES'); await this.image('multi_atv01'); }
      // The original catalog is available before a terrain/engine is loaded.
      const content = await this.content;
      await this.audio?.()?.music.load({ ...content, read: this.read, isCurrent: () => this.alive });
      await this.audio?.()?.loadMenuMusic({ ...content, read: this.read, isCurrent: () => this.alive });
      this.initialized = true;
      if (showTitle) await this.show('title');
    } catch (error) { this.fail(error); }
  }
  fail(error) { this.setLoading(false); this.notice.textContent = error.message; this.busy = false; this.root.dataset.error = 'true'; }
  syncPreview() {
    const rideMenu = overlayPages.has(this.page);
    void this.audio?.()?.setMusicPreview((rideMenu || ['title','main','single','tracks','garage','multiplayer','create-lobby','join-lobby','lobby','race','loading','music'].includes(this.page)) && !this.root.hidden && !document.hidden && document.hasFocus(), !rideMenu).catch(error => this.fail(error));
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
  resume() { this.animatePage(true); this.setLoading(false); this.clearModelViews(); this.cancelCapture(); this.root.hidden = true; this.active(false); void this.audio?.()?.setMusicPreview(false); }
  async show(page) {
    this.padBlocked = true;
    const previous = this.root.hidden ? null : this.canvas;
    this.captureTransition(previous);
    this.cancelCapture();
    this.clearModelViews();
    delete this.root.dataset.error;
    // Arm soundtrack streaming before silencing gameplay voices, avoiding a
    // suspend/resume interruption on entry to the pause screen.
    if (overlayPages.has(page)) void this.audio?.()?.setMusicPreview(!document.hidden && document.hasFocus(), false);
    this.busy = true; this.root.hidden = false; this.active(true); this.notice.textContent = ''; this.buttons.replaceChildren();
    try {
      if (page === 'tracks' || page === 'loading' || page === 'race') {
        if (!this.loaded.has('/UI/RESOURCE/TP/TP_ANAT.RES')) this.setLoading(true, 'Preparing course previews…');
        await this.archive('/UI/RESOURCE/TP/TP_ANAT.RES');
        await this.archive('/UI/RESOURCE/TP/TP_PSTNT.RES');
        for (const track of this.tracks) { await this.image(`${track.image}a`); await this.image(`${track.image}b`); }
      }
      this.page = page; this.selection = 0; this.layoutKey = '';
      if (page === 'pause') this.resetPauseScroll = true;
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
    if (['up','down'].includes(action) && ['title','main','single','garage','multiplayer','create-lobby','join-lobby','lobby','race','pause','players','hud-settings','minimap-config','controls','bindings','music'].includes(this.page)) {
      const order = this.page === 'pause' ? this.pauseOrder : this.frame.choices;
      const index = order.indexOf(this.frame.choices[this.selection]);
      this.selection = this.frame.choices.indexOf(order[(index + (action === 'up' ? -1 : 1) + order.length) % order.length]);
      this.scrollPauseSelection = this.page === 'pause';
      if (this.page === 'bindings' && this.selection < 4) this.bindingRow = this.bindingPage * 4 + this.selection;
    } else if (action === 'back') {
      if (this.page === 'pause') return this.resume();
      if (this.page === 'garage') return this.show(this.garageReturn);
      if (this.page === 'lobby') return this.run('leave');
      return this.show(({players:'pause',bindings:'controls',music:'pause','hud-settings':'pause','minimap-config':'hud-settings',controls:'pause',main:'title',single:'main',tracks:'single',race:'single','create-lobby':'multiplayer','join-lobby':'multiplayer',multiplayer:'main'})[this.page] ?? 'title');
    } else if (this.page === 'garage' && ['left','right'].includes(action)) {
      return this.run(action === 'left' ? 'quadprev' : 'quadnext');
    } else if (this.page === 'tracks' && ['left','right','up','down'].includes(action)) {
      this.track = (this.track + (['left','up'].includes(action) ? -1 : 1) + this.tracks.length) % this.tracks.length;
    } else if (this.page === 'race' && ['left','right'].includes(action)) { return this.run(this.frame.choices[this.selection],action === 'left' ? -1 : 1);
    } else if (this.page === 'minimap-config' && this.frame.choices[this.selection] === 'minimap-size' && ['left','right'].includes(action)) {
      this.minimapSettings?.setSize(this.minimapSettings.size + (action === 'left' ? -8 : 8));
    } else if (this.page === 'minimap-config' && this.frame.choices[this.selection] === 'minimap-zoom' && ['left','right'].includes(action)) {
      this.minimapSettings?.setZoom(this.minimapSettings.zoom + (action === 'left' ? -25 : 25));
    } else if (this.page === 'controls' && this.frame.choices[this.selection] === 'steering-sensitivity' && ['left','right'].includes(action)) {
      this.mobileControls?.setSteeringSensitivity(this.mobileControls.steeringSensitivity + (action === 'left' ? -5 : 5));
    } else if (['bindings','music'].includes(this.page) && ['left','right'].includes(action)) {
      return this.run(`${this.page === 'music' ? 'music' : 'bind'}${action === 'left' ? 'prev' : 'next'}`);
    } else if (action === 'accept') {
      if (this.page === 'title') return this.run(this.frame.choices[this.selection]);
      if (this.page === 'tracks') {
        const environment = this.tracks[this.track].id;
        this.busy = true;
        try { await this.start(environment); this.lastFreestyleEnvironment = environment; this.saveSelection(); this.raceActive=false;this.resume(); }
        catch (error) { await this.show('tracks'); this.fail(error); }
        this.busy = false; return;
      }
      return this.run(this.frame.choices[this.selection]);
    }
    await this.refresh();
  }
  async run(action,direction=1) {
    if (this.busy) return;
    try {
      if (action === 'garage') { this.garageReturn = this.page; return this.show('garage'); }
      if (action === 'quadprev' || action === 'quadnext') { const count = ['multiplayer','create-lobby','join-lobby','lobby'].includes(this.garageReturn) ? 26 : 46; this.quadId = ((this.quadId - 1 + (action === 'quadnext' ? 1 : count - 1)) % count) + 1; }
      else if (action === 'riderchange' && this.quadId < 27) this.riderId = 1 - this.riderId;
      else if (action.startsWith('spawn:')) { this.busy=true; try { await this.multiplayer.action(action,this); } finally {this.busy=false;} return; }
      else if (['host','join','share','lobbymap','lobbystart','leave'].includes(action)) { if (['host','join'].includes(action) && this.quadId>=27) throw new Error('Choose an ATV for multiplayer. Motorcycles are available in Single Player.'); return await this.multiplayer.action(action, this); }
      else if (['main','single','tracks','pause','players','hud-settings','minimap-config','controls','music','multiplayer','create-lobby','join-lobby','lobby','race'].includes(action)) return await this.show(action);
      if (action === 'racestart') {
        await this.multiplayer?.disconnect();
        this.busy = true;
        try { await this.start('env01', {...this.raceSettings}); this.raceActive = true; this.resume(); }
        catch(error) { await this.show('race');this.fail(error); } finally { this.busy = false; }
        return;
      }
      if (['racelaps','racecpu'].includes(action)) {
        if (action === 'racelaps') this.raceSettings.laps = (this.raceSettings.laps - 1 + direction + 5) % 5 + 1;
        if (action === 'racecpu') { const options = [0,3,4]; this.raceSettings.cpu = options[(options.indexOf(this.raceSettings.cpu) + direction + 3) % 3]; }
        this.layoutKey = ''; await this.refresh(); return;
      }
      if (['keyboard','controller'].includes(action)) { this.bindingDevice = action; this.bindingPage = 0; this.bindingRow = 0; return this.show('bindings'); }
      if (action.startsWith('bind:')) { const id = action.slice(5); this.selection = gameplayBindings.rows(this.bindingDevice).findIndex(row => row.id === id) % 4; return this.beginCapture(id); }
      if (action === 'bindcancel') return this.cancelCapture();
      if (['bindprev','bindnext','musicprev','musicnext'].includes(action)) {
        const music = action.startsWith('music'), rows = music ? (this.audio?.()?.music?.tracks ?? []) : gameplayBindings.rows(this.bindingDevice), pages = Math.max(1,Math.ceil(rows.length / 4)), prop = music ? 'musicPage' : 'bindingPage';
        this[prop] = (this[prop] + (action.endsWith('prev') ? -1 : 1) + pages) % pages; this.selection = 0;
        if (!music) this.bindingRow = this.bindingPage * 4;
      } else if (action === 'bindclear') { const row = gameplayBindings.rows(this.bindingDevice)[this.bindingRow]; if (row) gameplayBindings.setBinding(this.bindingDevice,row.id,[]); }
      else if (action === 'bindreset') gameplayBindings.reset(this.bindingDevice);
      else if (action.startsWith('song:')) { const music = this.audio?.()?.music; music?.setEnabled(true); music?.select(Number(action.slice(5))); this.syncPreview(); }
      else if (action === 'musictoggle') { const music = this.audio?.()?.music; if (music) music.setEnabled(!music.snapshot().enabled); this.syncPreview(); }
      else if (action === 'musicvolume') { const music = this.audio?.()?.music; if (music) music.setVolume((Math.round(music.snapshot().volume * 4) + 1) % 5 / 4); }
      else if (action === 'steering-sensitivity') { this.buttons.querySelector('input[type=range]')?.focus(); return; }
      else if (action === 'minimap-zoom') { this.buttons.querySelector('input[data-setting="minimap-zoom"]')?.focus(); return; }
      else if (action === 'minimap-size') { this.buttons.querySelector('input[data-setting="minimap-size"]')?.focus(); return; }
      else if (action === 'minimap-orientation') this.minimapSettings?.toggleOrientation();
      else if (action === 'minimap-shape') this.minimapSettings?.toggleShape();
      else if (action === 'minimap-corner') this.minimapSettings?.toggleCorner();
      else if (action === 'minimap-toggle') this.minimapSettings?.toggle();
      else if (action === 'onehanded') this.mobileControls?.toggleOneHanded();
      else if (action === 'funnycrashes') this.crashSettings?.toggle();
      else if (action === 'diagnostics') this.diagnosticsOverlay?.cycle();
      else if (action === 'resume') return this.resume();
      else if (action === 'reset') { await document.getElementById('reset').onclick(); return this.resume(); }
      else if (action === 'fullscreen') {
        if (document.fullscreenElement) await document.exitFullscreen(); else await document.documentElement.requestFullscreen();
      } else if (action === 'devlog') location.href = 'devlog.html';
      else if (!['quadprev','quadnext','riderchange'].includes(action)) return await this.action(action);
      if (['quadprev','quadnext','riderchange'].includes(action)) this.saveSelection();
      this.layoutKey = ''; await this.refresh();
    } catch (error) { this.fail(error); }
  }
  saveSelection() {
    savePlayerSelection({quadId:this.quadId, riderId:this.riderId, freestyleEnvironment:this.lastFreestyleEnvironment});
  }
  async refresh(force = false) {
    if (!this.initialized || this.root.hidden || (this.busy && this.page !== 'loading' && !force) || this.layoutPending) return;
    const stage = this.canvas.parentElement, track = this.page === 'race' ? this.tracks.find(t=>t.id==='env01') : this.tracks[this.track];
    const music = this.audio?.()?.music, musicState = music?.snapshot();
    const lobby = this.multiplayer?.state?.lobby;
    const request = {lobby_players:(lobby?.players??[]).map(p=>({id:String(p.id),label:`P${p.id+1} ${p.name}${p.id===this.multiplayer.state.peer?' (You)':''}`,value:p.id===this.multiplayer.state.peer?'self':(this.multiplayer.remote.find(r=>r.peer===p.id && !r.stale)?'ready':'waiting')})),race_laps:this.raceSettings.laps,race_cpu:this.raceSettings.cpu,race_difficulty:this.raceSettings.difficulty,race_available:this.raceAvailable,race_active:this.raceActive,quad_id:this.quadId,rider_id:this.riderId,player_name:this.playerName,room_code:this.multiplayer?.state?.code ?? (this.page==='create-lobby'?this.createCode:this.roomCode),lobby_status:lobby ? `${lobby.players.length}/4 riders / ${lobby.players.map(p => p.name).join(', ')}` : '',lobby_started:Boolean(lobby?.started),lobby_host:Boolean(lobby && this.multiplayer.state.peer === lobby.host),page:this.page,loading_status:this.loadingStatus,width:stage.clientWidth,height:stage.clientHeight,selection:this.selection,track_name:(this.page === 'loading' ? this.loadingTrack?.name : this.page === 'lobby' ? this.tracks.find(t => t.id === this.multiplayer.state.environment)?.name : track?.name) ?? '',track_image:(this.page === 'loading' ? this.loadingTrack?.image : track?.image) ?? '',fullscreen:Boolean(document.fullscreenEnabled),
      minimap_orientation:this.minimapSettings?.orientation ?? 'north-up',minimap_shape:this.minimapSettings?.shape ?? 'circle',minimap_corner:this.minimapSettings?.corner ?? 'top-right',minimap_enabled:this.minimapSettings?.enabled ?? true,minimap_size:this.minimapSettings?.size ?? 160,minimap_zoom:this.minimapSettings?.zoom ?? 50,funny_crashes:this.crashSettings?.enabled ?? true,mobile_steering_sensitivity:this.mobileControls?.steeringSensitivity ?? 85,one_handed_mobile:this.mobileControls?.oneHanded ?? true,diagnostics_mode:this.diagnosticsOverlay?.mode ?? 'off',control_rows:this.page === 'bindings' ? gameplayBindings.rows(this.bindingDevice).map(row => ({id:row.id,label:row.label,value:row.labelBinding})) : [],
      binding_device:this.bindingDevice,binding_page:this.bindingPage,capture_label:this.capture?.label ?? '',
      music_ready:Boolean(music?.tracks?.length),music_rows:this.page === 'music' ? music.tracks.map(row => ({id:String(row.index),label:row.title,value:`${row.index === musicState.playingIndex && musicState.enabled ? 'Playing / ' : row.index === musicState.selectedIndex ? 'Selected / ' : ''}${row.artist}`})) : [],
      music_page:this.musicPage,music_status:musicState ? `${musicState.enabled ? 'On' : 'Off'} / Volume ${Math.round(musicState.volume * 100)}% / ${musicState.title || 'Choose a song'}` : '',music_enabled:musicState?.enabled ?? true,music_volume:Math.round((musicState?.volume ?? .7)*100),menu_logo:this.images.has('brand-logo'),menu_atv:this.images.has('multi_atv01')};
    const key = JSON.stringify(request);
    if (this.layoutKey === key) return;
    this.layoutPending = true;
    try {
      const frame = await this.json(62,new TextEncoder().encode(key));
      // Page/viewport can change while the worker handles the request. The next
      // frame requests fresh data, rather than applying a stale hit map.
      if ((this.minimapSettings?.zoom ?? 50) !== request.minimap_zoom || (this.minimapSettings?.orientation ?? 'north-up') !== request.minimap_orientation || (this.minimapSettings?.shape ?? 'circle') !== request.minimap_shape || (this.minimapSettings?.corner ?? 'top-right') !== request.minimap_corner || (this.minimapSettings?.enabled ?? true) !== request.minimap_enabled || (this.minimapSettings?.size ?? 160) !== request.minimap_size || this.raceSettings.laps!==request.race_laps || this.raceSettings.cpu!==request.race_cpu || this.raceSettings.difficulty!==request.race_difficulty || (this.mobileControls?.steeringSensitivity ?? 85) !== request.mobile_steering_sensitivity || (this.capture?.label ?? '') !== request.capture_label || this.bindingPage !== request.binding_page || this.musicPage !== request.music_page || this.page !== request.page || this.selection !== request.selection || this.quadId !== request.quad_id || this.riderId !== request.rider_id || stage.clientWidth !== request.width || stage.clientHeight !== request.height || this.root.hidden) return;
      this.frame = frame; this.layoutKey = key; this.draw(); this.rebuildButtons(); this.updateModelViews();
    } catch (error) { this.fail(error); }
    finally { this.layoutPending = false; }
  }
  async beginLoading(environment) {
    if (!this.initialized) this.setLoading(true, 'Preparing course…');
    this.loadingTrack = this.tracks.find(track => track.id === environment) ?? this.tracks[this.track];
    this.loadingStatus = 'Preparing course…';
    await this.show('loading'); this.busy = true;
    // Yield a frame so the loading UI is painted before decoding/upload work.
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  }
  loadingStage(text) {
    if (this.page !== 'loading' || this.root.hidden) return;
    // Keep decoder details in diagnostics; the player sees the actual loading phase.
    this.loadingStatus = /engine|sound/i.test(text) ? 'Tuning sound…' : /ATV/i.test(text) ? 'Preparing your ride…' : /rider/i.test(text) ? 'Getting your rider ready…' : /texture|sky|model|vegetation/i.test(text) ? 'Building the scenery…' : /collision/i.test(text) ? 'Preparing the trail…' : /incomplete|Static terrain/i.test(text) ? 'Ready to ride' : 'Opening course…';
    this.layoutKey = '';
    this.courseMeter?.setAttribute('aria-label', this.loadingStatus); void this.refresh();
  }
  setLoading(visible, detail = 'Preparing menus…', title = 'Loading') {
    if (!this.loadingScreen) return;
    this.loadingScreen.hidden = !visible;
    this.loadingScreen.setAttribute('aria-busy', String(visible));
    document.getElementById('loading-title').textContent = title;
    document.getElementById('loading-detail').textContent = detail;
  }
  captureTransition(canvas = this.canvas) {
    for (const animation of this.transitions) animation.cancel();
    this.transitions = []; this.transitionGhost?.remove(); this.transitionGhost = null;
    if (this.page === 'pause' || !canvas || !this.frame || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
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
      this.transitions.push(exit); exit.finished.then(() => ghost.remove(), () => ghost.remove());
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
    if (this.page !== 'garage') return;
    const commands = this.frame.draw.filter(command => command.kind === 'model_preview');
    for (const command of commands) {
      let view = this.modelViews.get(command.model);
      if (!view) {
        view = new ModelView(this.canvas.parentElement, command.model);
        this.modelViews.set(command.model, view);
      }
      view.place(command.rect, this.frame);
    }
    const key = `${this.quadId}:${this.riderId}`;
    if (key === this.modelKey) return;
    cancelAnimationFrame(this.modelAnimation);
    this.modelKey = key; const token = ++this.modelToken;
    for (const view of this.modelViews.values()) {
      view.clear('Loading model...');
      delete view.canvas.dataset.clip; delete view.canvas.dataset.animationTime;
    }
    void this.modelAssets(this.quadId, this.riderId).then(source => {
      if (token !== this.modelToken || this.page !== 'garage' || this.root.hidden) return;
      this.modelViews.get('quad').set(source.mesh, source.textures);
      this.modelViews.get('rider').set(source.riderMesh, source.riderTextures, source.riderPose?.world_matrices);
      void this.startMenuRider(token, key);
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
      if (!original || token !== this.modelToken || this.page !== 'garage') return;
      const view = this.modelViews.get('rider');
      view.yaw = original.previewYaw ?? -.6;
      view.set(original.mesh,original.textures,original.pose.world_matrices);
      view.canvas.dataset.selection = key; view.canvas.dataset.clip = original.clip;
      const start = performance.now(); let busy = false, last = 0;
      const tick = async time => {
        if (token !== this.modelToken || !view.element.isConnected) return;
        this.modelAnimation = requestAnimationFrame(tick);
        if (document.hidden || !document.hasFocus() || busy || time-last < 33) return;
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
    this.pausePanel.hidden = this.page !== 'pause';
    this.root.dataset.page = this.page;
    if (this.page === 'pause') { this.rebuildPausePanel(); return; }
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
  rebuildPausePanel() {
    // Rust remains the source of available actions and stateful labels. Native
    // text/layout keeps this browser overlay sharp and accessible at any zoom.
    const activeAction = this.pausePanel.contains(document.activeElement) ? document.activeElement.dataset.action : null;
    const scroll = this.resetPauseScroll ? 0 : this.pausePanel.scrollTop;
    this.resetPauseScroll = false;
    this.buttons.replaceChildren();
    this.courseMeter?.remove(); this.courseMeter = null;
    const header = document.createElement('header');
    const eyebrow = document.createElement('p'); eyebrow.className = 'pause-eyebrow'; eyebrow.textContent = 'FURY REMASTERED';
    const title = document.createElement('h1'); title.id = 'pause-title';
    const online = Boolean(this.multiplayer?.state?.lobby?.started);
    title.textContent = online ? 'Ride menu' : 'Paused';
    const status = document.createElement('p'); status.className = 'pause-status';
    status.textContent = online ? 'Multiplayer is live. The ride keeps going while this menu is open.' : 'Take a breather. Your ride is paused.';
    header.append(eyebrow, title, status);
    const body = document.createElement('div'); body.className = 'pause-groups';
    const groups = [
      ['Ride', ['resume','reset','players']],
      ['Preferences', ['controls','hud-settings','music','onehanded','funnycrashes','diagnostics','fullscreen']],
      ['Session', ['devlog','main','lobby']],
    ];
    const ordered = [];
    for (const [name, actions] of groups) {
      const section = document.createElement('section'); section.className = `pause-group pause-${name.toLowerCase()}`;
      const heading = document.createElement('h2'); heading.textContent = name; section.append(heading);
      for (const action of actions) {
        const hit = this.frame.hits.find(hit => hit.action === action); if (!hit) continue;
        ordered.push(action);
        const button = document.createElement('button'); button.type = 'button'; button.dataset.action = action;
        button.setAttribute('aria-label', hit.label);
        const [label, ...value] = hit.label.split(': ');
        const text = document.createElement('span'); text.textContent = label; button.append(text);
        if (value.length) { const setting = document.createElement('span'); setting.className = 'pause-value'; setting.textContent = value.join(': '); button.append(setting); }
        else { const arrow = document.createElement('span'); arrow.className = 'pause-arrow'; arrow.textContent = action === 'resume' ? '↗' : '›'; arrow.setAttribute('aria-hidden','true'); button.append(arrow); }
        button.setAttribute('aria-current', String(this.frame.choices[this.selection] === action));
        button.onclick = () => { this.selection = this.frame.choices.indexOf(action); void this.run(action); };
        button.onfocus = () => { const index = this.frame.choices.indexOf(action); if (!this.restoringPauseFocus && this.selection !== index) { this.selection = index; this.layoutKey = ''; } };
        section.append(button);
      }
      body.append(section);
    }
    this.pauseOrder = ordered;
    this.pausePanel.replaceChildren(header, body); this.pausePanel.scrollTop = scroll;
    this.restoringPauseFocus = true;
    if (activeAction) this.pausePanel.querySelector(`[data-action="${activeAction}"]`)?.focus({preventScroll:true});
    if (this.scrollPauseSelection) {
      const selected = this.pausePanel.querySelector('[aria-current="true"]');
      selected?.focus({preventScroll:true}); selected?.scrollIntoView({block:'nearest'});
      this.scrollPauseSelection = false;
    }
    this.restoringPauseFocus = false;
    this.root.dataset.selection = String(this.selection); this.root.dataset.renderer = 'rust';
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
    if (this.page === 'pause') { this.ctx.clearRect(0,0,this.canvas.width,this.canvas.height); if(this.backgroundCanvas)this.backgroundCanvas.hidden=true; return; }
    const c = this.ctx, stage = this.canvas.parentElement, dpr = Math.min(devicePixelRatio || 1,3, Math.sqrt(8_000_000 / Math.max(1,stage.clientWidth * stage.clientHeight)));
    this.canvas.width = Math.round(stage.clientWidth*dpr); this.canvas.height = Math.round(stage.clientHeight*dpr);
    c.setTransform(this.canvas.width/frame.width,0,0,this.canvas.height/frame.height,0,0);
    const fullBleed = document.documentElement.classList.contains('app-standalone') && !['pause','hud-settings','minimap-config','controls','bindings','music'].includes(this.page) && this.backgroundCanvas;
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
        if (overlayPages.has(this.page)) {
          const screenScale = stage.clientWidth / frame.width;
          const size = Math.min(command.font.includes('italic') ? 30 : 22, (command.font.includes('italic') ? 30 : 20) * command.scale * screenScale) / screenScale;
          c.save(); c.font = `600 ${size}px system-ui, sans-serif`; c.textBaseline = 'top';
          c.fillStyle = `rgb(${command.color})`; c.fillText(command.text, ...command.at, command.max_width); c.restore(); continue;
        }
        // The font is still the original CEL/PST glyph atlas, not a screenshot
        // of a page. Rust owns text content, positioning and maximum bounds.
        const font = this.fonts.get(command.font), width = [...command.text].reduce((width,char) => {
          const g = font?.glyphs.get(char.charCodeAt(0)); return width + (g ? (g.metrics ? g.metrics[0]+g.metrics[2] : (g.uv[2]-g.uv[0])*font.image.width) : 0);
        },0);
        const scale = Math.min(command.scale,command.max_width/Math.max(width,1));
        c.save(); c.translate(...command.at); c.scale(scale,scale);
        this.text(command.text,0,0,command.font,command.color); c.restore();
      } else if (command.kind === 'artwork') {
        const image = this.images.get(command.image); if (image) c.drawImage(image,...command.rect);
      } else if (command.kind === 'preview') {
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
class ModelView {
  constructor(stage, kind) {
    this.element = document.createElement('div'); this.element.className = 'model-view';
    this.canvas = document.createElement('canvas'); this.canvas.setAttribute('aria-label', `${kind === 'quad' ? 'ATV' : 'Rider'} model; drag to rotate`);
    this.canvas.tabIndex = 0; this.message = document.createElement('span'); this.message.setAttribute('role', 'status');
    this.element.append(this.canvas, this.message); stage.append(this.element);
    this.gl = this.canvas.getContext('webgl2', { alpha: true, antialias: true });
    this.yaw = -.6; this.groups = []; this.textures = []; this.kind = kind;
    this.canvas.addEventListener('pointerdown', event => { this.pointer = {id:event.pointerId, x:event.clientX}; this.canvas.setPointerCapture(event.pointerId); });
    this.canvas.addEventListener('pointermove', event => { if (this.pointer?.id !== event.pointerId) return; this.yaw += (event.clientX - this.pointer.x) * .015; this.pointer.x = event.clientX; this.render(); });
    for (const type of ['pointerup','pointercancel','lostpointercapture']) this.canvas.addEventListener(type, () => { this.pointer = null; });
    this.canvas.addEventListener('keydown', event => { if (!['ArrowLeft','ArrowRight'].includes(event.code)) return; event.preventDefault(); event.stopPropagation(); this.yaw += event.code === 'ArrowLeft' ? -.25 : .25; this.render(); });
    this.canvas.addEventListener('webglcontextlost', event => { event.preventDefault(); this.lost = true; delete this.canvas.dataset.ready; this.message.textContent = 'Restoring model…'; this.element.classList.add('is-loading'); });
    this.canvas.addEventListener('webglcontextrestored', () => { this.lost = false; this.program = null; this.groups = []; this.textures = []; if (this.source) this.set(...this.source); });
  }
  place([x,y,w,h], frame) {
    Object.assign(this.element.style, {left:`${x/frame.width*100}%`,top:`${y/frame.height*100}%`,width:`${w/frame.width*100}%`,height:`${h/frame.height*100}%`}); this.render();
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
  set(mesh, materials, pose = mesh.nodes.map(node => node.world_matrix)) {
    this.clear(); this.pose = pose; this.source = [mesh, materials, pose]; const gl = this.gl;
    if (!gl || this.lost) { this.message.textContent = '3D preview unavailable'; return; }
    if (!this.program) {
      const shader = (type, text) => { const s=gl.createShader(type); gl.shaderSource(s,text); gl.compileShader(s); if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s)); return s; };
      const vertex=shader(gl.VERTEX_SHADER, `#version 300 es
        layout(location=0) in vec3 p; layout(location=1) in vec3 n; layout(location=2) in vec2 uv; layout(location=3) in float node; uniform mat4 bones[32];
        uniform vec3 center; uniform vec2 extent; uniform float yaw; out vec2 tex; out float light;
        void main(){float c=cos(yaw),s=sin(yaw);mat3 r=mat3(c,0.,-s,0.,1.,0.,s,0.,c);vec3 v=r*((bones[int(node)]*vec4(p,1.)).xyz-center);
        float pitch=.25;float y=v.y*cos(pitch)-v.z*sin(pitch);float z=v.y*sin(pitch)+v.z*cos(pitch);
        gl_Position=vec4(v.x/extent.x,y/extent.y,-z/40.,1.);tex=uv;light=.58+.42*max(dot(normalize(r*mat3(bones[int(node)])*n),normalize(vec3(-.4,.8,.6))),0.);}`);
      const fragment=shader(gl.FRAGMENT_SHADER, `#version 300 es
        precision highp float; in vec2 tex; in float light; uniform sampler2D image; out vec4 color;
        void main(){vec4 t=texture(image,tex);if(t.a<.15)discard;color=vec4(t.rgb*light,t.a);}`);
      this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);
      gl.deleteShader(vertex);gl.deleteShader(fragment);if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(this.program));
      this.uniforms=Object.fromEntries(['center','extent','yaw','image','bones[0]'].map(n=>[n,gl.getUniformLocation(this.program,n)]));
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
      this.groups.push({vao,buffer,count:group.indices.length,texture:gpu.get(mesh.materials[group.material])??fallback});
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
    gl.uniform3fv(this.uniforms.center,this.center);gl.uniform2f(this.uniforms.extent,extent*aspect,extent);gl.uniform1f(this.uniforms.yaw,this.yaw);gl.uniform1i(this.uniforms.image,0);gl.activeTexture(gl.TEXTURE0);
    for(const group of this.groups){gl.bindVertexArray(group.vao);gl.bindTexture(gl.TEXTURE_2D,group.texture);gl.drawArrays(gl.TRIANGLES,0,group.count);}
    this.canvas.dataset.glError=String(gl.getError());
  }
  destroy(){this.clear();if(this.program&&!this.lost)this.gl.deleteProgram(this.program);this.gl?.getExtension('WEBGL_lose_context')?.loseContext();this.element.remove();}
}

function localNormal(m, n) {
  const a=m.slice(0,3),b=m.slice(4,7),c=m.slice(8,11);
  const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]],dot=(a,b)=>a.reduce((sum,v,i)=>sum+v*b[i],0);
  const determinant=dot(a,cross(b,c));
  if(Math.abs(determinant)<1e-10)throw new Error('Menu bind matrix is singular');
  return [cross(b,c),cross(c,a),cross(a,b)].map(row=>dot(row,n)/determinant);
}
