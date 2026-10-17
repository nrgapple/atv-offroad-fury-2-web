// Riding on the Arlington train: original route poses and COL bodies, the ATV
// placed on a car at the car's own velocity. Compares Extras → Train riding.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, waitForGame } from './browser/harness.mjs';
const browser = await launch();
try {
  const errors = [],
    page = await newPage(browser, { viewport: { width: 960, height: 600 } }, errors);
  await page.addInitScript(() => {
    const NativeWorker = Worker;
    let id = -1000;
    window.Worker = class extends NativeWorker {
      constructor(...args) {
        super(...args);
        window.rideWorker = this;
        this.pending = new Map();
        this.addEventListener('message', ({ data }) => {
          const p = this.pending.get(data.id);
          if (!p) return;
          this.pending.delete(data.id);
          data.error ? p.reject(Error(data.error)) : p.resolve(JSON.parse(new TextDecoder().decode(data.bytes)));
        });
      }
      postMessage(data, ...args) {
        if (data.op === 67 && data.bytes.length) window.rideResource = { bytes: data.bytes.slice(), index: data.index };
        super.postMessage(data, ...args);
      }
    };
    window.rideBinary = (op, bytes, index = 0) =>
      new Promise((resolve, reject) => {
        const key = id--;
        rideWorker.pending.set(key, { resolve, reject });
        rideWorker.postMessage({ id: key, op, index, bytes: bytes.slice() });
      });
    window.rideCommand = (op, value) => rideBinary(op, new TextEncoder().encode(value == null ? '' : JSON.stringify(value)));
  });
  await page.goto(`${base}/play.html?drive=1&environment=env08`);
  await waitForGame(page);
  // Time-based on purpose: let any in-flight game worker batch drain after pausing.
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Resume', exact: true }).waitFor();
  await page.waitForTimeout(100);
  const ride = test =>
    page.evaluate(async test => {
      const startup = await rideBinary(67, rideResource.bytes, rideResource.index);
      await rideCommand(23, { origin: startup.wheel_matrices[0].slice(12, 15), yaw: 0 });
      const installed = await rideCommand(69, {});
      if (installed.bodies !== 11) throw Error('Original train bodies missing');
      for (let i = 0; i < test.routeSteps - 1; i++) await rideCommand(68, { dt: 1 / 60 });
      const before = (await rideCommand(68, null)).cars[test.car].matrix;
      await rideCommand(68, { dt: 1 / 60 });
      const train = await rideCommand(68, null),
        car = train.cars[test.car],
        m = car.matrix;
      const toWorld = (m, l) => [0, 1, 2].map(i => m[12 + i] + m[i] * l[0] + m[4 + i] * l[1] + m[8 + i] * l[2]);
      const toLocal = (m, p) => {
        const d = [0, 1, 2].map(i => p[i] - m[12 + i]);
        return [0, 4, 8].map(c => (d[0] * m[c] + d[1] * m[c + 1] + d[2] * m[c + 2]) / Math.hypot(m[c], m[c + 1], m[c + 2]) ** 2);
      };
      const origin = toWorld(m, test.local);
      const velocity = test.carVelocity ? [0, 1, 2].map(i => (m[12 + i] - before[12 + i]) * 60) : [0, 0, 0];
      // Direction 0 faces the car's side, as in test_train_collisions.mjs.
      const forward = test.direction ? [m[8] * test.direction, m[10] * test.direction] : [-m[0], -m[2]];
      await rideCommand(23, { origin, yaw: Math.atan2(forward[0], forward[1]), velocity });
      const samples = [];
      for (let i = 0; i < test.batches; i++) {
        const frames = (
          await rideCommand(50, {
            count: 4,
            input: { throttle: test.throttle ?? 0, brake: test.brake ?? 0, steer: test.steer ?? 0, lean: 0, reverse: false, realistic_train: test.realistic },
            orbit: 0
          })
        ).map(s => s.frame);
        const f = frames.at(-1),
          cm = (await rideCommand(68, null)).cars[test.car].matrix;
        samples.push({ local: toLocal(cm, f.origin), along: f.linear_velocity[0] * cm[8] + f.linear_velocity[2] * cm[10], grounded: f.grounded.filter(Boolean).length, phase: f.wreck.phase, speed: Math.hypot(...f.linear_velocity), dynamic: Math.max(...frames.map(x => x.dynamic_contact_count)) });
      }
      return { model: car.model, trainSpeed: Math.hypot(...velocity), samples };
    }, test);
  // Largest horizontal car-local displacement from the first sample.
  const drift = r => {
    const [x, , z] = r.samples[0].local;
    return Math.max(...r.samples.map(s => Math.hypot(s.local[0] - x, s.local[2] - z)));
  };
  const cases = [
    // The flatbed through the long left curve (~12.5°/s at 44.7 ft/s, ~0.3 g).
    { name: 'curve-coast', car: 6, local: [0, 8, 0], direction: 1, routeSteps: 1980, batches: 90 },
    // Parked across the deck with the brakes on through the same curve.
    { name: 'curve-brake-across', car: 6, local: [0, 8, 0], direction: 0, routeSteps: 1980, batches: 90, brake: 1 }
  ];
  for (const test of cases) {
    const original = await ride({ ...test, realistic: false, carVelocity: true }),
      realistic = await ride({ ...test, realistic: true, carVelocity: true });
    for (const r of [original, realistic])
      assert(r.samples.every(s => s.local.every(Number.isFinite) && Number.isFinite(s.speed)));
    assert.equal(realistic.model, 'animgeometry\\flatbedA.slt');
    assert(realistic.trainSpeed > 40, `${test.name}: train is not moving`);
    assert(realistic.samples.every(s => s.phase === 'Riding'), `${test.name}: realistic rider wrecked`);
    assert.equal(realistic.samples.at(-1).grounded, 4, `${test.name}: realistic quad left the deck`);
    assert(drift(realistic) < 2, `${test.name}: realistic drifted ${drift(realistic).toFixed(1)} ft`);
    // Recovered world-frame grip and drag slide the quad off in the curve.
    assert(drift(original) > 10, `${test.name}: original drifted only ${drift(original).toFixed(1)} ft`);
    console.log(JSON.stringify({ name: test.name, trainSpeed: +realistic.trainSpeed.toFixed(1), originalDrift: +drift(original).toFixed(1), realisticDrift: +drift(realistic).toFixed(1), originalPhases: [...new Set(original.samples.map(s => s.phase))] }));
  }
  // Driving into the side of a passing car still produces body contact in both modes.
  for (const realistic of [false, true]) {
    const r = await ride({ car: 3, local: [12, 3, 0], direction: 0, routeSteps: 900, batches: 30, throttle: 0.4, realistic });
    assert(r.samples.some(s => s.dynamic > 0), 'no side contact');
    assert(r.samples.every(s => s.local.every(Number.isFinite)));
  }
  assertNoErrors(errors);
  console.log(
    'PASS: Extras realistic train riding keeps the quad on the moving flatbed through the curve, coasting or braked across the deck; original world-frame grip slides it off. Side contact still runs in both modes.'
  );
} finally {
  await browser.close();
}
