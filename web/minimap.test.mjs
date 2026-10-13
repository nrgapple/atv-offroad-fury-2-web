import test from 'node:test';
import assert from 'node:assert/strict';
import { MinimapSettings, mapHeading, mapPoint, terrainTiles, rasterRow, instrumentFooter } from './minimap.js';

test('footer lettering and black outline fit the circular edge at minimum and maximum sizes', () => {
  for (const size of [96, 160, 224]) for (const width of [size * .3, size * .55]) {
    const descent = 3, baseline = instrumentFooter(size, 'circle', width, descent);
    const radius = size / 2;
    assert((width / 2 + 2) ** 2 + (baseline + descent + 2 - radius) ** 2 < radius ** 2);
    assert(baseline > size * .8, 'Speed stays near the bottom edge');
    assert.equal(instrumentFooter(size, 'square', width, descent), size - 7);
  }
});

test('north-up projection and quaternion heading agree at course boundaries', () => {
  const terrain = { sizeX: 100, sizeZ: 200 };
  assert.deepEqual(mapPoint(0, 200, terrain, 400, 400), [0, 0]);
  assert.deepEqual(mapPoint(100, 0, terrain, 400, 400), [400, 400]);
  assert.deepEqual(mapPoint(50, 100, terrain, 400, 400), [200, 200]);
  assert.equal(mapHeading([1, 0, 0, 0]), 0);
  assert(Math.abs(mapHeading([Math.SQRT1_2, 0, Math.SQRT1_2, 0]) - Math.PI / 2) < 1e-12);
});

test('saved Off and clamped sizes survive reload; blocked storage still allows session changes', () => {
  const entries = new Map(), storage = { getItem: key => entries.get(key) ?? null, setItem: (key, value) => entries.set(key, value) };
  const settings = new MinimapSettings(storage);
  assert.equal(settings.enabled, true); assert.equal(settings.size, 160);
  assert.equal(settings.shape, 'circle'); assert.equal(settings.corner, 'top-right');
  assert.equal(settings.orientation, 'north-up'); settings.toggleOrientation();
  settings.toggleShape(); settings.toggleCorner();
  settings.toggle(); settings.setSize(900);
  const reloaded = new MinimapSettings(storage);
  assert.equal(reloaded.enabled, false); assert.equal(reloaded.size, 224);
  assert.equal(reloaded.shape, 'square'); assert.equal(reloaded.corner, 'bottom-left');
  assert.equal(reloaded.orientation, 'heading-up');
  reloaded.setSize('bad'); assert.equal(reloaded.size, 160);
  const blocked = new MinimapSettings({ getItem() { throw Error(); }, setItem() { throw Error(); } });
  blocked.toggle(); blocked.setSize(96);
  blocked.toggleShape(); blocked.toggleCorner();
  assert.equal(blocked.enabled, false); assert.equal(blocked.size, 96);
  assert.equal(blocked.shape, 'square'); assert.equal(blocked.corner, 'bottom-left');
});

test('orthographic terrain uses the actual 64-cell texture mapping and positive Z at the top', () => {
  const solid = value => ({ width: 1, height: 1, rgba: new Uint8Array([value, 0, 0, 255]) });
  const terrain = { width: 257, depth: 257, spacing: 1, sizeX: 256, sizeZ: 256, heights: new Float32Array(257 * 257), textures: { images: Array.from({ length: 16 }, (_, i) => solid((i + 1) * 10)), layout: { regions: [{ x: 0, z: 0, subtextures: Array.from({ length: 16 }, (_, i) => i) }] } } };
  const tiles = terrainTiles(terrain), pixels = new Uint8ClampedArray(4 * 4 * 4);
  for (let row = 0; row < 4; row++) rasterRow(terrain, tiles, pixels, row, 4, 4);
  assert(pixels[0] > pixels[3 * 4 * 4], 'Northern material appears above southern material');
  assert(pixels[4] > pixels[0], 'East material appears to the right');
  for (let i = 3; i < pixels.length; i += 4) assert.equal(pixels[i], 255);
});
