import test from 'node:test';
import assert from 'node:assert/strict';
import { HypeDirector, SLOWMO, heavyMudZones } from './hype.js';
import { BurstEmitter } from './hype-gl.js';
import { AlphaSettings, ALPHA_FEATURES } from './alpha.js';
import { DirtBuildup } from './graphics.js';

const flat = () => 0;
const frame = (y, vy, grounded, extra = {}) => ({ origin: [0, y, 0], linear_velocity: [0, vy, 30], grounded: [grounded, grounded, grounded, grounded],
  wreck: { phase: 'Riding' }, reset_count: 0, speed_magnitude: 30, ...extra });
// A ballistic jump from the ground: launch speed vy (ft/s), gravity 32 ft/s².
function jump(director, vy0, { onFrame = () => {}, real = 1 / 60 } = {}) {
  director.step(frame(1, 0, true), flat);
  let y = 1, vy = vy0;
  for (let i = 0; i < 600; i++) {
    vy -= 32 / 60; y += vy / 60;
    const grounded = y <= 1;
    director.step(frame(Math.max(1, y), vy, grounded), flat);
    onFrame(director.update(real, { slowmo: true }), director);
    if (grounded) break;
  }
  return director;
}

test('small hops stay ordinary: no apex, slow motion, blur or framing change', () => {
  const d = new HypeDirector(); let maxAir = 0;
  jump(d, 12, { onFrame: (_, h) => { maxAir = Math.max(maxAir, h.air); } });
  assert.equal(d.apexes, 0); assert.equal(d.slowmos, 0); assert.equal(d.timeScale, 1);
  assert(maxAir < .01, `air ${maxAir}`);
  const f = { position: [0, 5, -10], target: [0, 2, 0], fov: 60 };
  d.dip = 0; d.dipVelocity = 0; d.air = 0;
  assert.equal(d.camera(f, flat), f);
});

test('a big jump fires one apex slow motion that returns to full speed before touchdown', () => {
  const d = new HypeDirector(); const scales = []; let landed = null;
  jump(d, 40, { onFrame: (events, h) => { scales.push(h.timeScale); if (events.landed) landed = events.landed; } });
  assert.equal(d.apexes, 1); assert.equal(d.slowmos, 1);
  assert(Math.min(...scales) <= SLOWMO.scale + .01);
  assert(landed && landed.severity > 30 && landed.peak > 20);
  assert.equal(d.timeScale, 1);
  assert(d.dipVelocity < 0 || d.dip < 0, 'landing compresses the camera');
});

test('slow motion is skipped when disabled and big-air framing still closes in', () => {
  const d = new HypeDirector(); let maxAir = 0;
  d.step(frame(1, 0, true), flat);
  let y = 1, vy = 40;
  for (let i = 0; i < 120; i++) { vy -= 32 / 60; y += vy / 60; d.step(frame(y, vy, false), flat); d.update(1 / 60, { slowmo: false }); maxAir = Math.max(maxAir, d.air); }
  assert.equal(d.slowmos, 0); assert.equal(d.timeScale, 1); assert(maxAir > .5);
  const f = { position: [0, 30, -10], target: [0, 26, 0], fov: 60 }, shot = d.camera(f, flat);
  assert(Math.hypot(...shot.position.map((v, i) => v - shot.target[i])) < .8 * Math.hypot(...f.position.map((v, i) => v - f.target[i])));
  assert(d.blur > .5, 'big air darkens and blurs the edges');
  assert(shot.fov > f.fov);
  assert(d.muffle > .4 && d.wind > .4, 'big air muffles music and raises wind');
});

test('camera never drops under the terrain floor', () => {
  const d = new HypeDirector(); d.air = 1; d.dip = -3;
  const shot = d.camera({ position: [0, 3, -10], target: [0, 2, 0], fov: 60 }, () => 2.5);
  assert(shot.position[1] >= 2.5 + 2 - 3 - 1e-9);
});

test('a reset or teleport clears jump state', () => {
  const d = new HypeDirector();
  d.step(frame(1, 0, true), flat); d.step(frame(20, 5, false), flat);
  d.step(frame(20, 5, false, { reset_count: 1 }), flat);
  assert.equal(d.airTime, 0); assert.equal(d.peak, 0);
});

test('landing burst scales with severity and ignores soft landings', () => {
  const e = new BurstEmitter(64);
  assert.equal(e.burst([0, 0, 0], 8, 0, [.5, .5, .5]), 0);
  const soft = e.burst([0, 0, 0], 20, 0, [.5, .5, .5]); e.reset();
  const hard = e.burst([0, 0, 0], 60, 0, [.5, .5, .5]);
  assert(hard > soft && soft > 0);
  assert.equal(e.live, Math.min(64, hard));
});

test('alpha settings persist known ids only and default every feature off', () => {
  const values = new Map(), store = { getItem: k => values.get(k), setItem: (k, v) => values.set(k, v) };
  const a = new AlphaSettings(store);
  assert(a.rows().every(r => r.value === 'Off')); assert.equal(a.rows().length, ALPHA_FEATURES.length);
  let changed = null; a.onChange = (id, on) => { changed = [id, on]; };
  a.toggle('air-slowmo'); a.toggle('nope');
  assert.deepEqual(changed, ['air-slowmo', true]);
  assert(new AlphaSettings(store).enabled('air-slowmo'));
  values.set('atv-alpha', '{bad'); assert.equal(new AlphaSettings(store).on.size, 0);
  assert(ALPHA_FEATURES.every(f => /^[a-z0-9-]+$/.test(f.id) && f.label.length <= 48));
});

test('heavy mud soils faster, dusty zones soil, and turning it off restores the source rule', () => {
  const zones = Array.from({ length: 8 }, () => ({ time: 0, amount: 0, tireTrack: false, dust: false }));
  zones[0] = { time: 1, amount: 2, tireTrack: true, dust: true }; zones[1] = { time: 0, amount: 0, tireTrack: false, dust: true };
  const heavy = heavyMudZones(zones);
  assert.equal(heavy[0].time, .25); assert.equal(heavy[1].amount, 1); assert.deepEqual(heavy[2], zones[2]);
  const dirt = new DirtBuildup(zones);
  dirt.setHeavy(true, heavyMudZones); assert.equal(dirt.rate, 3); assert.equal(dirt.zones[1].amount, 1);
  dirt.splat(400); assert.equal(dirt.level, 255);
  dirt.setHeavy(false, heavyMudZones); assert.equal(dirt.zones, zones); assert.equal(dirt.rate, 1);
});

test('rolling off a hillside never counts as big air, however far the ground drops', () => {
  const d = new HypeDirector(); let maxAir = 0, maxBlur = 0;
  // Ground falls away at 40 ft/s while the quad drops from rest, no launch.
  const ground = { y: 0 }, height = () => ground.y;
  d.step(frame(1, 0, true), height);
  let y = 1, vy = 0;
  for (let i = 0; i < 75; i++) {
    vy -= 32 / 60; y += vy / 60; ground.y -= 40 / 60;
    d.step(frame(y, vy, false), height); d.update(1 / 60, { slowmo: true });
    maxAir = Math.max(maxAir, d.air); maxBlur = Math.max(maxBlur, d.blur);
  }
  assert(d.peak > 12, `clearance grew to ${d.peak}`);
  assert.equal(d.apexes, 0); assert.equal(d.slowmos, 0); assert(maxAir < .01 && maxBlur < .01, `${maxAir} ${maxBlur}`);
});

test('a medium kicker gets no slow motion and little framing', () => {
  const d = new HypeDirector(); let maxAir = 0;
  jump(d, 22, { onFrame: (_, h) => { maxAir = Math.max(maxAir, h.air); } });
  assert.equal(d.slowmos, 0); assert(maxAir < .2, `air ${maxAir}`);
});
