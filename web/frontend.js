// Rust owns responsive menu geometry, labels, supported choices and hit regions.
// This bridge draws commands with recovered glyph atlases and binds browser input.
const base = name => name.replaceAll('\\', '/').split('/').at(-1).toLowerCase();
const unquote = value => (value ?? '').replace(/^"/, '').replace(/"$/, '');
const fields = section => Object.fromEntries([...section.fields].reverse().map(f => [f.key.toLowerCase(), f.value]));
const sections = doc => new Map(doc.sections.map(s => [s.name.toLowerCase(), fields(s)]));
const fontName = 'tradegothic_boldcondensed_22s';
export class Frontend {
  constructor({ codec, json, content, read, start, active }) {
    Object.assign(this, { codec, json, content, read, start, active });
    this.root = document.getElementById('frontend'); this.canvas = document.getElementById('menu-canvas');
    this.ctx = this.canvas.getContext('2d'); this.buttons = document.getElementById('menu-buttons');
    this.notice = document.getElementById('menu-notice');
    this.assets = new Map(); this.images = new Map(); this.fonts = new Map();
    this.layoutKey = ''; this.layoutPending = false; this.loaded = new Set(); this.page = 'loading'; this.selection = 0; this.track = 0; this.busy = false; this.lastPad = new Set(); this.alive = true;
    window.addEventListener('keydown', e => {
      if (e.repeat || this.busy) return;
      if (this.root.hidden) { if (e.code === 'Escape') { e.preventDefault(); void this.show('pause'); } return; }
      const action = ({ ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right', Enter: 'accept', Space: 'accept', Escape: 'back', Backspace: 'back' })[e.code];
      if (action) { e.preventDefault(); void this.action(action); }
    });
    window.addEventListener('pagehide', event => {
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
      this.tracks = [...tracks.entries()].filter(([s, f]) => /^unspnat\d+$/.test(s) && ['env01','env02','env03','env04','env05','env06','env07','env08'].includes(f.scn))
        .map(([, f]) => ({ id: f.scn, name: unquote(f.name), image: f.image }));
      this.track = this.tracks.findIndex(t => t.id === 'env01');
      for (const name of [fontName, 'tradegothic_boldcondensed_22soff', 'tradegothic_boldcondenseditalic_-1fs_32ds']) await this.font(name);
      if (showTitle) await this.show('title');
    } catch (error) { this.fail(error); }
  }
  fail(error) { this.notice.textContent = error.message; this.busy = false; this.root.dataset.error = 'true'; }
  resume() { this.root.hidden = true; this.active(false); }
  async show(page) {
    delete this.root.dataset.error;
    this.busy = true; this.root.hidden = false; this.active(true); this.notice.textContent = ''; this.buttons.replaceChildren();
    try {
      if (page === 'tracks') {
        await this.archive('/UI/RESOURCE/TP/TP_ANAT.RES');
        for (const track of this.tracks) { await this.image(`${track.image}a`); await this.image(`${track.image}b`); }
      }
      this.page = page; this.selection = 0; this.busy = false; this.layoutKey = '';
      await this.refresh();
    } catch (error) { this.fail(error); }
  }
  async action(action) {
    if (this.busy || this.root.hidden) return;
    this.notice.textContent = '';
    if (['up','down'].includes(action) && ['title','main','pause'].includes(this.page)) {
      this.selection = (this.selection + (action === 'up' ? -1 : 1) + this.frame.choices.length) % this.frame.choices.length;
    } else if (action === 'back') {
      if (this.page === 'pause') return this.resume();
      return this.show(({controls:'pause',main:'title',single:'main',tracks:'single'})[this.page] ?? 'title');
    } else if (this.page === 'tracks' && ['left','right','up','down'].includes(action)) {
      this.track = (this.track + (['left','up'].includes(action) ? -1 : 1) + this.tracks.length) % this.tracks.length;
    } else if (action === 'accept') {
      if (this.page === 'title') return this.run(this.frame.choices[this.selection]);
      if (this.page === 'controls') return this.show('pause');
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
      if (['main','single','tracks','controls'].includes(action)) return await this.show(action);
      if (action === 'resume') return this.resume();
      if (action === 'reset') { await document.getElementById('reset').onclick(); return this.resume(); }
      if (action === 'fullscreen') {
        if (document.fullscreenElement) await document.exitFullscreen(); else await document.documentElement.requestFullscreen();
      } else if (action === 'devlog') location.href = 'devlog.html';
      else return await this.action(action);
    } catch (error) { this.fail(error); }
  }
  async refresh() {
    if (this.root.hidden || this.busy || this.layoutPending) return;
    const stage = this.canvas.parentElement, track = this.tracks[this.track];
    const request = {page:this.page,width:stage.clientWidth,height:stage.clientHeight,selection:this.selection,track_name:track?.name ?? '',track_image:track?.image ?? '',fullscreen:Boolean(document.fullscreenEnabled)};
    const key = JSON.stringify(request);
    if (this.layoutKey === key) return;
    this.layoutPending = true;
    try {
      const frame = await this.json(62,new TextEncoder().encode(key));
      // Page/viewport can change while the worker handles the request. The next
      // frame requests fresh data, rather than applying a stale hit map.
      if (this.page !== request.page || this.selection !== request.selection || this.tracks[this.track]?.image !== request.track_image || stage.clientWidth !== request.width || stage.clientHeight !== request.height || this.root.hidden) return;
      this.frame = frame; this.layoutKey = key; this.draw(); this.rebuildButtons();
    } catch (error) { this.fail(error); }
    finally { this.layoutPending = false; }
  }
  rebuildButtons() {
    this.buttons.replaceChildren();
    for (const hit of this.frame.hits) {
      const b = document.createElement('button'), [x,y,w,h] = hit.rect;
      b.type = 'button'; b.textContent = hit.label; b.setAttribute('aria-label',hit.label);
      b.style.left = `${x/this.frame.width*100}%`; b.style.top = `${y/this.frame.height*100}%`;
      b.style.width = `${w/this.frame.width*100}%`; b.style.height = `${h/this.frame.height*100}%`;
      b.setAttribute('aria-current',String(hit.selected));
      b.onclick = () => void this.run(hit.action);
      this.buttons.append(b);
    }
    this.root.dataset.page = this.page; this.root.dataset.selection = String(this.selection);
    this.root.dataset.track = this.tracks[this.track]?.id ?? '';
    this.root.dataset.renderer = 'rust';
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
    const c = this.ctx, stage = this.canvas.parentElement, dpr = Math.min(devicePixelRatio || 1,2);
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
