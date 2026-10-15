import test from 'node:test';
import assert from 'node:assert/strict';
import { MinimapSettings, mapPeerMarker, mapHeading, mapPoint, mapPose, mapRange, terrainTiles, rasterRow, instrumentFooter } from './minimap.js';

test('footer lettering and black outline fit the circular edge at minimum and maximum sizes', () => {
  for (const size of [96, 160, 224]) for (const width of [size * .3, size * .55]) {
    const descent = 3, baseline = instrumentFooter(size, 'circle', width, descent);
    const radius = size / 2;
    assert((width / 2 + 2) ** 2 + (baseline + descent + 2 - radius) ** 2 < radius ** 2);
    assert(baseline > size * .8, 'Speed stays near the bottom edge');
    assert.equal(instrumentFooter(size, 'square', width, descent), size - 7);
  }
});

test('overhead camera projection preserves handedness and matches quaternion forward', () => {
  const terrain = { sizeX: 100, sizeZ: 200 };
  assert.deepEqual(mapPoint(0, 200, terrain, 400, 400), [0, 0]);
  assert.deepEqual(mapPoint(100, 0, terrain, 400, 400), [400, 400]);
  assert.deepEqual(mapPoint(50, 100, terrain, 400, 400), [200, 200]);
  // Source camera convention: screen-right=+X, screen-up=+Z.
  const right = [1, 0, 0], north = [0, 0, 1];
  for (const yaw of [0, Math.PI / 2, Math.PI, -Math.PI / 2, .7]) {
    const q = [Math.cos(yaw / 2), 0, Math.sin(yaw / 2), 0];
    const forward = [Math.sin(yaw), 0, Math.cos(yaw)];
    const dot = (a, b) => a.reduce((sum, v, i) => sum + v * b[i], 0);
    const expected = Math.atan2(dot(forward, right), dot(forward, north));
    assert(Math.abs(mapHeading(q) - expected) < 1e-12);
    const p = mapPoint(50 + forward[0], 100 + forward[2], terrain, 100, 200);
    const heading = mapHeading(q), dx = p[0] - 50, dy = p[1] - 100;
    assert(Math.abs(dx * Math.cos(heading) + dy * Math.sin(heading)) < 1e-12);
    assert(-dx * Math.sin(heading) + dy * Math.cos(heading) < 0, 'Heading-up forward is screen-up');
  }
});

test('GPS uses rendered vehicle, native separated rider and world-space ragdoll roots', () => {
  const vehicle = { origin: [1, 2, 3], orientation: [1, 0, 0, 0] };
  const matrix = [0, 0, -1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 40, 5, 60, 1];
  assert.deepEqual(mapPose(vehicle), { origin: vehicle.origin, heading: 0, actor: 'vehicle' });
  assert.deepEqual(mapPose({ ...vehicle, rider_world_matrix: matrix }), { origin: [40, 5, 60], heading: Math.PI / 2, actor: 'rider' });
  const root = [...matrix]; root[12] = 80;
  assert.deepEqual(mapPose({ ...vehicle, rider_world_matrix: matrix, funny_crash: { rider_node_matrices: [root] } }), { origin: [80, 5, 60], heading: Math.PI / 2, actor: 'ragdoll rider' });
});

test('saved Off and clamped sizes survive reload; blocked storage still allows session changes', () => {
  const entries = new Map(), storage = { getItem: key => entries.get(key) ?? null, setItem: (key, value) => entries.set(key, value) };
  const settings = new MinimapSettings(storage);
  assert.equal(settings.enabled, true); assert.equal(settings.size, 160);
  assert.equal(settings.zoom, 50); assert.equal(mapRange(settings.zoom), 1536);
  assert.equal(settings.shape, 'circle'); assert.equal(settings.corner, 'top-right');
  assert.equal(settings.orientation, 'north-up'); settings.toggleOrientation();
  settings.toggleShape(); settings.toggleCorner();
  settings.toggle(); settings.setSize(900); settings.setZoom(900);
  const reloaded = new MinimapSettings(storage);
  assert.equal(reloaded.enabled, false); assert.equal(reloaded.size, 224);
  assert.equal(reloaded.zoom, 200); reloaded.setZoom(-10); assert.equal(reloaded.zoom, 25);
  reloaded.setZoom('bad'); assert.equal(reloaded.zoom, 50);
  assert.equal(reloaded.size, 224, 'Zoom does not change HUD size');
  for (const [zoom, range] of [[25, 3072], [50, 1536], [100, 768], [200, 384]]) assert.equal(mapRange(zoom), range);
  assert.equal(reloaded.shape, 'square'); assert.equal(reloaded.corner, 'bottom-left');
  assert.equal(reloaded.orientation, 'heading-up');
  reloaded.setSize('bad'); assert.equal(reloaded.size, 160);
  const blocked = new MinimapSettings({ getItem() { throw Error(); }, setItem() { throw Error(); } });
  blocked.toggle(); blocked.setSize(96); blocked.setZoom(75);
  assert.equal(blocked.zoom, 75);
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
  assert(pixels[4] > pixels[0], 'Positive X is screen-right, matching the source camera');
  for (let i = 3; i < pixels.length; i += 4) assert.equal(pixels[i], 255);
});

test('remote markers share scene handedness, zoom, rider tracking and heading-up rotation', () => {
  const center = { origin: [100, 0, 100], heading: Math.PI / 2 };
  const matrix = [0,0,-1,0, 0,1,0,0, 1,0,0,0, 148,0,100,1];
  const peer = { position: [900,0,900], rider_world_matrix: matrix };
  const north = mapPeerMarker(peer, center, mapRange(100), 160, false);
  assert.equal(north.dx, 10); assert.equal(Math.abs(north.dy), 0);
  assert.equal(north.rotation, Math.PI/2); assert.equal(north.edge, false);
  assert.equal(mapPeerMarker(peer, center, mapRange(50), 160, false).dx, 5);
  const heading = mapPeerMarker(peer, center, mapRange(100), 160, true);
  assert(Math.abs(heading.dx)<1e-12); assert.equal(heading.dy,-10); assert.equal(heading.rotation,0);
  const far = mapPeerMarker({position:[10000,0,100]},center,768,160,false);
  assert.equal(far.edge,true); assert.equal(far.dx,64);
});
