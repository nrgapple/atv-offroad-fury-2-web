import { gameplayBindings } from './bindings.js';
// Rust owns responsive menu geometry, labels, supported choices and hit regions.
// This bridge draws commands with recovered glyph atlases and binds browser input.
const base = name => name.replaceAll('\\', '/').split('/').at(-1).toLowerCase();
const unquote = value => (value ?? '').replace(/^"/, '').replace(/"$/, '');
const fields = section => Object.fromEntries([...section.fields].reverse().map(f => [f.key.toLowerCase(), f.value]));
const sections = doc => new Map(doc.sections.map(s => [s.name.toLowerCase(), fields(s)]));
const fontName = 'tradegothic_boldcondensed_22s';
export class Frontend {
  constructor({ codec, json, content, read, start, active, audio, diagnosticsOverlay, mobileControls, multiplayer, crashSettings }) {
    Object.assign(this, { codec, json, content, read, start, active, audio, diagnosticsOverlay, mobileControls, multiplayer, crashSettings });
    this.root = document.getElementById('frontend'); this.canvas = document.getElementById('menu-canvas');
    this.ctx = this.canvas.getContext('2d'); this.buttons = document.getElementById('menu-buttons');
    this.notice = document.getElementById('menu-notice');
    this.assets = new Map(); this.images = new Map(); this.fonts = new Map();
    this.layoutKey = ''; this.layoutPending = false; this.loaded = new Set(); this.page = 'loading'; this.selection = 0; this.track = 0; this.busy = false; this.lastPad = new Set(); this.alive = true;
    this.quadId = 1; this.riderId = 0; this.playerName = 'Rider'; this.roomCode = new URL(location.href).searchParams.get('room') ?? ''; this.garageReturn = 'single';
    this.bindingDevice = 'keyboard'; this.bindingPage = 0; this.bindingRow = 0; this.musicPage = 0;
    this.capture = null; this.menuKeys = new Set();
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
          if (action === 'pause' && ['pause','controls','bindings','music','multiplayer','lobby','garage'].includes(this.page) && !this.busy) this.resume();
          else void this.action(action === 'pause' ? 'accept' : action);
        }
        this.lastPad = held;
        if (!this.root.hidden && !this.busy && this.page !== 'loading') void this.refresh();
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
      this.track = this.tracks.findIndex(t => t.id === 'env01');
      for (const name of [fontName, 'tradegothic_boldcondensed_22soff', 'tradegothic_boldcondenseditalic_-1fs_32ds']) await this.font(name);
      await this.archive('/UI/RESOURCE/PAGEA/PAGE01.RES');
      const logoA = await this.image('mmlogo01'), logoB = await this.image('mmlogo02');
      const logo = document.createElement('canvas'); logo.width = logoA.width + logoB.width; logo.height = Math.max(logoA.height,logoB.height);
      logo.getContext('2d').drawImage(logoA,0,0); logo.getContext('2d').drawImage(logoB,logoA.width,0); this.images.set('original-logo',logo);
      // The thumbnail arrives through a separately validated R2 package. Older
      // manifests can still open every menu while that content update publishes.
      const {files} = await this.content;
      if (files.some(f => f.path === '/UI/RESOURCE/QUADINFO.RES')) { await this.archive('/UI/RESOURCE/QUADINFO.RES'); await this.image('multi_atv01'); }
      if (showTitle) await this.show('title');
    } catch (error) { this.fail(error); }
  }
  fail(error) { this.notice.textContent = error.message; this.busy = false; this.root.dataset.error = 'true'; }
  syncPreview() {
    void this.audio?.()?.setMusicPreview(this.page === 'music' && !this.root.hidden && !document.hidden && document.hasFocus()).catch(error => this.fail(error));
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
  resume() { this.cancelCapture(); void this.audio?.()?.setMusicPreview(false); this.root.hidden = true; this.active(false); }
  async show(page) {
    this.padBlocked = true;
    this.cancelCapture();
    if (page !== 'music') void this.audio?.()?.setMusicPreview(false);
    delete this.root.dataset.error;
    this.busy = true; this.root.hidden = false; this.active(true); this.notice.textContent = ''; this.buttons.replaceChildren();
    try {
      if (page === 'tracks') {
        await this.archive('/UI/RESOURCE/TP/TP_ANAT.RES');
        await this.archive('/UI/RESOURCE/TP/TP_PSTNT.RES');
        for (const track of this.tracks) { await this.image(`${track.image}a`); await this.image(`${track.image}b`); }
      }
      this.page = page; this.selection = 0; this.busy = false; this.layoutKey = '';
      this.syncPreview();
      await this.refresh();
    } catch (error) { this.fail(error); }
  }
  async action(action) {
    if (this.busy || this.root.hidden) return;
    this.notice.textContent = '';
    if (['up','down'].includes(action) && ['title','main','single','pause','controls','bindings','music'].includes(this.page)) {
      this.selection = (this.selection + (action === 'up' ? -1 : 1) + this.frame.choices.length) % this.frame.choices.length;
      if (this.page === 'bindings' && this.selection < 4) this.bindingRow = this.bindingPage * 4 + this.selection;
    } else if (action === 'back') {
      if (this.page === 'pause') return this.resume();
      if (this.page === 'garage') return this.show(this.garageReturn);
      if (this.page === 'lobby') return this.run('leave');
      return this.show(({bindings:'controls',music:'pause',controls:'pause',main:'title',single:'main',tracks:'single',multiplayer:'main'})[this.page] ?? 'title');
    } else if (this.page === 'tracks' && ['left','right','up','down'].includes(action)) {
      this.track = (this.track + (['left','up'].includes(action) ? -1 : 1) + this.tracks.length) % this.tracks.length;
    } else if (this.page === 'controls' && this.frame.choices[this.selection] === 'steering-sensitivity' && ['left','right'].includes(action)) {
      this.mobileControls?.setSteeringSensitivity(this.mobileControls.steeringSensitivity + (action === 'left' ? -5 : 5));
    } else if (['bindings','music'].includes(this.page) && ['left','right'].includes(action)) {
      return this.run(`${this.page === 'music' ? 'music' : 'bind'}${action === 'left' ? 'prev' : 'next'}`);
    } else if (action === 'accept') {
      if (this.page === 'title') return this.run(this.frame.choices[this.selection]);
      if (this.page === 'tracks') {
        this.busy = true; this.notice.textContent = `Loading ${this.tracks[this.track].name}…`;
        try { await this.start(this.tracks[this.track].id); this.resume(); }
        catch (error) { this.fail(error); }
        this.busy = false; return;
      }
      return this.run(this.frame.choices[this.selection]);
    }
    await this.refresh();
  }
  async run(action) {
    if (this.busy) return;
    try {
      if (action === 'garage') { this.garageReturn = this.page; return this.show('garage'); }
      if (action === 'quadprev' || action === 'quadnext') { this.quadId = ((this.quadId - 1 + (action === 'quadnext' ? 1 : 25)) % 26) + 1; }
      else if (action === 'riderchange') this.riderId = 1 - this.riderId;
      else if (['host','join','share','lobbymap','lobbystart','leave'].includes(action)) return await this.multiplayer.action(action, this);
      else if (['main','single','tracks','controls','music','multiplayer','lobby'].includes(action)) return await this.show(action);
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
      else if (action === 'onehanded') this.mobileControls?.toggleOneHanded();
      else if (action === 'funnycrashes') this.crashSettings?.toggle();
      else if (action === 'diagnostics') this.diagnosticsOverlay?.cycle();
      else if (action === 'resume') return this.resume();
      else if (action === 'reset') { await document.getElementById('reset').onclick(); return this.resume(); }
      else if (action === 'fullscreen') {
        if (document.fullscreenElement) await document.exitFullscreen(); else await document.documentElement.requestFullscreen();
      } else if (action === 'devlog') location.href = 'devlog.html';
      else return await this.action(action);
      this.layoutKey = ''; await this.refresh();
    } catch (error) { this.fail(error); }
  }
  async refresh() {
    if (this.root.hidden || this.busy || this.layoutPending) return;
    const stage = this.canvas.parentElement, track = this.tracks[this.track];
    const music = this.audio?.()?.music, musicState = music?.snapshot();
    const lobby = this.multiplayer?.state?.lobby;
    const request = {quad_id:this.quadId,rider_id:this.riderId,player_name:this.playerName,room_code:this.multiplayer?.state?.code ?? this.roomCode,lobby_status:lobby ? `${lobby.players.length}/4 riders / ${lobby.players.map(p => p.name).join(', ')}` : '',lobby_started:Boolean(lobby?.started),lobby_host:Boolean(lobby && this.multiplayer.state.peer === lobby.host),page:this.page,width:stage.clientWidth,height:stage.clientHeight,selection:this.selection,track_name:(this.page === 'lobby' ? this.tracks.find(t => t.id === this.multiplayer.state.environment)?.name : track?.name) ?? '',track_image:track?.image ?? '',fullscreen:Boolean(document.fullscreenEnabled),
      funny_crashes:this.crashSettings?.enabled ?? false,mobile_steering_sensitivity:this.mobileControls?.steeringSensitivity ?? 85,one_handed_mobile:this.mobileControls?.oneHanded ?? true,diagnostics_mode:this.diagnosticsOverlay?.mode ?? 'off',control_rows:this.page === 'bindings' ? gameplayBindings.rows(this.bindingDevice).map(row => ({id:row.id,label:row.label,value:row.labelBinding})) : [],
      binding_device:this.bindingDevice,binding_page:this.bindingPage,capture_label:this.capture?.label ?? '',
      music_ready:Boolean(music?.tracks?.length),music_rows:this.page === 'music' ? music.tracks.map(row => ({id:String(row.index),label:row.title,value:`${row.index === musicState.playingIndex && musicState.enabled ? 'Playing / ' : row.index === musicState.selectedIndex ? 'Selected / ' : ''}${row.artist}`})) : [],
      music_page:this.musicPage,music_status:musicState ? `${musicState.enabled ? 'On' : 'Off'} / Volume ${Math.round(musicState.volume * 100)}% / ${musicState.title || 'Choose a song'}` : '',music_enabled:musicState?.enabled ?? true,music_volume:Math.round((musicState?.volume ?? .7)*100),menu_logo:this.images.has('original-logo'),menu_atv:this.images.has('multi_atv01')};
    const key = JSON.stringify(request);
    if (this.layoutKey === key) return;
    this.layoutPending = true;
    try {
      const frame = await this.json(62,new TextEncoder().encode(key));
      // Page/viewport can change while the worker handles the request. The next
      // frame requests fresh data, rather than applying a stale hit map.
      if ((this.mobileControls?.steeringSensitivity ?? 85) !== request.mobile_steering_sensitivity || (this.capture?.label ?? '') !== request.capture_label || this.bindingPage !== request.binding_page || this.musicPage !== request.music_page || this.page !== request.page || this.selection !== request.selection || this.tracks[this.track]?.image !== request.track_image || stage.clientWidth !== request.width || stage.clientHeight !== request.height || this.root.hidden) return;
      this.frame = frame; this.layoutKey = key; this.draw(); this.rebuildButtons();
    } catch (error) { this.fail(error); }
    finally { this.layoutPending = false; }
  }
  rebuildButtons() {
    // Preserve native range nodes during input/redraw so dragging and keyboard
    // focus survive every Rust label update.
    for (const element of [...this.buttons.children]) if (element.tagName !== 'INPUT' || ![...this.frame.sliders, ...(this.frame.inputs ?? [])].some(item => item.action === element.dataset.setting)) element.remove();
    for (const hit of this.frame.hits) {
      const b = document.createElement('button'), [x,y,w,h] = hit.rect;
      b.dataset.action = hit.action; b.type = 'button'; b.textContent = hit.label; b.setAttribute('aria-label',hit.label);
      b.style.left = `${x/this.frame.width*100}%`; b.style.top = `${y/this.frame.height*100}%`;
      b.style.width = `${w/this.frame.width*100}%`; b.style.height = `${h/this.frame.height*100}%`;
      b.setAttribute('aria-current',String(hit.selected));
      b.onclick = () => void this.run(hit.action);
      this.buttons.append(b);
    }
    for (const slider of this.frame.sliders) {
      let input = this.buttons.querySelector(`input[data-setting="${slider.action}"]`);
      if (!input) {
        input = document.createElement('input'); input.type = 'range'; input.className = 'atv-range'; input.dataset.setting = slider.action;
        input.setAttribute('aria-label', slider.label);
        input.addEventListener('input', () => {
          this.mobileControls?.setSteeringSensitivity(Number(input.value));
          input.setAttribute('aria-valuetext', `${input.value}%`);
          input.style.setProperty('--atv-range-fill', `${(Number(input.value) - Number(input.min)) / (Number(input.max) - Number(input.min)) * 100}%`);
          this.layoutKey = ''; void this.refresh();
        });
        this.buttons.append(input);
      }
      const [x,y,w,h] = slider.rect;
      Object.assign(input.style, { position: 'absolute', left: `${x/this.frame.width*100}%`, top: `${y/this.frame.height*100}%`, width: `${w/this.frame.width*100}%`, height: `${h/this.frame.height*100}%` });
      input.min = slider.min; input.max = slider.max; input.step = slider.step; input.value = slider.value;
      input.setAttribute('aria-valuetext', `${slider.value}%`);
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
          else this.roomCode = input.value.toUpperCase().trim();
          this.layoutKey = '';
        });
        this.buttons.append(input);
      }
      const [x,y,w,h] = field.rect;
      Object.assign(input.style, {position:'absolute',left:`${x/this.frame.width*100}%`,top:`${y/this.frame.height*100}%`,width:`${w/this.frame.width*100}%`,height:`${h/this.frame.height*100}%`});
      input.maxLength = field.max_length; input.setAttribute('aria-label', field.label);
    }
    this.root.dataset.page = this.page; this.root.dataset.selection = String(this.selection);
    this.root.dataset.track = this.tracks[this.track]?.id ?? '';
    this.root.dataset.renderer = 'rust';
    this.root.dataset.musicPage = String(this.musicPage);
    this.root.dataset.bindingPage = String(this.bindingPage);
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
    const c = this.ctx, stage = this.canvas.parentElement, dpr = Math.min(devicePixelRatio || 1,3, Math.sqrt(8_000_000 / Math.max(1,stage.clientWidth * stage.clientHeight)));
    this.canvas.width = Math.round(stage.clientWidth*dpr); this.canvas.height = Math.round(stage.clientHeight*dpr);
    c.setTransform(this.canvas.width/frame.width,0,0,this.canvas.height/frame.height,0,0);
    for (const command of frame.draw) {
      if (command.kind === 'text') {
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
        const path = command.kind === 'path' ? new Path2D(command.d) : new Path2D();
        if (command.kind === 'rect') path.roundRect(...command.rect,command.radius);
        const y = command.rect?.[1] ?? 0, height = command.rect?.[3] ?? frame.height;
        const paint=c.createLinearGradient(0,y,0,y+height); paint.addColorStop(0,command.color); paint.addColorStop(1,command.bottom);
        c.fillStyle=paint;c.fill(path);
        if (command.stroke) {c.strokeStyle=command.stroke;c.lineWidth=1.5;c.stroke(path);}
      }
    }
  }
}
