// Draw original runtime HUD layouts with crisp HD text. All changing values and animation
// clocks arrive from authored Rust; this bridge never derives scores from input.
import { courseResource } from './launch.js';
const unquote = value => (value ?? '').replace(/^"/, '').replace(/"$/, '');
const numbers = value => value?.split(',').map(s => Number(s.trim()));
const fontDefault = 'tradegothic_boldcondensed_22s';

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
  async init(environment) {
    this.ready = false; this.lastKey = ''; this.clear();
    const f = this.frontend;
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
    this.ready = true;
    // The Rust controller parses the same original section. Keep resource bytes
    // out of source code and preserve the exact build's authoring overrides.
    const original = await f.data('Overlays.pb'), text = new TextDecoder('latin1').decode(original);
    const headers = [...text.matchAll(/^\[([^\]\r\n]+)\][^\S\r\n]*\r?$/gm)];
    const index = headers.findIndex(m => m[1].toLowerCase() === 'overlayobject-12');
    return original.slice(headers[index].index, headers[index + 1]?.index ?? original.length);
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
  draw(hud, visible = true, { speedVisible = true } = {}) {
    if (hud) {
      this.canvas.dataset.renderer = 'rust'; this.canvas.dataset.speed = hud.speed_text;
      this.canvas.dataset.preload = String(hud.preload_visible); this.canvas.dataset.stunt = hud.stunt_visible ? hud.stunt_name : '';
    }
    this.canvas.dataset.speedVisible = String(speedVisible);
    if (!this.ready || !visible || !hud || (!speedVisible && !hud.stunt_visible)) { this.release(); return; }
    const rect = this.surface.getBoundingClientRect(), dpr = Math.min(devicePixelRatio || 1, 3);
    const width = Math.round(rect.width * dpr), height = Math.round(rect.height * dpr);
    if (width <= 0 || height <= 0) { this.release(); return; }
    const key = JSON.stringify([hud, speedVisible, rect.width, rect.height, dpr]);
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
    const right = hud.stunt_visible ? width : Math.min(width, Math.ceil((bounds[2] + 2) / 64) * 64), bottom = hud.stunt_visible ? height : Math.min(height, Math.ceil((bounds[3] + 2) / 64) * 64);
    if (right <= left || bottom <= top) { this.release(); return; }
    this.canvas.hidden = false;
    Object.assign(this.canvas.style, { inset: `${top / dpr}px auto auto ${left / dpr}px`, width: `${(right-left) / dpr}px`, height: `${(bottom-top) / dpr}px` });
    if (this.canvas.width !== right-left || this.canvas.height !== bottom-top) { this.canvas.width = right-left; this.canvas.height = bottom-top; this.painted = false; }
    this.clear();
    for (const [operation, ...args] of list.operations) {
      if (operation === 'setTransform') actual.setTransform(...args.slice(0, 4), args[4]-left, args[5]-top);
      else if (operation === 'text') {
        const [text,x,y,w,h,color]=args;
        actual.save(); actual.font=`italic 800 ${h}px "Arial Narrow", "Helvetica Neue", Arial, sans-serif`;
        actual.textBaseline='top'; actual.fillStyle=`rgb(${color})`; actual.strokeStyle='#080808bb'; actual.lineWidth=2;
        actual.lineJoin='round'; actual.strokeText(text,x,y,w); actual.fillText(text,x,y,w); actual.restore();
      }
      else if (operation === 'alpha') actual.globalAlpha = args[0];
      else actual[operation](...args);
    }
    this.painted = true; this.lastKey = key; this.draws++;
  }
}
