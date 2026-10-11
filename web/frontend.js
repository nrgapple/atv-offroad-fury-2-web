// Original page layouts, bitmap fonts and animated textures are decoded by Rust.
// Page routing is reconstructed here; unfinished modes never pretend to start a race.
const base = name => name.replaceAll('\\', '/').split('/').at(-1).toLowerCase();
const unquote = value => (value ?? '').replace(/^"/, '').replace(/"$/, '');
const fields = section => Object.fromEntries([...section.fields].reverse().map(f => [f.key.toLowerCase(), f.value]));
const sections = doc => new Map(doc.sections.map(s => [s.name.toLowerCase(), fields(s)]));
const num = (value, fallback = 0) => value === undefined || value === '' ? fallback : Number(value);
const truth = value => /^(t|true|1)$/i.test(value ?? '');
const fontName = 'tradegothic_boldcondensed_22s';
export class Frontend {
  constructor({ codec, json, content, read, start, active }) {
    Object.assign(this, { codec, json, content, read, start, active });
    this.root = document.getElementById('frontend'); this.canvas = document.getElementById('menu-canvas');
    this.ctx = this.canvas.getContext('2d'); this.buttons = document.getElementById('menu-buttons');
    this.notice = document.getElementById('menu-notice');
    this.assets = new Map(); this.images = new Map(); this.fonts = new Map(); this.materials = new Map(); this.animations = new Map();
    this.loaded = new Set(); this.page = 'loading'; this.selection = 0; this.track = 0; this.busy = false; this.lastPad = new Set(); this.alive = true;
    window.addEventListener('keydown', e => {
      if (e.repeat || this.busy) return;
      if (this.root.hidden) { if (e.code === 'Escape') { e.preventDefault(); void this.show('pause'); } return; }
      const action = ({ ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right', Enter: 'accept', Space: 'accept', Escape: 'back', Backspace: 'back' })[e.code];
      if (action) { e.preventDefault(); void this.action(action); }
    });
    window.addEventListener('pagehide', () => { this.alive = false; });
    this.poll = () => {
      if (!this.alive) return;
      if (!document.hidden && document.hasFocus()) {
        let pads = []; try { pads = [...navigator.getGamepads()].filter(Boolean); } catch { /* browser without gamepad permission */ }
        const held = new Set();
        for (const p of pads) {
          if (p.buttons[0]?.pressed) held.add('accept');
          if (p.buttons[9]?.pressed) held.add('pause');
          if (p.buttons[1]?.pressed || p.buttons[3]?.pressed) held.add('back');
          if (p.buttons[12]?.pressed || p.axes[1] < -.55) held.add('up');
          if (p.buttons[13]?.pressed || p.axes[1] > .55) held.add('down');
          if (p.buttons[14]?.pressed || p.axes[0] < -.55) held.add('left');
          if (p.buttons[15]?.pressed || p.axes[0] > .55) held.add('right');
        }
        if (this.root.hidden) {
          if (held.has('pause') && !this.lastPad.has('pause')) void this.show('pause');
        } else for (const action of held) if (!this.lastPad.has(action)) {
          if (action === 'pause' && ['pause','controls'].includes(this.page) && !this.busy) this.resume();
          else void this.action(action === 'pause' ? 'accept' : action);
        }
        this.lastPad = held;
        if (!this.root.hidden && !this.busy && this.page !== 'loading') this.draw();
      } else this.lastPad.clear();
      requestAnimationFrame(this.poll);
    };
    requestAnimationFrame(this.poll);
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
  async material(name) {
    name = name.toLowerCase(); if (this.materials.has(name)) return;
    let texture = name;
    if (this.assets.has(`${name}.psm`)) {
      const doc = await this.document(`${name}.psm`), pass = doc.get('shaderpass_1');
      texture = pass?.texturemap_1?.toLowerCase() || name;
      this.materials.set(name, { texture, fps: num(pass?.animatedmapframespersecond, 78) });
    } else this.materials.set(name, { texture, fps: 78 });
    if (this.assets.has(`${texture}.pst`)) await this.image(texture);
    else if (this.assets.has(`${texture}.atm`) && !this.animations.has(texture)) {
      const bytes = await this.data(`${texture}.atm`), info = await this.json(56, bytes), frames = [];
      for (let i = 0; i < info.frames; i++) frames.push(this.bitmap(info, await this.codec(57, bytes, i)));
      this.animations.set(texture, frames);
    }
  }
  async init(showTitle = true) {
    try {
      await this.archive('/STARTAPP.RES'); await this.archive('/UI/RESOURCE/TITLE.RES');
      this.globals = await this.document('gwidgets.dat');
      const tracks = await this.document('tracks.dat');
      // Only environments verified by this reconstruction are offered for driving.
      this.tracks = [...tracks.entries()].filter(([s, f]) => /^unspnat\d+$/.test(s) && ['env01', 'env02'].includes(f.scn))
        .map(([, f]) => ({ id: f.scn, name: unquote(f.name), image: f.image }));
      this.track = this.tracks.findIndex(t => t.id === 'env01');
      await this.font(fontName); await this.image('title'); if (showTitle) await this.show('title');
    } catch (error) { this.fail(error); }
  }
  fail(error) { this.notice.textContent = error.message; this.busy = false; this.root.dataset.error = 'true'; }
  resume() { this.root.hidden = true; this.active(false); }
  async loadPause() {
    if (this.pauseItems) return;
    await this.archive('/STARTAPP.RES'); await this.archive('/STARTEVT.RES'); await this.archive('/OVERLAYS.RES');
    await this.archive('/TRACKS/TRACKSA/ENV01/E01_FR.RES');
    const pauseLayout = (await this.document('overlays.pb', 'Main Menu')).get('main menu');
    this.pauseLayout = pauseLayout;
    if (!this.pauseLayout) throw new Error('Original pause layout is missing.');
    const p = this.pauseLayout;
    for (let i = 1; i <= num(p.numicons); i++) await this.image(unquote(p[`icon${i}texture`]));
    for (let i = 1; i <= num(p.numselectedicons); i++) await this.image(unquote(p[`selectedicon${i}texture`]));
    for (const name of [fontName, unquote(p.selectedfont), unquote(p.text12font)]) await this.font(name);
    // Browser commands use the original pause graphics; these callbacks are port-specific.
    this.pauseItems = [
      { label: 'Resume', run: () => this.resume() },
      { label: 'Reset ATV', run: async () => { await document.getElementById('reset').onclick(); this.resume(); } },
      { label: 'Controls', run: () => this.show('controls') },
      { label: 'Fullscreen', run: async () => {
        if (!document.fullscreenEnabled) { this.notice.textContent = 'This browser already uses the available game view. Use landscape for a wider view.'; return; }
        if (document.fullscreenElement) await document.exitFullscreen(); else await document.documentElement.requestFullscreen();
      } },
      { label: 'Dev log', run: () => { location.href = 'devlog.html'; } },
      { label: 'Quit to Main Menu', run: () => this.show('main') },
    ];
  }
  pausePosition(index) {
    const [x,y] = this.pauseLayout.text1offset.split(',').map(Number);
    return [x, y + index * (this.portraitPause ? 72 : 35)];
  }
  pauseIcon(prefix, offset = [0,0], extraHeight = 0) {
    const p = this.pauseLayout, name = base(unquote(p[`${prefix}texture`]));
    let image = this.images.get(name);
    if (!image) return;
    const color = p[`${prefix}color`];
    if (color) {
      const key = `${name}:${color}`;
      if (!this.images.has(key)) {
        const tinted = document.createElement('canvas'); tinted.width = image.width; tinted.height = image.height;
        const ctx = tinted.getContext('2d'); ctx.drawImage(image,0,0);
        const pixels = ctx.getImageData(0,0,image.width,image.height), tint = color.split(',').map(Number);
        for (let i = 0; i < pixels.data.length; i += 4) for (let channel = 0; channel < 3; channel++) pixels.data[i+channel] = pixels.data[i+channel] * tint[channel] / 255;
        ctx.putImageData(pixels,0,0); this.images.set(key,tinted);
      }
      image = this.images.get(key);
    }
    const [x,y] = p[`${prefix}offset`].split(',').map(Number);
    const [u1,u2,v1,v2] = p[`${prefix}uvs`].split(',').map(Number);
    const width = num(p[`${prefix}width`], Math.abs(u2-u1)+1), height = num(p[`${prefix}height`], Math.abs(v2-v1)+1) + extraHeight;
    const c = this.ctx; c.save(); c.globalAlpha = num(p[`${prefix}alpha`],255)/255;
    c.translate(x+offset[0]+(u2<u1?width:0),y+offset[1]+(v2<v1?height:0)); c.scale(u2<u1?-1:1,v2<v1?-1:1);
    // Inclusive texel bounds also support one-pixel stretched panel strips.
    c.drawImage(image,Math.min(u1,u2),Math.min(v1,v2),Math.min(image.width-Math.min(u1,u2),Math.abs(u2-u1)+1),Math.min(image.height-Math.min(v1,v2),Math.abs(v2-v1)+1),0,0,width,height); c.restore();
  }
  drawPause() {
    const p = this.pauseLayout; if (!p) return;
    const portrait = innerWidth < 600 && innerHeight > innerWidth;
    if (this.portraitPause !== portrait || this.canvas.height !== (portrait ? 760 : 448)) {
      this.portraitPause = portrait; this.canvas.height = portrait ? 760 : 448;
      this.canvas.parentElement.style.aspectRatio = `640 / ${this.canvas.height}`;
      this.rebuildButtons();
    }
    const stretch = portrait ? 250 : 0;
    for (let i = 1; i <= num(p.numicons); i++) this.pauseIcon(`icon${i}`, [0,[3,6,9,11,12,13,14].includes(i)?stretch:0], [2,5,8].includes(i)?stretch:0);
    const title = this.page === 'controls' ? 'Controls' : unquote(p.text12text);
    const titlePos = p.text12offset.split(',').map(Number);
    this.text(title,...titlePos,unquote(p.text12font));
    if (this.page === 'pause') {
      const pos = this.pausePosition(this.selection);
      for (let i = 1; i <= num(p.numselectedicons); i++) this.pauseIcon(`selectedicon${i}`, pos);
      this.pauseItems.forEach((item,i) => {
        const [x,y] = this.pausePosition(i), chosen = i === this.selection;
        this.ctx.save(); this.ctx.translate(x,y); if (portrait) this.ctx.scale(1.25,1.25);
        this.text(item.label,0,0,chosen?unquote(p.selectedfont):fontName,chosen?p.selectedcolor:p.unselectedcolor,chosen?num(p.selectedalpha):num(p.unselectedalpha)); this.ctx.restore();
      });
    } else {
      const lines = ['TOUCH', 'Left: slide forward to accelerate.', 'Pull back to brake, then reverse.', 'Right: hold to preload; drag to lean.', 'Lift your finger to release preload.', '', 'CONTROLLER', 'Left stick: steer. Triggers: drive / brake.', 'Left stick up / down: lean.', 'Y / triangle: camera. Start: pause.', '', 'KEYBOARD', 'WASD / arrows: drive. R: reverse.', 'Q / E: lean. C: camera. Esc: pause.'];
      lines.forEach((line,i) => { this.ctx.save(); this.ctx.translate(95,125+i*(portrait?31:17)); if (!portrait) this.ctx.scale(.75,.75); this.text(line,0,0,fontName,'220,220,220'); this.ctx.restore(); });
    }
    this.text(this.page === 'controls'?'Back':'Select',317,365+stretch,fontName);
  }
  async show(page) {
    delete this.root.dataset.error;
    this.busy = true; this.root.hidden = false; this.active(true); this.notice.textContent = ''; this.buttons.replaceChildren();
    this.canvas.height = 448; this.canvas.parentElement.style.aspectRatio = '640 / 448';
    try {
      if (page === 'pause' || page === 'controls') {
        await this.loadPause();
      } else if (page !== 'title') {
        const number = { main: '01', single: '02', tracks: '06' }[page];
        // widgets.dat has the same basename in each archive: explicitly replace this page's member.
        await this.archive(`/UI/RESOURCE/PAGEA/PAGE${number}.RES`);
        const { file, files } = await this.content, record = files.find(f => f.path === `/UI/RESOURCE/PAGEA/PAGE${number}.RES`);
        const bytes = await this.read(file, record.lba * 2048, record.bytes), entries = await this.json(2, bytes);
        this.widgets = sections(await this.json(54, await this.codec(3, bytes, entries.findIndex(e => base(e.name) === 'widgets.dat'))));
        const title = { main: 'Main', single: 'SinglePlayer', tracks: 'TrackSelect' }[page];
        this.layout = await this.document(`${title}.dat`);
        this.background = await this.image(this.layout.get('pageinfo').background);
        const selected = this.widgetList(page);
        for (const w of selected) {
          if (w.fontname) await this.font(w.fontname);
          const texture = w.texture || w.materialdeselect || w.texturedeselect;
          if (texture) await this.material(texture);
        }
        if (page === 'tracks') {
          await this.archive('/UI/RESOURCE/TP/TP_ANAT.RES');
          for (const t of this.tracks) { await this.image(`${t.image}a`); await this.image(`${t.image}b`); }
        }
      }
      this.page = page; this.selection = 0; this.changed = performance.now(); this.busy = false; this.rebuildButtons(); this.draw();
    } catch (error) { this.fail(error); }
  }
  widget(id) {
    const w = this.widgets.get(`widget ${id}`);
    if (!w) return {};
    return w.type === 'Global' ? { ...this.globals.get(`widget ${w.globalwidgetnumber}`), global: w.globalwidgetnumber } : w;
  }
  control(id) {
    const c = this.layout.get(`control ${id}`);
    return c ? Array.from({ length: num(c.numberofwidgets) }, (_, i) => this.widget(c[`widgetid${i + 1}`])) : [];
  }
  widgetList(page = this.page) {
    if (page === 'tracks') return [1,2,3,4,5,6,7,12,13,14,15,16,17,38,39,40,42,43,44,45,72,76,77].map(i => this.widget(i));
    return [0, ...Array.from({ length: page === 'main' ? 6 : 4 }, (_, i) => i + 1)].flatMap(i => this.control(i));
  }
  label(id) { return unquote(this.control(id).find(w => w.type === 'Overlay_Text' && w.text)?.text); }
  button(label, x, y, width, height, action, selected = false) {
    const b = document.createElement('button'); b.type = 'button'; b.textContent = label; b.setAttribute('aria-label', label);
    b.style.left = `${x / 6.4}%`; b.style.top = `${y / this.canvas.height * 100}%`; b.style.width = `${width / 6.4}%`; b.style.height = `${height / this.canvas.height * 100}%`;
    b.setAttribute('aria-current', String(selected)); b.onclick = () => void action(); this.buttons.append(b);
  }
  rebuildButtons() {
    this.buttons.replaceChildren();
    if (this.page === 'title') this.button('Start', 0, 0, 640, 448, () => this.action('accept'));
    else if (this.page === 'pause') {
      this.pauseItems.forEach((item, i) => {
        const [x,y] = this.pausePosition(i);
        this.button(item.label, x - 42, y - 1, 457, this.portraitPause ? 72 : 35, () => { this.selection = i; return this.action('accept'); }, i === this.selection);
      });
    } else if (this.page === 'controls') {
      this.button('Back', 275, this.portraitPause ? 610 : 360, 290, this.portraitPause ? 72 : 40, () => this.show('pause'));
    } else if (this.page === 'tracks') {
      this.button('Previous environment', 15, 125, 40, 80, () => this.action('left'));
      this.button('Next environment', 245, 125, 50, 80, () => this.action('right'));
      this.button(`Ride ${this.tracks[this.track].name}`, 350, 380, 86, 50, () => this.action('accept'));
    } else {
      const count = this.page === 'main' ? 6 : 4;
      for (let i = 0; i < count; i++) this.button(this.label(i + 1), 18, 70 + i * 30, 270, 30, () => { this.selection = i; return this.action('accept'); }, i === this.selection);
      this.button('Next', 350, 380, 86, 50, () => this.action('accept'));
    }
    if (!['title','pause','controls'].includes(this.page)) this.button('Back', 440, 380, 95, 50, () => this.action('back'));
    this.root.dataset.page = this.page; this.root.dataset.selection = String(this.selection);
    this.root.dataset.track = this.tracks[this.track]?.id ?? '';
  }
  async action(action) {
    if (this.busy || this.root.hidden) return;
    this.notice.textContent = '';
    if (this.page === 'pause') {
      if (action === 'back') return this.resume();
      if (action === 'up' || action === 'down') {
        this.selection = (this.selection + (action === 'up' ? -1 : 1) + this.pauseItems.length) % this.pauseItems.length;
        this.rebuildButtons();
      } else if (action === 'accept') {
        try { await this.pauseItems[this.selection].run(); } catch (error) { this.fail(error); }
      }
      this.draw(); return;
    }
    if (this.page === 'controls') { if (action === 'back' || action === 'accept') return this.show('pause'); return; }
    if (action === 'back') return this.show(({ main: 'title', single: 'main', tracks: 'single' })[this.page] ?? 'title');
    if (this.page === 'title') { if (action === 'accept') return this.show('main'); return; }
    if (this.page === 'tracks') {
      if (['left', 'right', 'up', 'down'].includes(action)) { this.track = (this.track + (['left','up'].includes(action) ? -1 : 1) + this.tracks.length) % this.tracks.length; this.rebuildButtons(); }
      if (action === 'accept') {
        this.busy = true; this.notice.textContent = `Loading ${this.tracks[this.track].name}…`;
        delete this.root.dataset.error;
        try { await this.start(this.tracks[this.track].id); this.root.hidden = true; this.active(false); }
        catch (error) { this.fail(error); }
        this.busy = false;
      }
    } else if (action === 'up' || action === 'down') {
      const count = this.page === 'main' ? 6 : 4; this.selection = (this.selection + (action === 'up' ? -1 : 1) + count) % count; this.changed = performance.now(); this.rebuildButtons();
    } else if (action === 'accept') {
      if (this.page === 'main' && this.selection === 0) return this.show('single');
      if (this.page === 'single' && this.selection === 1) return this.show('tracks');
      this.notice.textContent = `${this.label(this.selection + 1)} is not implemented yet. Single Player → Freestyle opens free riding.`;
    }
    this.draw();
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
  drawWidget(w, selected = false, overrides = {}) {
    w = { ...w, ...overrides };
    if ((truth(w.enableonselect) && !selected) || (truth(w.disableonselect) && selected)) return;
    if (w.fontname) {
      const lines = w.text !== undefined ? [unquote(w.text)] : Array.from({ length: num(w.numberoflines) }, (_, i) => unquote(w[`line${i + 1}`]));
      const height = this.fonts.get(w.fontname.toLowerCase())?.height ?? 25;
      lines.forEach((line, i) => this.text(line, num(w.xposition), num(w.yposition) + i * num(w.leadingfactor, 1) * height, w.fontname, w.color, num(w.alpha, 255), w.justification));
      return;
    }
    const name = (w.texture || w.materialdeselect || w.texturedeselect || '').toLowerCase(), material = this.materials.get(name), animation = this.animations.get(material?.texture);
    // Selection enters the recovered material's frame sequence; unselected stays at frame zero.
    const frame = selected ? Math.min(animation?.length - 1, Math.floor((performance.now() - this.changed) * (material?.fps ?? 78) / 1000)) : 0;
    const image = animation?.[Math.max(0, frame)] ?? this.images.get(material?.texture ?? name);
    if (!image) return;
    const src = w.partialrect?.split(',').map(Number) ?? [0, image.width, 0, image.height];
    const x = num(w.xlocation ?? w.xlocationdeselect), y = num(w.ylocation ?? w.ylocationdeselect);
    const dst = w.maprect?.split(',').map(Number) ?? [x, x + src[1]-src[0], y, y+src[3]-src[2]];
    this.ctx.globalAlpha = num(w.alpha, 255) / 255;
    this.ctx.save(); this.ctx.translate(dst[0], dst[2]); this.ctx.scale(Math.sign(dst[1]-dst[0]) || 1, Math.sign(dst[3]-dst[2]) || 1);
    this.ctx.drawImage(image, Math.min(src[0],src[1]),Math.min(src[2],src[3]),Math.abs(src[1]-src[0]),Math.abs(src[3]-src[2]),0,0,Math.abs(dst[1]-dst[0]),Math.abs(dst[3]-dst[2]));
    this.ctx.restore(); this.ctx.globalAlpha = 1;
  }
  draw() {
    const c = this.ctx; c.clearRect(0,0,640,448);
    if (this.page === 'pause' || this.page === 'controls') { this.drawPause(); return; }
    if (this.page === 'title') { c.drawImage(this.images.get('title'),0,0); this.text('Press START',320,298,fontName,'255,255,255',255,'CenterHorizontal'); return; }
    if (!this.background) return;
    c.drawImage(this.background,0,0);
    if (this.page === 'tracks') {
      const list = this.widgetList().sort((a,b) => Number(Boolean(a.fontname)) - Number(Boolean(b.fontname)));
      // Dynamic original widgets are populated from Tracks.dat, without placeholder or lock text.
      for (const w of list) {
        if (w.global === '32') this.drawWidget(w,true,{text:'"Freestyle"'});
        else if (w.text?.includes('really long track')) this.drawWidget(w,true,{text:`"${this.tracks[this.track].name}"`});
        else if (w.text?.includes('really long type')) this.drawWidget(w,true,{text:'"Free Ride"'});
        else this.drawWidget(w,true);
      }
      const t = this.tracks[this.track]; c.drawImage(this.images.get(`${t.image}a`),303,92); c.drawImage(this.images.get(`${t.image}b`),559,92);
      return;
    }
    const controls = [0,...Array.from({length:this.page === 'main' ? 6 : 4},(_,i)=>i+1)];
    const widgets = controls.flatMap(id => this.control(id).map(w=>({w,selected:id===this.selection+1})));
    // Draw original animation plates behind their text regardless of file widget order.
    widgets.sort((a,b)=>(num(a.w.depth)-num(b.w.depth)) || (Number(Boolean(a.w.fontname))-Number(Boolean(b.w.fontname))));
    for (const {w,selected} of widgets) this.drawWidget(w,selected,w.global==='32'?{text:`"${this.page==='main'?'Main Menu':'Single Player'}"`}:{});
  }
}
