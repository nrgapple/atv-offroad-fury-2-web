// Draw original runtime HUD layouts with crisp HD text. All changing values and animation
// clocks arrive from authored Rust; this bridge never derives scores from input.
import { courseResource } from './launch.js';
import { observedSize } from './render-support.js';
const unquote = value => (value ?? '').replace(/^"/, '').replace(/"$/, '');
const numbers = value => value?.split(',').map(s => Number(s.trim()));
const fontDefault = 'tradegothic_boldcondensed_22s';
// 002e0770: overlay text without a colour draws 190,190,190.
const raceText = '190,190,190', raceGold = '218,192,31';
// Original MM:SS.hh (rounded hundredths); 0 shows dashes.
export const overlayTime = t => {
  if (!t) return '--:--.--';
  if (t > 5999.99) return '99:99.99';
  if (t < -5999.99) return '-99:99.99';
  const h = Math.floor(Math.abs(t) * 100 + .5), pad = n => String(n).padStart(2, '0');
  return `${t < 0 ? '-' : ''}${pad(Math.floor(h / 6000))}:${pad(Math.floor(h / 100) % 60)}.${pad(h % 100)}`;
};
const RACE_OBJECTS = [1, 2, 3, 4, 7, 11, 14];
const SIGNS = { 1: 'Single-Sign-1', 2: 'Single-Sign-2', 3: 'Single-Sign-3', 12: 'Single-Sign-12', 13: 'Single-Sign-13', 14: 'Single-Sign-14', 15: 'Single-Sign-15' };

// Record text/icon draws before sizing a tightly bounded canvas.
// Replaying the original transforms preserves DPR, filtering and glyph placement.
class HudDrawList {
  constructor() { this.commands = []; this.operations = []; this.stack = []; this.alpha = 1; }
  get globalAlpha() { return this.alpha; }
  set globalAlpha(value) { this.alpha = value; this.operations.push(['alpha', value]); }
  setTransform(a, b, c, d, x, y) { this.transform = [a, b, c, d, x, y]; this.operations.push(['setTransform', a, b, c, d, x, y]); }
  save() { this.stack.push([this.transform.slice(), this.alpha]); this.operations.push(['save']); }
  restore() { [this.transform, this.alpha] = this.stack.pop(); this.operations.push(['restore']); }
  translate(x, y) { const t = this.transform; t[4] += t[0] * x; t[5] += t[3] * y; this.operations.push(['translate', x, y]); }
  scale(x, y) { this.transform[0] *= x; this.transform[3] *= y; this.operations.push(['scale', x, y]); }
  drawText(text, x, y, width, height, color) {
    const t=this.transform;
    this.operations.push(['text',text,x,y,width,height,color]);
    this.commands.push({bounds:[t[0]*(x-3)+t[4],t[3]*(y-3)+t[5],t[0]*(x+width+3)+t[4],t[3]*(y+height+4)+t[5]]});
  }
  drawImage(image, ...args) {
    const [x, y, w, h] = args.length === 4 ? args : args.slice(4);
    if (!(w > 0 && h > 0) || !(this.globalAlpha > 0)) return;
    const t = this.transform.slice();
    this.operations.push(['drawImage', image, ...args]);
    this.commands.push({ image, args, transform: t, alpha: this.globalAlpha,
      bounds: [t[0] * x + t[4], t[3] * y + t[5], t[0] * (x + w) + t[4], t[3] * (y + h) + t[5]] });
  }
}

export class HeadUpDisplay {
  constructor(frontend, surface = document.getElementById('game')) {
    this.frontend = frontend; this.surface = surface; this.ready = false;
    this.canvas = document.createElement('canvas'); this.canvas.id = 'native-hud';
    this.canvas.setAttribute('aria-hidden', 'true');
    Object.assign(this.canvas.style, { position: 'absolute', inset: '0', width: '100%', height: '100%', pointerEvents: 'none', zIndex: '2' });
    surface.parentElement.append(this.canvas); this.ctx = this.canvas.getContext('2d');
    this.canvas.width = this.canvas.height = 1; this.painted = false; this.lastKey = ''; this.draws = 0; this.clears = 0;
    this.layout = new Map(); this.images = new Map(); this.tints = new Map();
  }
  async init(environment, racing = false) {
    this.ready = false; this.lastKey = ''; this.clear();
    const f = this.frontend;
    this.race = null;
    // Each authorized Freestyle runtime archive contains the original HUD art
    // and layout. Reuse the resource cache also used by the frontend.
    await f.archive('/STARTEVT.RES');
    await f.archive(courseResource(environment).archive);
    // Small shared ENV01 jump-HUD images support Freestyle on Coonce's hockey
    // venue, whose original SO archive has no preload display assets.
    await f.archive('/OVERLAYS.RES');
    for (const name of ['OverlayObject-5', 'OverlayObject-12']) {
      const sections = await f.document('Overlays.pb', name);
      this.layout.set(name, sections.get(name.toLowerCase()));
    }
    for (const layout of this.layout.values()) {
      for (let i = 1; i <= Number(layout.numtext); i++) await f.font(unquote(layout[`text${i}font`]) || fontDefault);
      for (let i = 1; i <= Number(layout.numicons); i++) {
        const name = unquote(layout[`icon${i}texture`]);
        this.images.set(name, await f.image(name));
      }
    }
    // Race start 321 LIGHTS (OverlayObject-10): race and Freeride course
    // archives carry the lamp art; Training does not.
    this.lamps = null;
    try {
      const names = ['321ron', '321roff', '321gon', '321goff'];
      const images = await Promise.all(names.map(name => f.image(name)));
      this.lamps = Object.fromEntries(names.map((name, i) => [name, images[i]]));
    } catch { this.lamps = null; }
    if (racing) {
      try { this.race = await this.initRace(environment); } catch (error) { this.race = null; this.raceError = error.message; }
    }
    this.ready = true;
    // The Rust controller parses the same original section. Keep resource bytes
    // out of source code and preserve the exact build's authoring overrides.
    const original = await f.data('Overlays.pb'), text = new TextDecoder('latin1').decode(original);
    const headers = [...text.matchAll(/^\[([^\]\r\n]+)\][^\S\r\n]*\r?$/gm)];
    const index = headers.findIndex(m => m[1].toLowerCase() === 'overlayobject-12');
    return original.slice(headers[index].index, headers[index + 1]?.index ?? original.length);
  }
  // OverlaySet1 single-player race overlays, from the race archive: layouts,
  // sign icons, the radar map from the race TOY and the radar racer icon.
  async initRace(environment) {
    const f = this.frontend;
    await f.archive(courseResource(environment, true).archive);
    const layout = new Map();
    for (const n of RACE_OBJECTS) layout.set(n, (await f.document('Overlays.pb', `OverlayObject-${n}`)).get(`overlayobject-${n}`));
    const signs = new Map();
    for (const [id, name] of Object.entries(SIGNS)) signs.set(Number(id), (await f.document('Overlays.pb', name)).get(name.toLowerCase()));
    for (const section of [...layout.values(), ...signs.values()]) {
      for (let i = 1; i <= Number(section.numtext); i++) await f.font(unquote(section[`text${i}font`]) || fontDefault);
      for (let i = 1; i <= Number(section.numicons); i++) { const name = unquote(section[`icon${i}texture`]); this.images.set(name, await f.image(name)); }
    }
    const radar = (await f.document(`${environment}nat.toy`, 'Radar')).get('radar');
    const map = await f.image(radar.texture);
    const racer = await f.image('radaricon_racer');
    return { layout, signs, radar: { map, left: Number(radar.left), top: Number(radar.top), right: Number(radar.right), bottom: Number(radar.bottom) }, racer, tinted: new Map() };
  }
  tinted(image, rgb) {
    const key = rgb, cache = this.race.tinted;
    if (cache.has(key)) return cache.get(key);
    const canvas = document.createElement('canvas'); canvas.width = image.width; canvas.height = image.height;
    const c = canvas.getContext('2d'); c.drawImage(image, 0, 0); c.globalCompositeOperation = 'multiply';
    c.fillStyle = `rgb(${rgb})`; c.fillRect(0, 0, canvas.width, canvas.height);
    c.globalCompositeOperation = 'destination-in'; c.drawImage(image, 0, 0);
    cache.set(key, canvas); return canvas;
  }
  // Original 640x448 race layout: left items keep their x, right items keep
  // their distance from the right edge, centred items stay centred; items in
  // the lower half keep their distance from the bottom.
  drawRace(race, frame, rect, dpr, topInset) {
    // Phones keep a 480-unit minimum width (the gaps between the top items
    // allow it) and drop the right column below the compact speed meter.
    const compact = rect.width < 560, R = this.race, unit = Math.min(rect.height / 448, rect.width / 480);
    const width = rect.width / unit, height = rect.height / unit, top = topInset / unit, meter = compact ? 118 / unit : 0;
    const ax = x => x <= 213 ? x : x >= 427 ? width - (640 - x) : x + (width - 640) / 2 - (compact ? 30 : 0);
    const ay = (y, x = 0) => y < 224 ? y + top + (x >= 427 ? meter : 0) : height - (448 - y);
    const at = (x, y) => [ax(x), ay(y, x)];
    const c = this.ctx, saved = this.logicalWidth; this.logicalWidth = width;
    c.save(); c.setTransform(unit * dpr, 0, 0, unit * dpr, 0, 0);
    const L = n => R.layout.get(n), hud = race.hud ?? {};
    const text = (value, where, layout, i, options = {}) => this.text(value, where, layout, i, { color: options.color ?? layout[`text${i}color`] ?? raceText, ...options });
    // LAP TIME, LAP, SCORE.
    text('LAP TIME', at(609, 23), L(1), 1); text(overlayTime(race.lap_time), at(609, 23), L(1), 2);
    text('LAP', at(354, 23), L(2), 1); text(`${race.lap} / ${race.laps}`, at(354, 23), L(2), 2);
    text('SCORE', at(178, 23), L(3), 1); text(String(Math.floor((frame?.stunt_score?.committed_points ?? 0) + .5)), at(178, 23), L(3), 2);
    // PLACE position tree: each rider slot's name slides to its rank.
    const tree = L(4), names = ['YOU', ...race.opponents.map(o => o.name)].map(n => String(n).slice(0, 9));
    text('PLACE', at(20, 23), tree, 1);
    for (let rank = 1; rank <= Math.min(5, names.length); rank++) {
      const player = race.position === rank;
      text(String(rank), at(20, 23), tree, rank + 1, { color: player ? raceGold : raceText, scale: player ? hud.place_scale ?? 1 : 1 });
    }
    names.slice(0, 5).forEach((name, slot) => text(name, at(20, 23), tree, 7, { color: slot === 0 ? raceGold : raceText, offset: [0, hud.tree?.[slot] ?? (race.ranks?.[slot] - 1) * 23] }));
    // BEST/TOTAL/LEAD, faded by the lap history; LEAD green ahead, red behind.
    const listing = L(7), alpha = 255 * (hud.listing_alpha ?? 1);
    if (alpha > 0) {
      ['BEST:', 'TOTAL:', 'LEAD:'].forEach((label, i) => text(label, at(609, 23), listing, i + 1, { alpha }));
      text(overlayTime(race.best ?? 0), at(609, 23), listing, 4, { alpha });
      text(overlayTime(race.elapsed), at(609, 23), listing, 5, { alpha });
      text(overlayTime(race.lead), at(609, 23), listing, 6, { alpha, color: race.lead >= 0 ? '30,215,30' : '200,60,60' });
    }
    if (hud.history) {
      const history = L(14), where = [ax(609) + hud.history.offset, ay(23, 609)];
      hud.history.rows.forEach((row, i) => {
        const color = row.best ? (history.bestlapcolor ?? '76,181,255').replace(/\s/g, '') : raceText;
        text(`Lap ${row.lap}`, where, history, i * 2 + 1, { color }); text(overlayTime(row.time), where, history, i * 2 + 2, { color });
      });
    }
    // Infotext and signage.
    if (hud.infotext) text(hud.infotext, at(320, 304), L(11), 1);
    const sign = hud.sign && R.signs.get(hud.sign.id);
    if (sign) {
      // Phones show signs below the stacked top items.
      const where = at(320, 80); if (compact) where[1] += meter + 120; this.icon(where, sign, 1);
      const counted = hud.sign.digit != null;
      if (counted) text(String(hud.sign.digit), where, sign, 1);
      text(unquote(sign[`text${counted ? 2 : 1}text`]), where, sign, counted ? 2 : 1);
    }
    // Radar: north-up course map at native size, racer dots drawn last-first.
    const { map, left, top: north, right } = R.radar, scale = (right - left) / map.width, origin = at(20, 305);
    c.drawImage(map, origin[0], origin[1], map.width, map.height);
    const dot = (p, rgb) => {
      const x = Math.max(0, Math.min(map.width - 8, Math.trunc((p[0] - left) / scale))), y = Math.max(0, Math.min(map.height - 8, Math.trunc((north - p[2]) / scale)));
      c.drawImage(rgb ? this.tinted(R.racer, rgb) : R.racer, origin[0] + x, origin[1] + y, 8, 8);
    };
    for (const cpu of [...race.opponents].reverse()) dot(cpu.origin, null);
    if (frame?.origin) dot(frame.origin, '255,0,0');
    c.restore(); this.logicalWidth = saved;
  }
  clear() {
    if (!this.painted) return;
    this.clears++; this.painted = false;
    this.ctx.save(); this.ctx.setTransform(1, 0, 0, 1, 0, 0);
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height); this.ctx.restore();
  }
  text(text, at, layout, index, { scale = 1, color, alpha, offset = [0, 0], justify } = {}) {
    const prefix = `text${index}`, name = unquote(layout[`${prefix}font`]) || fontDefault;
    const font = this.frontend.fonts.get(name.toLowerCase()); if (!font) return;
    color ??= layout[`${prefix}color`] || '255,255,255';
    // Preserve original placement and advances, but rasterize vector type at
    // final display resolution rather than enlarging the source glyph atlas.
    const glyphs=[...String(text)].map(c=>font.glyphs.get(c.charCodeAt(0))).filter(Boolean);
    const width=glyphs.reduce((sum,g)=>sum+(g.metrics ? g.metrics[0]+g.metrics[2] : (g.uv[2]-g.uv[0])*font.image.width),0);
    const height=Math.max(1,...glyphs.map(g=>g.metrics?.[3]??25));
    const position = numbers(layout[`${prefix}offset`]) || [0, 0];
    const alignment = (justify ?? layout[`${prefix}justification`] ?? 'L').toUpperCase();
    scale *= Number(layout[`${prefix}scale`] ?? 1);
    // Desktop uses original dimensions. Compact phones preserve readable glyphs
    // and keep center text inside the viewport without changing simulation data.
    scale = Math.min(scale, (this.logicalWidth - 20) / Math.max(width, 1));
    const c = this.ctx; c.save(); c.translate(at[0] + position[0] + offset[0], at[1] + position[1] + offset[1]); c.scale(scale, scale);
    c.globalAlpha = (alpha ?? Number(layout[`${prefix}alpha`] ?? 255)) / 255;
    let x = alignment === 'R' ? -width : alignment === 'H' || alignment === 'B' ? -width/2 : 0;
    const y = alignment === 'V' || alignment === 'B' ? -height/2 : 0;
    c.drawText(String(text),x,y,width,height,color);
    c.restore();
  }
  icon(at, layout, index, { visible = true, fill, alpha } = {}) {
    if (!visible) return;
    const prefix = `icon${index}`, image = this.images.get(unquote(layout[`${prefix}texture`])); if (!image) return;
    const offset = numbers(layout[`${prefix}offset`]) || [0, 0];
    const scale = Number(layout[`${prefix}scale`] ?? 1);
    const width = Number(layout[`${prefix}width`] ?? image.width) * scale;
    let height = Number(layout[`${prefix}height`] ?? image.height) * scale, y = at[1] + offset[1];
    // 002e2290 stretches the entire orange image upward from its original
    // bottom edge. Clipping its UVs would change the original meter gradient.
    if (fill !== undefined) { const full = height; height *= fill; y += full - height; }
    const c = this.ctx; c.save(); c.globalAlpha = (alpha ?? Number(layout[`${prefix}alpha`] ?? 255)) / 255;
    if (height > 0 && width > 0) c.drawImage(image, at[0] + offset[0], y, width, height);
    c.restore();
  }
  release() {
    if (this.canvas.width !== 1 || this.canvas.height !== 1) this.canvas.width = this.canvas.height = 1;
    if (!this.canvas.hidden) this.canvas.hidden = true;
    this.painted = false; this.lastKey = '';
  }
  snapshot() { return { draws: this.draws, clears: this.clears, backingBytes: this.canvas.width * this.canvas.height * 4 }; }
  draw(hud, visible = true, { speedVisible = true, lights = null, race = null, frame = null, topInset = 0 } = {}) {
    if (!this.race) race = null;
    if (!this.lamps) lights = null;
    if (hud) {
      this.canvas.dataset.renderer = 'rust'; this.canvas.dataset.speed = hud.speed_text;
      this.canvas.dataset.preload = String(hud.preload_visible); this.canvas.dataset.stunt = hud.stunt_visible ? hud.stunt_name : '';
    }
    this.canvas.dataset.speedVisible = String(speedVisible);
    if (!this.ready || !visible || !hud || (!speedVisible && !hud.stunt_visible && lights == null && !race)) { this.release(); return; }
    this.surfaceSize ??= observedSize(this.surface);
    const rect = this.surfaceSize, dpr = Math.min(devicePixelRatio || 1, 3);
    const width = Math.round(rect.width * dpr), height = Math.round(rect.height * dpr);
    if (width <= 0 || height <= 0) { this.release(); return; }
    const key = JSON.stringify([hud, speedVisible, lights, race && { ...race, opponents: race.opponents.map(o => [o.name, o.origin]), standings: null }, race && frame?.origin, race && frame?.stunt_score?.committed_points, topInset, rect.width, rect.height, dpr]);
    if (key === this.lastKey) return;
    const actual = this.ctx, list = new HudDrawList(); this.ctx = list;
    // Original logical frame is 640x448. Extend horizontal framing on wide
    // displays. Compact portrait phones move the meter above thumb driving.
    const compact = rect.width < 560, unit = compact ? Math.min(rect.width / 400, rect.height / 448) : rect.height / 448;
    this.logicalWidth = rect.width / unit; const logicalHeight = rect.height / unit;
    try {
      const c = this.ctx; c.setTransform(unit * dpr, 0, 0, unit * dpr, 0, 0);
      const speedAt = compact ? [this.logicalWidth - 24, 72] : [this.logicalWidth - 43, logicalHeight - 76];
      const speed = this.layout.get('OverlayObject-5');
      // Native draw depth: background (5), blue (4), orange (2), highlight (1).
      if (speedVisible) {
        this.icon(speedAt, speed, 2); this.icon(speedAt, speed, 1, { visible: hud.preload_visible, alpha: hud.preload_blue_alpha });
        this.icon(speedAt, speed, 4, { visible: hud.preload_visible, fill: hud.preload_fill }); this.icon(speedAt, speed, 3);
        this.text(hud.speed_text, speedAt, speed, 1); this.text(hud.units, speedAt, speed, 2);
      }
      this.canvas.dataset.speedVisible = String(speedVisible);
      this.canvas.dataset.lights = lights == null ? '' : String(lights);
      this.canvas.dataset.race = race ? (race.hud?.sign?.id ?? 'on') : '';
      if (race) this.drawRace(race, frame, rect, dpr, topInset);
      if (lights != null) {
        // Four 32x32 lamps from (256,208) in 640x448, centred on wide screens:
        // three red lamps light one per second, then the green lamp at GO.
        const left = this.logicalWidth / 2 - 64, top = logicalHeight * 208 / 448;
        for (let lamp = 0; lamp < 4; lamp++) {
          const name = lamp < 3 ? (lights > lamp ? '321ron' : '321roff') : (lights >= 4 ? '321gon' : '321goff');
          this.ctx.drawImage(this.lamps[name], left + lamp * 32, top, 32, 32);
        }
      }
      if (hud.stunt_visible) {
        const stunt = this.layout.get('OverlayObject-12'), at = [this.logicalWidth / 2, compact ? logicalHeight * .62 : logicalHeight - 100];
        const common = { color: hud.stunt_color.join(','), alpha: hud.stunt_alpha };
        this.text(hud.stunt_name, at, stunt, 1, { ...common, offset: hud.stunt_offsets[0] });
        this.text(hud.stunt_points, at, stunt, 2, { ...common, scale: hud.stunt_scale, offset: hud.stunt_offsets[1] });
      }
    } finally { this.ctx = actual; }
    if (!list.commands.length) { this.release(); return; }
    const bounds = [Infinity, Infinity, -Infinity, -Infinity];
    for (const command of list.commands) {
      bounds[0] = Math.min(bounds[0], command.bounds[0]); bounds[1] = Math.min(bounds[1], command.bounds[1]);
      bounds[2] = Math.max(bounds[2], command.bounds[2]); bounds[3] = Math.max(bounds[3], command.bounds[3]);
    }
    // Retain the original raster origin: shifting fractional glyph transforms
    // changes antialiased edge rounding in Canvas2D. Trim only unused right/bottom
    // space for speed; animated stunts retain their original full backing size.
    const left = 0, top = 0;
    const full = hud.stunt_visible || race;
    const right = full ? width : Math.min(width, Math.ceil((bounds[2] + 2) / 64) * 64), bottom = full ? height : Math.min(height, Math.ceil((bounds[3] + 2) / 64) * 64);
    if (right <= left || bottom <= top) { this.release(); return; }
    this.canvas.hidden = false;
    Object.assign(this.canvas.style, { inset: `${top / dpr}px auto auto ${left / dpr}px`, width: `${(right-left) / dpr}px`, height: `${(bottom-top) / dpr}px` });
    if (this.canvas.width !== right-left || this.canvas.height !== bottom-top) { this.canvas.width = right-left; this.canvas.height = bottom-top; this.painted = false; }
    this.clear();
    for (const [operation, ...args] of list.operations) {
      if (operation === 'setTransform') actual.setTransform(...args.slice(0, 4), args[4]-left, args[5]-top);
      else if (operation === 'text') {
        const [text,x,y,w,h,color]=args;
        actual.save(); actual.font=`italic 800 ${h}px ${this.displayFont ??= getComputedStyle(document.documentElement).getPropertyValue('--atv-font-display').trim() || 'sans-serif'}`;
        actual.textBaseline='top'; actual.fillStyle=`rgb(${color})`; actual.strokeStyle='#080808bb'; actual.lineWidth=2;
        actual.lineJoin='round'; actual.strokeText(text,x,y,w); actual.fillText(text,x,y,w); actual.restore();
      }
      else if (operation === 'alpha') actual.globalAlpha = args[0];
      else actual[operation](...args);
    }
    this.painted = true; this.lastKey = key; this.draws++;
  }
}
