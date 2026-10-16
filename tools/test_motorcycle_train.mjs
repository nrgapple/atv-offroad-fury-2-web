// Motorcycle encounters with the original Arlington train bodies: the bike's
// chassis whiskers and tires query the same partitioned scene as the ATV.
// The setup only places the bike; it never injects velocity, impulses or wrecks.
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
        window.encounterWorker = this;
        this.pending = new Map();
        this.addEventListener('message', ({ data }) => {
          const p = this.pending.get(data.id);
          if (!p) return;
          this.pending.delete(data.id);
          data.error ? p.reject(Error(data.error)) : p.resolve(JSON.parse(new TextDecoder().decode(data.bytes)));
        });
      }
      postMessage(data, ...args) {
        if (data.op === 67 && data.bytes.length)
          window.encounterResource = { bytes: data.bytes.slice(), index: data.index };
        super.postMessage(data, ...args);
      }
    };
    window.encounterBinary = (op, bytes, index = 0) =>
      new Promise((resolve, reject) => {
        const key = id--;
        encounterWorker.pending.set(key, { resolve, reject });
        encounterWorker.postMessage({ id: key, op, index, bytes: bytes.slice() });
      });
    window.encounterCommand = (op, value) =>
      encounterBinary(op, new TextEncoder().encode(value == null ? '' : JSON.stringify(value)));
  });
  await page.goto(`${base}/play.html?drive=1&environment=env08&quad=${process.env.ATV_MOTORCYCLE_ID || 29}`);
  await waitForGame(page);
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.frame.vehicle_kind), 'motorcycle');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Resume', exact: true }).waitFor();
  await page.waitForTimeout(100);
  const cases = [
    { name: 'side-wreck', local: [17, 7, -20], direction: 0, throttle: 1, routeSteps: 30 },
    { name: 'top-drop', local: [0, 25, -10], direction: 1, throttle: 0, routeSteps: 30 }
  ];
  for (const test of cases) {
    const frames = await page.evaluate(async test => {
      const startup = await encounterBinary(67, encounterResource.bytes, encounterResource.index);
      await encounterCommand(23, { origin: startup.wheel_matrices[0].slice(12, 15), yaw: 0 });
      const installed = await encounterCommand(69, {});
      if (installed.bodies !== 11) throw Error('Original train bodies missing');
      for (let i = 0; i < test.routeSteps; i++) await encounterCommand(68, { dt: 1 / 60 });
      const train = await encounterCommand(68, null),
        m = train.cars[0].matrix;
      const origin = [0, 1, 2].map(
        i => m[12 + i] + m[i] * test.local[0] + m[4 + i] * test.local[1] + m[8 + i] * test.local[2]
      );
      const forward = test.direction ? [m[8] * test.direction, m[10] * test.direction] : [-m[0], -m[2]];
      await encounterCommand(23, { origin, yaw: Math.atan2(forward[0], forward[1]) });
      const frames = [];
      for (let i = 0; i < 180; i++)
        frames.push(
          ...(
            await encounterCommand(50, {
              count: 4,
              input: { throttle: test.throttle, brake: 0, steer: 0, lean: 0, reverse: false },
              orbit: 0
            })
          ).map(s => s.frame)
        );
      return frames;
    }, test);
    if (process.env.ATV_TRAIN_DEBUG)
      console.log(JSON.stringify(frames.filter((_, i) => i % 20 === 0).slice(0, 14).map(f => ({ o: f.origin.map(Math.round), v: Math.round(f.speed), c: f.chassis_contact_count, d: f.dynamic_contact_count, m: f.moving_support, sc: f.scene_candidates, dc: f.dynamic_candidates, g: f.grounded, w: f.wreck.phase }))));
    assert(frames.every(f => f.origin.every(Number.isFinite) && f.linear_velocity.every(Number.isFinite)));
    const contact = frames.findIndex(f => f.dynamic_contact_count > 0 || f.moving_support);
    assert(contact >= 0, `${test.name}: the bike never touched the train`);
    if (test.name === 'side-wreck') {
      assert(frames.some(f => f.dynamic_contact_count > 0), 'side: no chassis contact with the train');
      assert(frames.some(f => f.wreck.phase !== 'Riding'), 'side: a broadside train hit must wreck');
    } else assert(frames.some(f => f.moving_support), 'top: tires must stand on the moving train');
    console.log(
      JSON.stringify({
        name: test.name,
        contact,
        dynamicContacts: Math.max(...frames.map(f => f.dynamic_contact_count)),
        movingSupport: frames.filter(f => f.moving_support).length,
        phases: [...new Set(frames.map(f => f.wreck.phase))]
      })
    );
  }
  assertNoErrors(errors);
  console.log('PASS: motorcycle chassis and tires collide with the moving original train bodies; a broadside hit wrecks.');
} finally {
  await browser.close();
}
