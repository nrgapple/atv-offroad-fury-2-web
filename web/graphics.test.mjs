import test from 'node:test';
import assert from 'node:assert/strict';
import { GraphicsSettings, VibrationSettings, DirtBuildup, Rumble, detectGraphicsTier, parseDirtZones, presetFeatures, rumbleStrength } from './graphics.js';

const store = (values = {}) => ({ values, getItem: k => values[k] ?? null, setItem(k, v) { values[k] = String(v); } });
const env = ({ ua = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', touch = 0, coarse = false, fine = true, cores = 8, memory = 8, mobile } = {}) => ({
  navigator: { userAgent: ua, maxTouchPoints: touch, hardwareConcurrency: cores, deviceMemory: memory, userAgentData: mobile === undefined ? undefined : { mobile } },
  matchMedia: q => ({ matches: q === '(pointer: coarse)' ? coarse : q === '(any-pointer: fine)' ? fine : false }),
});

test('Auto resolves Enhanced on phones, tablets, touch-primary and low-power devices; High on desktop', () => {
  assert.equal(detectGraphicsTier(env()), 'high');
  assert.equal(detectGraphicsTier(env({ ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Mobile/15E148' })), 'enhanced');
  assert.equal(detectGraphicsTier(env({ ua: 'Mozilla/5.0 (Linux; Android 15; Pixel 9) Mobile' })), 'enhanced');
  assert.equal(detectGraphicsTier(env({ touch: 5 })), 'enhanced', 'iPadOS desktop user agent with multi-touch');
  assert.equal(detectGraphicsTier(env({ coarse: true, fine: false })), 'enhanced');
  assert.equal(detectGraphicsTier(env({ coarse: true, fine: true })), 'high', 'touch laptop with a mouse');
  assert.equal(detectGraphicsTier(env({ cores: 4 })), 'enhanced');
  assert.equal(detectGraphicsTier(env({ memory: 2 })), 'enhanced');
  assert.equal(detectGraphicsTier(env({ mobile: true })), 'enhanced');
  assert.equal(detectGraphicsTier({}), 'high', 'missing APIs count as desktop');
  assert.equal(detectGraphicsTier({ navigator: { hardwareConcurrency: 0 }, matchMedia() { throw new Error('x'); } }), 'high');
});

test('presets persist, cycle in both directions and report the Auto resolution', () => {
  const s = store(), phone = env({ coarse: true, fine: false });
  const g = new GraphicsSettings(s, phone);
  assert.equal(g.preset, 'auto'); assert.equal(g.resolved, 'enhanced'); assert.equal(g.label, 'Auto (Enhanced)');
  let changes = 0; g.onChange = () => changes++;
  g.cycle(); assert.equal(g.preset, 'original'); assert.equal(s.values['atv-graphics'], 'original');
  g.cycle(); g.cycle(); assert.equal(g.preset, 'high'); assert.equal(g.label, 'High');
  g.cycle(); assert.equal(g.preset, 'auto');
  g.cycle(-1); assert.equal(g.preset, 'high'); assert.equal(changes, 5);
  assert.equal(new GraphicsSettings(s, phone).preset, 'high', 'reload keeps the saved choice');
  assert.equal(new GraphicsSettings(store({ 'atv-graphics': 'ultra' }), env()).resolved, 'high', 'unknown values fall back to Auto');
  assert.equal(new GraphicsSettings(null, env()).resolved, 'high');
  const throwing = { getItem() { throw new Error('denied'); }, setItem() { throw new Error('denied'); } };
  const session = new GraphicsSettings(throwing, env()); session.cycle(); assert.equal(session.preset, 'original');
});

test('Original enables nothing; Enhanced caps anisotropy at 4x and High at 16x', () => {
  assert.deepEqual(presetFeatures('original'), { preset: 'original', interpolation: false, anisotropy: 0, contactShadows: false, dirt: false });
  assert.equal(presetFeatures('bogus').preset, 'original');
  assert.equal(presetFeatures('enhanced').anisotropy, 4);
  assert.equal(presetFeatures('high').anisotropy, 16);
  for (const p of ['enhanced', 'high']) { const f = presetFeatures(p); assert(f.interpolation && f.contactShadows && f.dirt); }
});

test('vibration defaults on and persists Off', () => {
  const s = store();
  const v = new VibrationSettings(s); assert.equal(v.enabled, true);
  v.toggle(); assert.equal(v.enabled, false); assert.equal(s.values['atv-vibration'], 'off');
  assert.equal(new VibrationSettings(s).enabled, false);
  assert.equal(new VibrationSettings(null).enabled, true);
});

test('rumble scales with impact, prefers the controller and rate-limits one impact', () => {
  assert.equal(rumbleStrength(10), 0); assert.equal(rumbleStrength(NaN), 0);
  assert(rumbleStrength(20) > 0 && rumbleStrength(20) < rumbleStrength(50)); assert.equal(rumbleStrength(500), 1);
  let time = 0; const effects = [], buzzes = [];
  const pad = { vibrationActuator: { playEffect: (type, params) => { effects.push([type, params]); return Promise.resolve(); } } };
  const r = new Rumble({ pad: () => pad, vibrate: ms => buzzes.push(ms), touch: () => true, now: () => time });
  assert(r.pulse(.5)); assert.equal(effects[0][0], 'dual-rumble'); assert.equal(effects[0][1].strongMagnitude, .5);
  assert.equal(buzzes.length, 0, 'a controller rumble replaces phone vibration');
  time = 10; assert(!r.pulse(.5), 'same impact bouncing'); assert(r.pulse(.9), 'stronger hit upgrades');
  time = 1000; assert(r.pulse(.4));
  const phone = new Rumble({ pad: () => null, vibrate: ms => { buzzes.push(ms); return true; }, touch: () => true, now: () => time });
  assert(!phone.pulse(.2), 'weak hits do not buzz the phone'); assert(phone.pulse(.8)); assert.equal(buzzes.length, 1);
  const desktop = new Rumble({ pad: () => null, vibrate: () => true, touch: () => false, now: () => time });
  assert(!desktop.pulse(1), 'no vibration without a controller or touch');
  const riding = { reset_count: 0, wreck: { phase: 'Riding' } }, wreck = { reset_count: 0, wreck: { phase: 'Ejected' }, crash_impact_speed: 0 };
  const w = new Rumble({ pad: () => pad, now: () => time });
  assert(!w.accept(riding, riding, 0, true)); assert(w.accept(riding, wreck, 0, true)); assert(w.last.strength >= .7);
  assert(!new Rumble({ pad: () => pad }).accept(riding, wreck, 80, false), 'disabled setting sends nothing');
  assert(!new Rumble({ pad: () => pad }).accept({ ...riding, reset_count: 1 }, wreck, 0, true), 'a reset is not a wreck');
});

const toy = `[Other]\nDirtAnimChangeTime=9\n[TerrainZone1]\nSurfaceDrag = 0.0\nGenerateDirt=T\nDirtAnimChangeTime=0.4\nDirtAnimChangeAmount=1\n;DirtAnimChangeAmount=7\n[TerrainZone2]\nGenerateDirt=T\n[TerrainZone3]\nDirtAnimChangeTime = 1.5 ; ice\nDirtAnimChangeAmount = -2\n[TerrainZone9]\nDirtAnimChangeTime=1\n`;
test('original TOY dirt zone fields are read per terrain zone', () => {
  const zones = parseDirtZones(toy);
  assert.equal(zones.length, 8);
  assert.deepEqual(zones[0], { time: .4, amount: 1 });
  assert.deepEqual(zones[1], { time: 0, amount: 0 });
  assert.deepEqual(zones[2], { time: 1.5, amount: -2 });
  assert.deepEqual(zones[7], { time: 0, amount: 0 });
  assert.equal(parseDirtZones(null)[0].time, 0);
});

const frame = ({ speed = 30, zone = 0, grounded = [true, true, true, true], water = -256, reset = 0, mud = false } = {}) => ({
  reset_count: reset, speed_magnitude: speed, grounded,
  wheel_effects: { surfaces: [zone, zone, zone, zone], mud: [mud, mud, mud, mud], centers: [[0, 1, 0], [0, 1, 0], [0, 1, 0], [0, 1, 0]], radii: [.8, .8, .8, .8], water_heights: [water, water, water, water] },
});
const run = (dirt, f, steps) => { for (let i = 0; i < steps; i++) dirt.step(f); return dirt.level; };
test('dirt builds above 10 mph on dirty zones, washes off in water and clears on a player reset', () => {
  const dirt = new DirtBuildup(parseDirtZones(toy));
  assert.equal(run(dirt, frame(), 60), 2, 'one pass per 0.4s on zone 1');
  assert.equal(run(dirt, frame({ speed: 14 }), 60), 2, 'at or below 10 mph nothing changes');
  assert.equal(run(dirt, frame({ grounded: [false, false, false, false] }), 60), 2, 'airborne');
  assert.equal(run(dirt, frame({ zone: 1 }), 600), 2, 'zone without dirt fields');
  run(dirt, frame(), 60 * 120); assert.equal(dirt.level, 255); assert.equal(dirt.amount, 1);
  run(dirt, frame({ zone: 2 }), 100); assert.equal(dirt.level, 253, 'negative amounts clean');
  run(dirt, frame({ water: 2 }), 60); assert(dirt.level < 253 - 60, 'water washes the quad');
  run(dirt, frame({ water: 2 }), 600); assert.equal(dirt.level, 0);
  run(dirt, frame({ mud: true, zone: 1 }), 60); assert.equal(dirt.level, 10, 'MUD scenery dirties quickly');
  assert.equal(dirt.step(frame({ reset: 1, speed: 0 })), 10 / 255, 'wreck recovery keeps the dirt');
  dirt.reset(); assert.equal(dirt.level, 0, 'a player reset cleans the quad');
  assert.equal(new DirtBuildup().step(frame()), 0, 'maps without dirt fields stay clean');
  assert.equal(new DirtBuildup().step(null), 0);
});
