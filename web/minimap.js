// Authored GPS cartography: orthographic +Z-up terrain/scenery, never a second
// live scene render. Source pixels stay in memory and outside the source tree.
export const MAP_RESOLUTION = 1024;
export const MAP_RANGE = 768; // feet across, independent of HUD size
export const MAP_INTERVAL = 50;
export const clampMapSize = value => Number.isFinite(Number(value)) ? Math.round(Math.max(96, Math.min(224, Number(value))) / 8) * 8 : 160;
export const mapHeading = ([w, x, y, z]) => Math.atan2(2 * (w * y + x * z), 1 - 2 * (y * y + x * x));
export const mapPoint = (x, z, terrain, width, height) => [x / terrain.sizeX * width, (1 - z / terrain.sizeZ) * height];

export class MinimapSettings {
  constructor(storage) {
    this.storage = storage; this.enabled = true; this.size = 160; this.changed = () => {};
    try { this.storage ??= globalThis.localStorage; storage = this.storage; this.enabled = storage.getItem('atv-minimap-enabled') !== 'false'; const value = storage.getItem('atv-minimap-size'); if (value !== null) this.size = clampMapSize(value); } catch { /* Session fallback. */ }
  }
  save() { try { this.storage.setItem('atv-minimap-enabled', String(this.enabled)); this.storage.setItem('atv-minimap-size', String(this.size)); } catch { /* Session fallback. */ } this.changed(); }
  toggle() { this.enabled = !this.enabled; this.save(); }
  setSize(value) { this.size = clampMapSize(value); this.save(); }
}

// Match the terrain shader's 64-cell tile lookup and inset UVs. Store only the
// tiny tile lookup; original decoded texture buffers are shared with the scene.
export function terrainTiles(terrain) {
  const width = Math.ceil((terrain.width - 1) / 64), height = Math.ceil((terrain.depth - 1) / 64);
  const layers = new Uint16Array(width * height);
  for (const region of terrain.textures?.layout.regions ?? []) for (let i = 0; i < 16; i++) {
    const x = region.x / 64 + i % 4, z = region.z / 64 + Math.floor(i / 4);
    layers[z * width + x] = region.subtextures[i];
  }
  return { width, layers };
}

export function rasterRow(terrain, tiles, pixels, row, width, height) {
  const { heights, spacing } = terrain;
  const gz = (1 - (row + .5) / height) * (terrain.depth - 1), iz = Math.floor(gz);
  for (let col = 0; col < width; col++) {
    const gx = (col + .5) / width * (terrain.width - 1), ix = Math.floor(gx);
    const h = (x, z) => heights[Math.max(0, Math.min(terrain.depth - 1, z)) * terrain.width + Math.max(0, Math.min(terrain.width - 1, x))];
    const dx = (h(ix + 1, iz) - h(ix - 1, iz)) / (2 * spacing), dz = (h(ix, iz + 1) - h(ix, iz - 1)) / (2 * spacing);
    const light = Math.max(.42, Math.min(1.15, .8 + (-dx * .4 + dz * .3 + .6) / Math.hypot(dx, dz, 1) * .35));
    const tileX = Math.floor(gx / 64), tileZ = Math.floor(gz / 64);
    const image = terrain.textures?.images[tiles.layers[tileZ * tiles.width + tileX]];
    const tx = image ? Math.min(image.width - 1, Math.floor(((gx / 64 % 1) * .9375 + .03125) * image.width)) : 0;
    const tz = image ? Math.min(image.height - 1, Math.floor(((gz / 64 % 1) * .9375 + .03125) * image.height)) : 0;
    const source = (tz * (image?.width ?? 1) + tx) * 4, dest = (row * width + col) * 4;
    for (let ch = 0; ch < 3; ch++) pixels[dest + ch] = Math.round((image?.rgba[source + ch] ?? [104, 110, 91][ch]) * light * .82);
    pixels[dest + 3] = 255;
  }
}

function* sceneryTriangles(terrain) {
  const models = [...(terrain.world?.metadata.models ?? [])].sort((a, b) => a.world_matrix[13] - b.world_matrix[13]);
  for (const model of models) {
    const mesh = terrain.world.meshes.get(model.name), m = model.world_matrix;
    for (const group of mesh?.lods[0]?.groups ?? []) {
      const material = terrain.world.textures.get(mesh.materials[group.material]);
      if (material?.pass?.reflection_plane || material?.pass?.blend) continue;
      const color = material?.rgba;
      // Use a sampled source material average, keeping roofs and ramps legible.
      let r = 0, g = 0, b = 0, count = 0;
      if (color) for (let p = 0; p < color.length; p += 1024) { r += color[p]; g += color[p + 1]; b += color[p + 2]; count++; }
      const fill = count ? `rgb(${r / count * .72},${g / count * .72},${b / count * .72})` : '#78766c';
      for (let i = 0; i < group.indices.length; i += 3) {
        const points = group.indices.slice(i, i + 3).map(index => {
          const [x, y, z] = group.vertices[index].position;
          return mapPoint(m[0] * x + m[4] * y + m[8] * z + m[12], m[2] * x + m[6] * y + m[10] * z + m[14], terrain, MAP_RESOLUTION, MAP_RESOLUTION);
        });
        yield { points, fill };
      }
    }
  }
}

export class FreestyleMinimap {
  constructor(settings, parent = document.body) {
    this.settings = settings; this.terrain = null; this.updated = -Infinity;
    this.root = document.createElement('div'); this.root.id = 'freestyle-minimap'; this.root.hidden = true;
    this.canvas = document.createElement('canvas'); this.canvas.setAttribute('aria-label', 'Freestyle GPS map, north up'); this.canvas.setAttribute('role', 'img');
    this.label = document.createElement('span'); this.label.textContent = 'GPS'; this.label.setAttribute('aria-hidden', 'true');
    this.root.append(this.canvas, this.label); parent.append(this.root); this.ctx = this.canvas.getContext('2d');
    this.metrics = { updates: 0, builds: 0, buildMs: 0, buildSlices: 0, maxBuildSliceMs: 0, lastCpuMs: 0, totalCpuMs: 0, backingBytes: 0, ready: false };
    settings.changed = () => { this.root.style.setProperty('--atv-minimap-size', `${settings.size}px`); this.updated = -Infinity; if (!settings.enabled) this.release(); };
    settings.changed();
    const css = getComputedStyle(this.root);
    this.colors = { ink: css.getPropertyValue('--atv-ink').trim(), gold: css.getPropertyValue('--atv-gold-light').trim(), silver: css.getPropertyValue('--atv-silver-light').trim(), font: css.getPropertyValue('--atv-font-label').trim() };
  }
  release() {
    if (this.base) this.base.width = this.base.height = 1;
    if (this.build) this.build.base.width = this.build.base.height = 1;
    this.root.hidden = true; this.base = null; this.build = null; this.canvas.width = this.canvas.height = 1;
    this.metrics.backingBytes = 0; this.metrics.ready = false; this.updated = -Infinity;
  }
  setTerrain(terrain) { this.release(); this.terrain = terrain; this.metrics.builds = this.metrics.buildMs = this.metrics.buildSlices = this.metrics.maxBuildSliceMs = 0; }
  buildSlice() {
    const start = performance.now();
    if (!this.build) {
      this.metrics.builds++; this.metrics.buildMs = this.metrics.buildSlices = this.metrics.maxBuildSliceMs = 0;
      const base = document.createElement('canvas'); base.width = base.height = MAP_RESOLUTION;
      const ctx = base.getContext('2d');
      this.build = { base, ctx, pixels: ctx.createImageData(MAP_RESOLUTION, MAP_RESOLUTION), tiles: terrainTiles(this.terrain), row: 0, scenery: null };
    }
    const build = this.build;
    if (build.row < MAP_RESOLUTION) {
      do { rasterRow(this.terrain, build.tiles, build.pixels.data, build.row++, MAP_RESOLUTION, MAP_RESOLUTION); } while (build.row < MAP_RESOLUTION && performance.now() - start < 2);
      if (build.row === MAP_RESOLUTION) { build.ctx.putImageData(build.pixels, 0, 0); build.pixels = null; build.scenery = sceneryTriangles(this.terrain); }
    } else {
      let next;
      do {
        next = build.scenery.next();
        if (next.done) { this.base = build.base; this.build = null; this.metrics.ready = true; break; }
        const { points, fill } = next.value, c = build.ctx;
        c.fillStyle = fill; c.beginPath(); points.forEach(([x, y], i) => i ? c.lineTo(x, y) : c.moveTo(x, y)); c.closePath(); c.fill();
      } while (performance.now() - start < 2);
    }
    const elapsed = performance.now() - start;
    this.metrics.buildMs += elapsed; this.metrics.buildSlices++; this.metrics.maxBuildSliceMs = Math.max(this.metrics.maxBuildSliceMs, elapsed);
    this.metrics.backingBytes = MAP_RESOLUTION * MAP_RESOLUTION * (build.pixels ? 8 : 4);
  }
  draw(time, player, visible) {
    if (!this.settings.enabled || !visible || !this.terrain || !player) { if (!this.root.hidden) this.root.hidden = true; return; }
    if (!this.base) { this.buildSlice(); return; }
    if (this.root.hidden) this.root.hidden = false;
    if (time - this.updated < MAP_INTERVAL) return;
    this.updated = time; const started = performance.now();
    const rect = this.root.getBoundingClientRect(), size = rect.width, dpr = Math.min(devicePixelRatio || 1, 1.5), pixels = Math.max(1, Math.round(size * dpr));
    if (this.canvas.width !== pixels) this.canvas.width = this.canvas.height = pixels;
    const c = this.ctx, t = this.terrain, [x, , z] = player.origin;
    c.setTransform(pixels / size, 0, 0, pixels / size, 0, 0); c.clearRect(0, 0, size, size);
    c.fillStyle = this.colors.ink; c.fillRect(0, 0, size, size);
    const [px, py] = mapPoint(x, z, t, MAP_RESOLUTION, MAP_RESOLUTION), sx = MAP_RANGE / t.sizeX * MAP_RESOLUTION, sy = MAP_RANGE / t.sizeZ * MAP_RESOLUTION;
    c.drawImage(this.base, px - sx / 2, py - sy / 2, sx, sy, 0, 0, size, size);
    // A small pale/gold heading marker follows the quad, including wrecks/reset.
    c.save(); c.translate(size / 2, size / 2); c.rotate(mapHeading(player.orientation));
    c.beginPath(); c.moveTo(0, -8); c.lineTo(6, 6); c.lineTo(0, 3); c.lineTo(-6, 6); c.closePath();
    c.lineWidth = 3; c.strokeStyle = this.colors.ink; c.stroke(); c.fillStyle = this.colors.gold; c.fill(); c.restore();
    c.font = `italic 800 10px ${this.colors.font}`; c.lineWidth = 3; c.strokeStyle = this.colors.ink; c.fillStyle = this.colors.silver;
    c.strokeText('N', size / 2 - 4, 13); c.fillText('N', size / 2 - 4, 13);
    const length = size * 200 / MAP_RANGE;
    c.strokeStyle = this.colors.silver; c.lineWidth = 1.5; c.beginPath(); c.moveTo(9, size - 12); c.lineTo(9 + length, size - 12); c.stroke();
    c.font = `700 9px ${this.colors.font}`; c.strokeStyle = this.colors.ink; c.lineWidth = 3; c.strokeText('200 ft', 9, size - 17); c.fillText('200 ft', 9, size - 17);
    this.metrics.lastCpuMs = performance.now() - started; this.metrics.totalCpuMs += this.metrics.lastCpuMs; this.metrics.updates++;
    this.metrics.center = [x, z]; this.metrics.heading = mapHeading(player.orientation);
    this.metrics.backingBytes = (MAP_RESOLUTION * MAP_RESOLUTION + pixels * pixels) * 4;
  }
  snapshot() { return { ...this.metrics, enabled: this.settings.enabled, size: this.settings.size, visible: !this.root.hidden, resolution: this.canvas.width, rangeFeet: MAP_RANGE, maxHz: 1000 / MAP_INTERVAL, extraSceneDraws: 0 }; }
}
