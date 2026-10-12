// Draw original runtime HUD layouts/glyphs. All changing values and animation
// clocks arrive from authored Rust; this bridge never derives scores from input.
const unquote = value => (value ?? '').replace(/^"/, '').replace(/"$/, '');
const numbers = value => value?.split(',').map(s => Number(s.trim()));
const fontDefault = 'tradegothic_boldcondensed_22s';

export class HeadUpDisplay {
  constructor(frontend, surface = document.getElementById('game')) {
    this.frontend = frontend; this.surface = surface; this.ready = false;
    this.canvas = document.createElement('canvas'); this.canvas.id = 'native-hud';
    this.canvas.setAttribute('aria-hidden', 'true');
    Object.assign(this.canvas.style, { position: 'absolute', inset: '0', width: '100%', height: '100%', pointerEvents: 'none', zIndex: '2' });
    surface.parentElement.append(this.canvas); this.ctx = this.canvas.getContext('2d');
    this.layout = new Map(); this.images = new Map(); this.tints = new Map();
  }
  async init(environment) {
    this.ready = false; this.clear();
    const env = /^env(0[1-8])$/i.exec(environment)?.[1];
    if (!env) throw new Error('Unsupported original HUD environment');
    const f = this.frontend;
    // Each authorized Freestyle runtime archive contains the original HUD art
    // and layout. Reuse the resource cache also used by the frontend.
    await f.archive('/STARTEVT.RES');
    await f.archive(`/TRACKS/TRACKSA/ENV${env}/E${env}_FR.RES`);
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
    this.ctx.save(); this.ctx.setTransform(1, 0, 0, 1, 0, 0);
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height); this.ctx.restore();
  }
  text(text, at, layout, index, { scale = 1, color, alpha, offset = [0, 0], justify } = {}) {
    const prefix = `text${index}`, name = unquote(layout[`${prefix}font`]) || fontDefault;
    const font = this.frontend.fonts.get(name.toLowerCase()); if (!font) return;
    color ??= layout[`${prefix}color`] || '255,255,255';
    const key = `${name}:${color}`; let atlas = this.tints.get(key);
    if (!atlas) {
      atlas = document.createElement('canvas'); atlas.width = font.image.width; atlas.height = font.image.height;
      const c = atlas.getContext('2d'); c.drawImage(font.image, 0, 0);
      const pixels = c.getImageData(0, 0, atlas.width, atlas.height), tint = numbers(color);
      for (let p = 0; p < pixels.data.length; p += 4) for (let ch = 0; ch < 3; ch++) pixels.data[p+ch] = pixels.data[p+ch] * tint[ch] / 255;
      c.putImageData(pixels, 0, 0); this.tints.set(key, atlas);
      // Failure colors change every tick. Bound tinted atlas memory across runs.
      if (this.tints.size > 96) this.tints.delete(this.tints.keys().next().value);
    }
    const glyphs = [...String(text)].map(c => font.glyphs.get(c.charCodeAt(0))).filter(Boolean);
    const advance = g => g.metrics ? g.metrics[0] + g.metrics[2] : (g.uv[2] - g.uv[0]) * atlas.width;
    const width = glyphs.reduce((a, g) => a + advance(g), 0), height = Math.max(0, ...glyphs.map(g => g.metrics?.[3] ?? (g.uv[3] - g.uv[1]) * atlas.height));
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
    for (const g of glyphs) {
      const [l, t, r, b] = g.uv, m = g.metrics;
      const sx = Math.round(l * (atlas.width - 1)), sy = Math.round(t * (atlas.height - 1));
      const w = m?.[1] ?? (r - l) * atlas.width, h = m?.[3] ?? (b - t) * atlas.height;
      c.drawImage(atlas, sx, sy, w, h, x, y, w, h); x += advance(g);
    }
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
  draw(hud, visible = true) {
    const rect = this.surface.getBoundingClientRect(), dpr = Math.min(devicePixelRatio || 1, 3);
    const width = Math.round(rect.width * dpr), height = Math.round(rect.height * dpr);
    if (this.canvas.width !== width || this.canvas.height !== height) { this.canvas.width = width; this.canvas.height = height; }
    this.clear(); if (!this.ready || !visible || !hud || width <= 0 || height <= 0) return;
    // Original logical frame is 640x448. Extend horizontal framing on wide
    // displays. Compact portrait phones move the meter above thumb driving.
    const compact = rect.width < 560, unit = compact ? Math.min(rect.width / 400, rect.height / 448) : rect.height / 448;
    this.logicalWidth = rect.width / unit; const logicalHeight = rect.height / unit;
    const c = this.ctx; c.setTransform(unit * dpr, 0, 0, unit * dpr, 0, 0);
    const speedAt = compact ? [this.logicalWidth - 24, 72] : [this.logicalWidth - 43, logicalHeight - 76];
    const speed = this.layout.get('OverlayObject-5');
    // Native draw depth: background (5), blue (4), orange (2), highlight (1).
    this.icon(speedAt, speed, 2); this.icon(speedAt, speed, 1, { visible: hud.preload_visible, alpha: hud.preload_blue_alpha });
    this.icon(speedAt, speed, 4, { visible: hud.preload_visible, fill: hud.preload_fill }); this.icon(speedAt, speed, 3);
    this.text(hud.speed_text, speedAt, speed, 1); this.text(hud.units, speedAt, speed, 2);
    if (hud.stunt_visible) {
      const stunt = this.layout.get('OverlayObject-12'), at = [this.logicalWidth / 2, compact ? logicalHeight * .62 : logicalHeight - 100];
      const common = { color: hud.stunt_color.join(','), alpha: hud.stunt_alpha };
      this.text(hud.stunt_name, at, stunt, 1, { ...common, offset: hud.stunt_offsets[0] });
      this.text(hud.stunt_points, at, stunt, 2, { ...common, scale: hud.stunt_scale, offset: hud.stunt_offsets[1] });
    }
    this.canvas.dataset.renderer = 'rust'; this.canvas.dataset.speed = hud.speed_text;
    this.canvas.dataset.preload = String(hud.preload_visible); this.canvas.dataset.stunt = hud.stunt_visible ? hud.stunt_name : '';
  }
}
